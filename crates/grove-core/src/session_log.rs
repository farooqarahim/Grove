//! Parse session log files (JSONL) into structured conversation entries.
//!
//! Log files are stored at `.grove/logs/runs/{run_id}/session-{session_id}.jsonl`.
//! Each line is a raw NDJSON event from the CLI agent (Claude Code stream-json
//! format for Claude Code, raw text lines for other agents).

use std::fs;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::paths;
use crate::errors::{GroveError, GroveResult};
use crate::providers::stream_parser;

/// A single conversation entry parsed from a session log file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// "system", "assistant", "tool_use", "tool_result", or "result"
    pub role: String,
    /// The text content of this entry.
    pub content: String,
    /// Tool name (for tool_use / tool_result entries).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Provider session ID (from system/result events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Cost in USD (from result events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Whether this is an error result.
    #[serde(default)]
    pub is_error: bool,
    /// 1-based line number from the session JSONL file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_no: Option<u32>,
    /// Raw event type, such as "system", "assistant", or "result".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    /// Event subtype when present, such as "init".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
    /// Additional parsed detail for richer log rendering.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Structured metadata captured from the source event as JSON.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    /// Correlates chat questions and answers for inline permission cards.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Structured options offered by the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    /// Whether the question blocks the agent until answered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking: Option<bool>,
    /// Pending or resolved status for interactive chat questions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Final decision for a resolved question.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    /// Optional timeout deadline for pending permission prompts.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_at: Option<String>,
    /// Persisted answer text for resolved questions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

/// Read and parse a session log file into structured conversation entries.
///
/// Returns an empty vec if the file doesn't exist (old runs without logs).
pub fn read_session_log(
    project_root: &Path,
    run_id: &str,
    session_id: &str,
) -> GroveResult<Vec<LogEntry>> {
    let log_path = paths::logs_dir(project_root)
        .join("runs")
        .join(run_id)
        .join(format!("session-{session_id}.jsonl"));

    if !log_path.exists() {
        return Ok(vec![]);
    }

    let content = fs::read_to_string(&log_path).map_err(|e| {
        GroveError::Runtime(format!(
            "failed to read session log {}: {e}",
            log_path.display()
        ))
    })?;

    Ok(parse_log_lines(&content))
}

/// List all session log files for a given run, returning session IDs.
pub fn list_session_logs(project_root: &Path, run_id: &str) -> GroveResult<Vec<String>> {
    let log_dir = paths::logs_dir(project_root).join("runs").join(run_id);

    if !log_dir.exists() {
        return Ok(vec![]);
    }

    let mut session_ids = Vec::new();
    for entry in fs::read_dir(&log_dir).map_err(|e| {
        GroveError::Runtime(format!("failed to read log dir {}: {e}", log_dir.display()))
    })? {
        let entry = entry.map_err(|e| GroveError::Runtime(format!("readdir error: {e}")))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(sid) = name
            .strip_prefix("session-")
            .and_then(|s| s.strip_suffix(".jsonl"))
        {
            session_ids.push(sid.to_string());
        }
    }

    Ok(session_ids)
}

/// Parse raw NDJSON content into structured entries (public for chatter module).
pub fn parse_log_lines_public(content: &str) -> Vec<LogEntry> {
    parse_log_lines(content)
}

/// Parse a single NDJSON line into structured entries.
pub fn parse_single_line(line: &str) -> Vec<LogEntry> {
    parse_log_lines(line)
}

/// Parse raw NDJSON lines into structured entries.
fn parse_log_lines(content: &str) -> Vec<LogEntry> {
    let mut entries = Vec::new();
    let mut tool_names: HashMap<String, String> = HashMap::new();

    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let line_no = Some((idx + 1) as u32);

        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if parse_chat_custom_event(&value, &mut entries, line_no) {
                continue;
            }
            if parse_modern_claude_event(&value, &mut tool_names, &mut entries, line_no) {
                continue;
            }
        }

        // Try to parse as stream-json event (Claude Code format)
        if let Some(event) = stream_parser::parse_event(trimmed) {
            match event {
                stream_parser::StreamEvent::System(sys) => {
                    entries.push(LogEntry {
                        role: "system".to_string(),
                        content: sys.message.unwrap_or_default(),
                        tool_name: None,
                        session_id: sys.session_id,
                        cost_usd: None,
                        is_error: false,
                        line_no,
                        event_type: Some("system".to_string()),
                        subtype: None,
                        detail: None,
                        metadata_json: None,
                        request_id: None,
                        options: None,
                        blocking: None,
                        status: None,
                        decision: None,
                        timeout_at: None,
                        answer: None,
                    });
                }
                stream_parser::StreamEvent::Assistant(a) => {
                    if let Some(msg) = a.message {
                        if !msg.is_empty() {
                            entries.push(LogEntry {
                                role: "assistant".to_string(),
                                content: msg,
                                tool_name: None,
                                session_id: None,
                                cost_usd: None,
                                is_error: false,
                                line_no,
                                event_type: Some("assistant".to_string()),
                                subtype: None,
                                detail: None,
                                metadata_json: None,
                                request_id: None,
                                options: None,
                                blocking: None,
                                status: None,
                                decision: None,
                                timeout_at: None,
                                answer: None,
                            });
                        }
                    }
                }
                stream_parser::StreamEvent::ToolUse(tu) => {
                    entries.push(LogEntry {
                        role: "tool_use".to_string(),
                        content: String::new(),
                        tool_name: tu.name,
                        session_id: None,
                        cost_usd: None,
                        is_error: false,
                        line_no,
                        event_type: Some("tool_use".to_string()),
                        subtype: None,
                        detail: None,
                        metadata_json: None,
                        request_id: None,
                        options: None,
                        blocking: None,
                        status: None,
                        decision: None,
                        timeout_at: None,
                        answer: None,
                    });
                }
                stream_parser::StreamEvent::ToolResult(tr) => {
                    entries.push(LogEntry {
                        role: "tool_result".to_string(),
                        content: String::new(),
                        tool_name: tr.name,
                        session_id: None,
                        cost_usd: None,
                        is_error: false,
                        line_no,
                        event_type: Some("tool_result".to_string()),
                        subtype: None,
                        detail: None,
                        metadata_json: None,
                        request_id: None,
                        options: None,
                        blocking: None,
                        status: None,
                        decision: None,
                        timeout_at: None,
                        answer: None,
                    });
                }
                stream_parser::StreamEvent::Result(res) => {
                    entries.push(LogEntry {
                        role: "result".to_string(),
                        content: res.result,
                        tool_name: None,
                        session_id: res.session_id,
                        cost_usd: res.cost_usd,
                        is_error: res.is_error,
                        line_no,
                        event_type: Some("result".to_string()),
                        subtype: None,
                        detail: None,
                        metadata_json: None,
                        request_id: None,
                        options: None,
                        blocking: None,
                        status: None,
                        decision: None,
                        timeout_at: None,
                        answer: None,
                    });
                }
                // Question events are handled separately by the streaming infrastructure
                _ => {}
            }
        } else {
            // Non-JSON line (raw output from non-Claude agents)
            entries.push(LogEntry {
                role: "raw".to_string(),
                content: trimmed.to_string(),
                tool_name: None,
                session_id: None,
                cost_usd: None,
                is_error: false,
                line_no,
                event_type: Some("raw".to_string()),
                subtype: None,
                detail: None,
                metadata_json: None,
                request_id: None,
                options: None,
                blocking: None,
                status: None,
                decision: None,
                timeout_at: None,
                answer: None,
            });
        }
    }

    entries
}

fn parse_chat_custom_event(
    value: &Value,
    entries: &mut Vec<LogEntry>,
    line_no: Option<u32>,
) -> bool {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return false;
    };

    match kind {
        "user_input" => {
            let content = value
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            entries.push(LogEntry {
                role: "user_input".to_string(),
                content,
                tool_name: None,
                session_id: None,
                cost_usd: None,
                is_error: false,
                line_no,
                event_type: Some("user_input".to_string()),
                subtype: None,
                detail: None,
                metadata_json: None,
                request_id: None,
                options: None,
                blocking: None,
                status: None,
                decision: None,
                timeout_at: None,
                answer: None,
            });
            true
        }
        "chat_question" => {
            entries.push(LogEntry {
                role: "agent_question".to_string(),
                content: value
                    .get("question")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                tool_name: value
                    .get("tool_name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                session_id: None,
                cost_usd: None,
                is_error: false,
                line_no,
                event_type: Some("chat_question".to_string()),
                subtype: value
                    .get("request_kind")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                detail: value
                    .get("tool_summary")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                metadata_json: value.get("metadata").map(|m| m.to_string()),
                request_id: value
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                options: value.get("options").and_then(Value::as_array).map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect::<Vec<_>>()
                }),
                blocking: value.get("blocking").and_then(Value::as_bool),
                status: value
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                decision: value
                    .get("decision")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                timeout_at: value
                    .get("timeout_at")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                answer: None,
            });
            true
        }
        "chat_answer" => {
            entries.push(LogEntry {
                role: "user_answer".to_string(),
                content: value
                    .get("answer")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                tool_name: None,
                session_id: None,
                cost_usd: None,
                is_error: false,
                line_no,
                event_type: Some("chat_answer".to_string()),
                subtype: None,
                detail: None,
                metadata_json: None,
                request_id: value
                    .get("request_id")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                options: None,
                blocking: None,
                status: value
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                decision: value
                    .get("decision")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                timeout_at: None,
                answer: value
                    .get("answer")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
            true
        }
        "stderr" => {
            entries.push(LogEntry {
                role: "system".to_string(),
                content: value
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                tool_name: None,
                session_id: None,
                cost_usd: None,
                is_error: true,
                line_no,
                event_type: Some("stderr".to_string()),
                subtype: None,
                detail: Some("stderr".to_string()),
                metadata_json: None,
                request_id: None,
                options: None,
                blocking: None,
                status: None,
                decision: None,
                timeout_at: None,
                answer: None,
            });
            true
        }
        _ => false,
    }
}

fn parse_modern_claude_event(
    value: &Value,
    tool_names: &mut HashMap<String, String>,
    entries: &mut Vec<LogEntry>,
    line_no: Option<u32>,
) -> bool {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return false;
    };

    match kind {
        "system" => {
            parse_modern_system_event(value, entries, line_no);
            true
        }
        "assistant" => {
            parse_modern_assistant_event(value, tool_names, entries, line_no);
            true
        }
        "user" => {
            parse_modern_user_event(value, tool_names, entries, line_no);
            true
        }
        "rate_limit_event" => {
            parse_rate_limit_event(value, entries, line_no);
            true
        }
        "result" => {
            parse_modern_result_event(value, entries, line_no);
            true
        }
        _ => false,
    }
}

fn parse_modern_system_event(value: &Value, entries: &mut Vec<LogEntry>, line_no: Option<u32>) {
    let subtype = value
        .get("subtype")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);

    if subtype == "init" {
        let model = value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let cwd = value.get("cwd").and_then(Value::as_str).unwrap_or_default();
        let cwd_path = PathBuf::from(cwd);
        let cwd_label = cwd_path.file_name().and_then(|s| s.to_str()).unwrap_or(cwd);
        let permission = value
            .get("permissionMode")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mut parts = vec![format!("Session initialized ({model})")];
        if !cwd_label.is_empty() {
            parts.push(format!("cwd {cwd_label}"));
        }
        if !permission.is_empty() {
            parts.push(format!("mode {permission}"));
        }
        entries.push(LogEntry {
            role: "system".to_string(),
            content: parts.join(" • "),
            tool_name: None,
            session_id: session_id.clone(),
            cost_usd: None,
            is_error: false,
            line_no,
            event_type: Some("system".to_string()),
            subtype: Some("init".to_string()),
            detail: None,
            metadata_json: Some(
                serde_json::json!({
                    "model": model,
                    "cwd": cwd,
                    "permission_mode": permission,
                    "output_style": value.get("output_style").and_then(Value::as_str),
                    "api_key_source": value.get("apiKeySource").and_then(Value::as_str),
                })
                .to_string(),
            ),
            request_id: None,
            options: None,
            blocking: None,
            status: None,
            decision: None,
            timeout_at: None,
            answer: None,
        });

        let version = value
            .get("claude_code_version")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let tools_count = value
            .get("tools")
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        let agents_count = value
            .get("agents")
            .and_then(Value::as_array)
            .map(|items| items.len())
            .unwrap_or(0);
        if !version.is_empty() || tools_count > 0 || agents_count > 0 {
            let mut details = Vec::new();
            if !version.is_empty() {
                details.push(format!("Claude Code {version}"));
            }
            if tools_count > 0 {
                details.push(format!("{tools_count} tools"));
            }
            if agents_count > 0 {
                details.push(format!("{agents_count} agents"));
            }
            entries.push(LogEntry {
                role: "system".to_string(),
                content: details.join(" • "),
                tool_name: None,
                session_id: session_id.clone(),
                cost_usd: None,
                is_error: false,
                line_no,
                event_type: Some("system".to_string()),
                subtype: Some("init".to_string()),
                detail: Some("Runtime".to_string()),
                metadata_json: Some(
                    serde_json::json!({
                        "claude_code_version": version,
                        "tools_count": tools_count,
                        "agents_count": agents_count,
                    })
                    .to_string(),
                ),
                request_id: None,
                options: None,
                blocking: None,
                status: None,
                decision: None,
                timeout_at: None,
                answer: None,
            });
        }

        let needs_auth = value
            .get("mcp_servers")
            .and_then(Value::as_array)
            .map(|servers| {
                servers
                    .iter()
                    .filter_map(|server| {
                        let status = server.get("status").and_then(Value::as_str)?;
                        if status == "needs-auth" || status == "unauthorized" {
                            server
                                .get("name")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !needs_auth.is_empty() {
            entries.push(LogEntry {
                role: "system".to_string(),
                content: format!(
                    "Integrations need auth • {}",
                    compact_text(&needs_auth.join(", "), 120)
                ),
                tool_name: None,
                session_id,
                cost_usd: None,
                is_error: false,
                line_no,
                event_type: Some("system".to_string()),
                subtype: Some("init".to_string()),
                detail: Some("Integrations".to_string()),
                metadata_json: Some(
                    serde_json::json!({
                        "needs_auth": needs_auth,
                    })
                    .to_string(),
                ),
                request_id: None,
                options: None,
                blocking: None,
                status: None,
                decision: None,
                timeout_at: None,
                answer: None,
            });
        }
    }
}

fn parse_modern_assistant_event(
    value: &Value,
    tool_names: &mut HashMap<String, String>,
    entries: &mut Vec<LogEntry>,
    line_no: Option<u32>,
) {
    let session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let content_items = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array);

    let Some(items) = content_items else {
        return;
    };

    for item in items {
        let Some(item_type) = item.get("type").and_then(Value::as_str) else {
            continue;
        };
        match item_type {
            "text" => {
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim();
                if !text.is_empty() {
                    entries.push(LogEntry {
                        role: "assistant".to_string(),
                        content: text.to_string(),
                        tool_name: None,
                        session_id: session_id.clone(),
                        cost_usd: None,
                        is_error: false,
                        line_no,
                        event_type: Some("assistant".to_string()),
                        subtype: Some("text".to_string()),
                        detail: value
                            .get("message")
                            .and_then(|m| m.get("id"))
                            .and_then(Value::as_str)
                            .map(|id| format!("message {id}")),
                        metadata_json: None,
                        request_id: None,
                        options: None,
                        blocking: None,
                        status: None,
                        decision: None,
                        timeout_at: None,
                        answer: None,
                    });
                }
            }
            "tool_use" => {
                let tool_name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string();
                if let Some(tool_id) = item.get("id").and_then(Value::as_str) {
                    tool_names.insert(tool_id.to_string(), tool_name.clone());
                }
                let input_summary = item
                    .get("input")
                    .map(summarize_tool_input)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_default();
                entries.push(LogEntry {
                    role: "tool_use".to_string(),
                    content: input_summary,
                    tool_name: Some(tool_name),
                    session_id: session_id.clone(),
                    cost_usd: None,
                    is_error: false,
                    line_no,
                    event_type: Some("assistant".to_string()),
                    subtype: Some("tool_use".to_string()),
                    detail: item
                        .get("id")
                        .and_then(Value::as_str)
                        .map(|id| format!("tool_use {id}")),
                    metadata_json: item
                        .get("input")
                        .map(build_tool_input_metadata)
                        .map(|m| m.to_string()),
                    request_id: None,
                    options: None,
                    blocking: None,
                    status: None,
                    decision: None,
                    timeout_at: None,
                    answer: None,
                });
            }
            _ => {}
        }
    }
}

fn parse_modern_user_event(
    value: &Value,
    tool_names: &mut HashMap<String, String>,
    entries: &mut Vec<LogEntry>,
    line_no: Option<u32>,
) {
    let session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let content_items = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array);

    let Some(items) = content_items else {
        return;
    };

    for item in items {
        let Some(item_type) = item.get("type").and_then(Value::as_str) else {
            continue;
        };
        if item_type != "tool_result" {
            continue;
        }
        let tool_name = item
            .get("tool_use_id")
            .and_then(Value::as_str)
            .and_then(|id| tool_names.get(id).cloned());
        let summary = summarize_tool_result(value, item, tool_name.as_deref());
        entries.push(LogEntry {
            role: "tool_result".to_string(),
            content: summary,
            tool_name,
            session_id: session_id.clone(),
            cost_usd: None,
            is_error: false,
            line_no,
            event_type: Some("user".to_string()),
            subtype: Some("tool_result".to_string()),
            detail: item
                .get("tool_use_id")
                .and_then(Value::as_str)
                .map(|id| format!("for {id}")),
            metadata_json: value
                .get("tool_use_result")
                .map(build_tool_result_metadata)
                .map(|m| m.to_string()),
            request_id: None,
            options: None,
            blocking: None,
            status: None,
            decision: None,
            timeout_at: None,
            answer: None,
        });
    }
}

fn parse_modern_result_event(value: &Value, entries: &mut Vec<LogEntry>, line_no: Option<u32>) {
    let session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let result = value
        .get("result")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    let stop_reason = value
        .get("stop_reason")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let duration_ms = value.get("duration_ms").and_then(Value::as_u64);
    entries.push(LogEntry {
        role: "result".to_string(),
        content: result.to_string(),
        tool_name: None,
        session_id,
        cost_usd: value.get("total_cost_usd").and_then(Value::as_f64),
        is_error: value
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        line_no,
        event_type: Some("result".to_string()),
        subtype: value
            .get("subtype")
            .and_then(Value::as_str)
            .map(str::to_string),
        detail: Some(
            [
                (!stop_reason.is_empty()).then(|| format!("stop {stop_reason}")),
                duration_ms.map(|d| format!("{d} ms")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" • "),
        )
        .filter(|s| !s.is_empty()),
        metadata_json: value.get("usage").map(|usage| usage.to_string()),
        request_id: None,
        options: None,
        blocking: None,
        status: None,
        decision: None,
        timeout_at: None,
        answer: None,
    });
}

fn parse_rate_limit_event(value: &Value, entries: &mut Vec<LogEntry>, line_no: Option<u32>) {
    let Some(info) = value.get("rate_limit_info") else {
        return;
    };
    let status = info
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if status == "allowed" {
        return;
    }
    let session_id = value
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let rate_type = info
        .get("rateLimitType")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    entries.push(LogEntry {
        role: "system".to_string(),
        content: format!("Rate limit event • {rate_type} • {status}"),
        tool_name: None,
        session_id,
        cost_usd: None,
        is_error: true,
        line_no,
        event_type: Some("rate_limit_event".to_string()),
        subtype: None,
        detail: None,
        metadata_json: Some(info.to_string()),
        request_id: None,
        options: None,
        blocking: None,
        status: None,
        decision: None,
        timeout_at: None,
        answer: None,
    });
}

fn build_tool_input_metadata(input: &Value) -> Value {
    let mut meta = serde_json::Map::new();
    if let Some(obj) = input.as_object() {
        for key in [
            "file_path",
            "path",
            "command",
            "pattern",
            "prompt",
            "url",
            "mode",
            "description",
        ] {
            if let Some(value) = obj.get(key) {
                let summarized = if let Some(text) = value.as_str() {
                    Value::String(compact_text(text, 240))
                } else {
                    value.clone()
                };
                meta.insert(key.to_string(), summarized);
            }
        }
        if let Some(content) = obj.get("content").and_then(Value::as_str) {
            meta.insert(
                "content_preview".to_string(),
                Value::String(compact_text(content, 240)),
            );
            meta.insert(
                "content_length".to_string(),
                Value::from(content.len() as u64),
            );
        }
    }
    Value::Object(meta)
}

fn build_tool_result_metadata(result: &Value) -> Value {
    let mut meta = serde_json::Map::new();
    if let Some(obj) = result.as_object() {
        for key in ["type", "filePath", "numLines"] {
            if let Some(value) = obj.get(key) {
                meta.insert(key.to_string(), value.clone());
            }
        }
        if let Some(content) = obj.get("content").and_then(Value::as_str) {
            meta.insert(
                "content_preview".to_string(),
                Value::String(compact_text(content, 240)),
            );
            meta.insert(
                "content_length".to_string(),
                Value::from(content.len() as u64),
            );
        }
        if let Some(file) = obj.get("file") {
            meta.insert("file".to_string(), file.clone());
        }
    }
    Value::Object(meta)
}

fn summarize_tool_input(input: &Value) -> String {
    let Some(obj) = input.as_object() else {
        return String::new();
    };
    if let Some(path) = obj.get("file_path").and_then(Value::as_str) {
        return file_label(path);
    }
    if let Some(path) = obj.get("path").and_then(Value::as_str) {
        return file_label(path);
    }
    if let Some(cmd) = obj.get("command").and_then(Value::as_str) {
        return compact_text(cmd, 120);
    }
    if let Some(prompt) = obj.get("prompt").and_then(Value::as_str) {
        return compact_text(prompt, 120);
    }
    if let Some(pattern) = obj.get("pattern").and_then(Value::as_str) {
        return compact_text(pattern, 80);
    }
    String::new()
}

fn summarize_tool_result(event: &Value, item: &Value, tool_name: Option<&str>) -> String {
    if let Some(result) = event.get("tool_use_result") {
        if let Some(summary) = summarize_structured_tool_result(result, tool_name) {
            return summary;
        }
    }

    if let Some(content) = item.get("content").and_then(Value::as_str) {
        return compact_text(content, 160);
    }

    String::new()
}

fn summarize_structured_tool_result(result: &Value, tool_name: Option<&str>) -> Option<String> {
    let result_type = result
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match result_type {
        "text" => {
            if let Some(file) = result.get("file") {
                let path = file
                    .get("filePath")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let num_lines = file.get("numLines").and_then(Value::as_u64);
                let mut label = match tool_name {
                    Some("Read") => format!("Read {}", file_label(path)),
                    _ => file_label(path),
                };
                if let Some(lines) = num_lines {
                    label.push_str(&format!(" ({lines} lines)"));
                }
                return Some(label);
            }
            result
                .get("content")
                .and_then(Value::as_str)
                .map(|s| compact_text(s, 160))
        }
        "create" => result
            .get("filePath")
            .and_then(Value::as_str)
            .map(|path| format!("Created {}", file_label(path))),
        "update" => result
            .get("filePath")
            .and_then(Value::as_str)
            .map(|path| format!("Updated {}", file_label(path))),
        "delete" => result
            .get("filePath")
            .and_then(Value::as_str)
            .map(|path| format!("Deleted {}", file_label(path))),
        _ => None,
    }
}

fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string()
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let char_count = compact.chars().count();
    if char_count <= max_chars {
        compact
    } else {
        let truncated: String = compact.chars().take(max_chars).collect();
        format!("{}…", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_single_line;

    #[test]
    fn parses_chat_question_event() {
        let line = r#"{"type":"chat_question","request_id":"req-1","request_kind":"permission","question":"Allow Write?","options":["Allow","Deny"],"blocking":true,"tool_name":"Write","tool_summary":"foo.txt","metadata":{"file_path":"foo.txt"},"status":"pending","timeout_at":"2026-03-13T12:00:00Z"}"#;
        let entries = parse_single_line(line);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.role, "agent_question");
        assert_eq!(entry.request_id.as_deref(), Some("req-1"));
        assert_eq!(entry.tool_name.as_deref(), Some("Write"));
        assert_eq!(entry.detail.as_deref(), Some("foo.txt"));
        assert_eq!(entry.status.as_deref(), Some("pending"));
        assert_eq!(entry.options.as_ref().map(|v| v.len()), Some(2));
    }

    #[test]
    fn parses_chat_answer_event() {
        let line = r#"{"type":"chat_answer","request_id":"req-1","answer":"Allow","decision":"allow","status":"resolved"}"#;
        let entries = parse_single_line(line);
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.role, "user_answer");
        assert_eq!(entry.request_id.as_deref(), Some("req-1"));
        assert_eq!(entry.answer.as_deref(), Some("Allow"));
        assert_eq!(entry.decision.as_deref(), Some("allow"));
        assert_eq!(entry.status.as_deref(), Some("resolved"));
    }

    #[test]
    fn parses_user_input_event() {
        let line = r#"{"type":"user_input","content":"Create Foo.md"}"#;
        let entries = parse_single_line(line);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].role, "user_input");
        assert_eq!(entries[0].content, "Create Foo.md");
    }
}
