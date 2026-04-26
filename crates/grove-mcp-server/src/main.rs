mod errors;
mod tools;

use rusqlite::Connection;
use serde_json::{json, Value};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

const SERVER_NAME: &str = "grove-mcp-server";
const SERVER_VERSION: &str = "0.1.0";
const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerMode {
    Graph,
    Run,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let (db_path, mode) = parse_args();
    let conn = open_db(&db_path);

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = io::stdout();

    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim().to_owned();
        if trimmed.is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&trimmed) {
            Ok(v) => v,
            Err(e) => {
                let err_response = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": format!("Parse error: {e}")
                    }
                });
                write_response(&mut stdout, &err_response).await;
                continue;
            }
        };

        let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = request.get("id").cloned();

        // Notifications (no id) don't get responses
        let is_notification = id.is_none() || id.as_ref().map(|v| v.is_null()).unwrap_or(true);

        match method {
            "initialize" => {
                let response = handle_initialize(&request);
                write_response(&mut stdout, &response).await;
            }
            "notifications/initialized" => {
                // Acknowledgement notification — no response needed
            }
            "tools/list" => {
                let response = handle_tools_list(&request, mode);
                write_response(&mut stdout, &response).await;
            }
            "tools/call" => {
                let response = handle_tools_call(&conn, &request, mode).await;
                write_response(&mut stdout, &response).await;
            }
            "ping" => {
                if !is_notification {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {}
                    });
                    write_response(&mut stdout, &response).await;
                }
            }
            _ => {
                if !is_notification {
                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": format!("Method not found: {method}")
                        }
                    });
                    write_response(&mut stdout, &response).await;
                }
            }
        }
    }
}

fn parse_args() -> (String, ServerMode) {
    let args: Vec<String> = std::env::args().collect();
    let mut db_path: Option<String> = None;
    let mut mode = ServerMode::Graph;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--db-path" {
            if i + 1 < args.len() {
                db_path = Some(args[i + 1].clone());
                i += 2;
                continue;
            } else {
                eprintln!("grove-mcp-server: --db-path requires a value");
                std::process::exit(1);
            }
        }
        if args[i] == "--mode" {
            if i + 1 < args.len() {
                mode = match args[i + 1].as_str() {
                    "graph" => ServerMode::Graph,
                    "run" => ServerMode::Run,
                    other => {
                        eprintln!("grove-mcp-server: unsupported --mode value: {other}");
                        std::process::exit(1);
                    }
                };
                i += 2;
                continue;
            } else {
                eprintln!("grove-mcp-server: --mode requires a value");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    match db_path {
        Some(p) => (p, mode),
        None => {
            eprintln!("grove-mcp-server: --db-path <path> is required");
            std::process::exit(1);
        }
    }
}

fn open_db(db_path: &str) -> Connection {
    let conn = Connection::open(db_path).unwrap_or_else(|e| {
        eprintln!("grove-mcp-server: failed to open database at {db_path}: {e}");
        std::process::exit(1);
    });

    // Enable WAL mode for concurrent read access with the main app
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .unwrap_or_else(|e| {
            eprintln!("grove-mcp-server: failed to set PRAGMA: {e}");
            std::process::exit(1);
        });

    conn
}

async fn write_response(stdout: &mut io::Stdout, response: &Value) {
    let serialized = serde_json::to_string(response).expect("failed to serialize JSON response");
    let line = format!("{serialized}\n");
    let _ = stdout.write_all(line.as_bytes()).await;
    let _ = stdout.flush().await;
}

// ── MCP Protocol Handlers ───────────────────────────────────────────────────

fn handle_initialize(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
            }
        }
    })
}

fn handle_tools_list(request: &Value, mode: ServerMode) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let tool_defs = tools::tool_definitions(mode == ServerMode::Run);

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tool_defs
        }
    })
}

async fn handle_tools_call(conn: &Connection, request: &Value, mode: ServerMode) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);

    let params = request.get("params").unwrap_or(&Value::Null);
    let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if tool_name.is_empty() {
        return json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32602,
                "message": "Invalid params: missing tool name"
            }
        });
    }

    match tools::dispatch(conn, tool_name, &arguments, mode == ServerMode::Run).await {
        Ok(result) => {
            let content_text =
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": content_text
                        }
                    ]
                }
            })
        }
        Err(e) => {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [
                        {
                            "type": "text",
                            "text": format!("Error: {}", e)
                        }
                    ],
                    "isError": true
                }
            })
        }
    }
}
