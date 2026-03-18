use std::collections::HashMap;

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, put};
use axum::{Json, Router};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct DatabaseEntry {
    id: String,
    name: String,
    path: String,
}

#[derive(Serialize)]
struct ColumnInfo {
    cid: i32,
    name: String,
    col_type: String,
    notnull: bool,
    default_value: Option<String>,
    pk: bool,
}

#[derive(Serialize)]
struct TableRows {
    columns: Vec<ColumnInfo>,
    rows: Vec<HashMap<String, serde_json::Value>>,
    total: i64,
    page: i64,
    page_size: i64,
}

#[derive(Deserialize)]
struct DbQuery {
    db: String,
}

#[derive(Deserialize)]
struct RowsQuery {
    db: String,
    #[serde(default = "default_page")]
    page: i64,
    #[serde(default = "default_size")]
    size: i64,
    #[serde(default)]
    sort: Option<String>,
    #[serde(default)]
    order: Option<String>,
}

fn default_page() -> i64 {
    1
}
fn default_size() -> i64 {
    50
}

#[derive(Deserialize)]
struct UpdateBody {
    db: String,
    pk_column: String,
    updates: HashMap<String, serde_json::Value>,
}

type AppError = (StatusCode, String);

// ── Helpers ──────────────────────────────────────────────────────────────────

fn open_db(path: &str) -> Result<Connection, AppError> {
    Connection::open(path).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to open database: {e}"),
        )
    })
}

fn get_columns(conn: &Connection, table: &str) -> Result<Vec<ColumnInfo>, AppError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info(\"{}\")", table.replace('"', "")))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let cols = stmt
        .query_map([], |row| {
            Ok(ColumnInfo {
                cid: row.get(0)?,
                name: row.get(1)?,
                col_type: row.get(2)?,
                notnull: row.get::<_, bool>(3)?,
                default_value: row.get(4)?,
                pk: row.get::<_, bool>(5)?,
            })
        })
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(cols)
}

fn sanitize_table(name: &str) -> String {
    // Only allow alphanumeric and underscores to prevent SQL injection
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn list_databases() -> Result<impl IntoResponse, AppError> {
    let home = dirs::home_dir()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "No home dir".into()))?;

    let workspaces_dir = home.join(".grove").join("workspaces");
    let mut databases = Vec::new();

    if workspaces_dir.exists() {
        let pattern = workspaces_dir
            .join("*")
            .join(".grove")
            .join("grove.db")
            .to_string_lossy()
            .to_string();

        if let Ok(paths) = glob::glob(&pattern) {
            for entry in paths.flatten() {
                let id = entry
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".into());

                let db_path_str = entry.to_string_lossy().to_string();

                // Try to read project name from the DB
                let name = Connection::open(&entry)
                    .ok()
                    .and_then(|conn| {
                        conn.query_row(
                            "SELECT name FROM projects ORDER BY created_at DESC LIMIT 1",
                            [],
                            |row| row.get::<_, String>(0),
                        )
                        .ok()
                    })
                    .unwrap_or_else(|| id.clone());

                databases.push(DatabaseEntry {
                    id,
                    name,
                    path: db_path_str,
                });
            }
        }
    }

    Ok(Json(databases))
}

async fn list_tables(Query(q): Query<DbQuery>) -> Result<impl IntoResponse, AppError> {
    let conn = open_db(&q.db)?;

    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(tables))
}

async fn get_schema(
    Path(table): Path<String>,
    Query(q): Query<DbQuery>,
) -> Result<impl IntoResponse, AppError> {
    let conn = open_db(&q.db)?;
    let cols = get_columns(&conn, &table)?;
    Ok(Json(cols))
}

async fn get_rows(
    Path(table): Path<String>,
    Query(q): Query<RowsQuery>,
) -> Result<impl IntoResponse, AppError> {
    let conn = open_db(&q.db)?;
    let safe_table = sanitize_table(&table);
    let columns = get_columns(&conn, &safe_table)?;

    if columns.is_empty() {
        return Err((StatusCode::NOT_FOUND, format!("Table '{safe_table}' not found")));
    }

    // Total count
    let total: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM \"{safe_table}\""), [], |r| {
            r.get(0)
        })
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Build ORDER BY
    let order_clause = if let Some(ref sort_col) = q.sort {
        let safe_col = sanitize_table(sort_col);
        let dir = match q.order.as_deref() {
            Some("desc") => "DESC",
            _ => "ASC",
        };
        format!("ORDER BY \"{safe_col}\" {dir}")
    } else {
        String::new()
    };

    let offset = (q.page - 1) * q.size;
    let sql = format!(
        "SELECT * FROM \"{safe_table}\" {order_clause} LIMIT {limit} OFFSET {offset}",
        limit = q.size
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let col_names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
    let col_count = col_names.len();

    let rows = stmt
        .query_map([], |row| {
            let mut map = HashMap::new();
            for (i, name) in col_names.iter().enumerate().take(col_count) {
                let val = sqlite_value_to_json(row, i);
                map.insert(name.clone(), val);
            }
            Ok(map)
        })
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(TableRows {
        columns,
        rows,
        total,
        page: q.page,
        page_size: q.size,
    }))
}

fn sqlite_value_to_json(row: &rusqlite::Row, idx: usize) -> serde_json::Value {
    // Try types in order: integer, real, text, blob, null
    if let Ok(v) = row.get::<_, i64>(idx) {
        return serde_json::Value::Number(v.into());
    }
    if let Ok(v) = row.get::<_, f64>(idx) {
        return serde_json::Number::from_f64(v)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.get::<_, String>(idx) {
        return serde_json::Value::String(v);
    }
    if let Ok(v) = row.get::<_, Vec<u8>>(idx) {
        return serde_json::Value::String(format!("<blob {} bytes>", v.len()));
    }
    serde_json::Value::Null
}

async fn update_row(
    Path((table, pk_value)): Path<(String, String)>,
    Json(body): Json<UpdateBody>,
) -> Result<impl IntoResponse, AppError> {
    let conn = open_db(&body.db)?;
    let safe_table = sanitize_table(&table);
    let safe_pk_col = sanitize_table(&body.pk_column);

    if body.updates.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No updates provided".into()));
    }

    // Build SET clause with positional params
    let set_parts: Vec<String> = body
        .updates
        .keys()
        .map(|k| format!("\"{}\" = ?", sanitize_table(k)))
        .collect();

    let sql = format!(
        "UPDATE \"{safe_table}\" SET {} WHERE \"{safe_pk_col}\" = ?",
        set_parts.join(", ")
    );

    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for val in body.updates.values() {
        params.push(json_to_sql_param(val));
    }
    params.push(Box::new(pk_value));

    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let affected = conn
        .execute(&sql, params_refs.as_slice())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "affected": affected })))
}

fn json_to_sql_param(val: &serde_json::Value) -> Box<dyn rusqlite::types::ToSql> {
    match val {
        serde_json::Value::Null => Box::new(rusqlite::types::Null),
        serde_json::Value::Bool(b) => Box::new(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        serde_json::Value::String(s) => Box::new(s.clone()),
        other => Box::new(other.to_string()),
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/databases", get(list_databases))
        .route("/api/tables", get(list_tables))
        .route("/api/schema/{table}", get(get_schema))
        .route("/api/rows/{table}", get(get_rows))
        .route("/api/rows/{table}/{pk_value}", put(update_row))
        .layer(cors);

    let addr = "0.0.0.0:3741";
    println!("grove-db-lookup API running at http://localhost:3741");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
