//! Adapter for **Codex** (OpenAI CLI — `codex exec` headless mode).
//!
//! ## Execution model
//!
//! Codex has two modes:
//! - Interactive TUI (default `codex` command) — checks `isatty`, requires a PTY.
//! - Headless exec (`codex exec PROMPT`) — designed for pipe/script use; no PTY needed.
//!
//! Grove always uses the `exec` subcommand so codex runs cleanly in a pipe.
//!
//! ## Structured output
//!
//! `--json` makes codex emit one NDJSON event per line to stdout.  Grove parses
//! this stream to extract the agent's final message and the `thread_id` (session
//! ID for `codex exec resume`).
//!
//! ## Session continuity
//!
//! When `provider_session_id` is set in a subsequent request the caller should
//! use `codex exec resume <thread_id>` — see `build_resume_args`.
//!
//! ## CLI reference
//!
//! ```text
//! codex exec [--full-auto] [--json] [--color=never] [--model <id>] <prompt>
//! codex exec resume <thread_id>
//! ```

use super::adapter::{CodingAgentAdapter, ExecutionMode};
use crate::errors::{GroveError, GroveResult};

pub struct CodexAdapter;

impl CodingAgentAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn default_command(&self) -> &str {
        "codex"
    }

    fn execution_mode(&self) -> ExecutionMode {
        // `codex exec` runs cleanly in a pipe — no PTY required.
        ExecutionMode::Pipe
    }

    fn build_args(&self, model: Option<&str>, prompt: &str) -> Vec<String> {
        // codex exec  → headless subcommand (no TTY check)
        // --full-auto → auto-approve all file/shell operations within workspace
        // --json      → emit NDJSON event stream so we can extract thread_id + result
        // --color=never → suppress ANSI codes (we're in a pipe, not a terminal)
        let mut args = vec![
            "exec".to_string(),
            "--full-auto".to_string(),
            "--json".to_string(),
            "--color=never".to_string(),
        ];
        if let Some(m) = model {
            if !m.is_empty() {
                args.push("--model".to_string());
                args.push(m.to_string());
            }
        }
        args.push(prompt.to_string());
        args
    }

    fn parse_output(&self, raw: String) -> GroveResult<(String, Option<String>)> {
        parse_codex_jsonl(&raw)
    }

    fn build_resume_args(&self, thread_id: &str) -> Option<Vec<String>> {
        // codex exec resume <thread_id>  — re-enters the same conversation thread.
        Some(vec![
            "exec".to_string(),
            "resume".to_string(),
            thread_id.to_string(),
        ])
    }
}

// ── JSONL parser ──────────────────────────────────────────────────────────────

/// Parse the NDJSON stream emitted by `codex exec --json`.
///
/// Returns `Ok((summary, thread_id))` where:
/// - `summary` is the concatenation of all completed `agent_message` items.
/// - `thread_id` is the session ID from the `thread.started` event, usable
///   with `codex exec resume <thread_id>` in a subsequent run.
///
/// Returns `Err` when a `turn.failed` event is found, so Grove marks the
/// session as failed and retry logic fires.  The error message includes the
/// `thread_id` (if known) so the caller can log it.
///
/// If no valid JSON events are found (e.g. an older codex version without
/// `--json`), the raw output is returned as-is and `thread_id` is `None`.
fn parse_codex_jsonl(raw: &str) -> GroveResult<(String, Option<String>)> {
    use serde_json::Value;

    let mut thread_id: Option<String> = None;
    let mut messages: Vec<String> = Vec::new();
    let mut parsed_any = false;
    let mut failed_msg: Option<String> = None;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        parsed_any = true;

        match val.get("type").and_then(|t| t.as_str()) {
            Some("thread.started") => {
                if let Some(tid) = val.get("thread_id").and_then(|t| t.as_str()) {
                    thread_id = Some(tid.to_string());
                }
            }
            Some("item.completed") => {
                if let Some(item) = val.get("item") {
                    let item_type = item.get("type").and_then(|t| t.as_str());
                    if item_type == Some("agent_message") {
                        if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                            if !text.trim().is_empty() {
                                messages.push(text.to_string());
                            }
                        }
                    }
                }
            }
            Some("turn.failed") => {
                // Codex reported an explicit failure (rate limit, context overflow, etc.).
                // Capture the message; we'll return Err after finishing the stream so
                // that thread_id (if seen) is also available for logging.
                let msg = val
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error");
                failed_msg = Some(msg.to_string());
            }
            _ => {}
        }
    }

    if !parsed_any {
        // Older codex or `--json` not supported — return raw output unchanged.
        return Ok((raw.to_string(), None));
    }

    if let Some(msg) = failed_msg {
        let detail = match &thread_id {
            Some(tid) => format!("codex turn.failed (thread {tid}): {msg}"),
            None => format!("codex turn.failed: {msg}"),
        };
        return Err(GroveError::Runtime(detail));
    }

    let summary = if messages.is_empty() {
        raw.to_string()
    } else {
        messages.join("\n\n")
    };

    Ok((summary, thread_id))
}

#[cfg(test)]
mod tests {
    use super::parse_codex_jsonl;

    #[test]
    fn extracts_thread_id_and_message() {
        let input = concat!(
            r#"{"type":"thread.started","thread_id":"tid-abc123"}"#,
            "\n",
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Done!"}}"#,
            "\n",
            r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":20}}"#,
            "\n",
        );
        let (summary, tid) = parse_codex_jsonl(input).unwrap();
        assert_eq!(summary, "Done!");
        assert_eq!(tid.as_deref(), Some("tid-abc123"));
    }

    #[test]
    fn multiple_messages_joined() {
        let input = concat!(
            r#"{"type":"thread.started","thread_id":"t1"}"#,
            "\n",
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"First"}}"#,
            "\n",
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"Second"}}"#,
            "\n",
        );
        let (summary, _) = parse_codex_jsonl(input).unwrap();
        assert_eq!(summary, "First\n\nSecond");
    }

    #[test]
    fn non_json_raw_returned_unchanged() {
        let raw = "plain text output\nno json here";
        let (summary, tid) = parse_codex_jsonl(raw).unwrap();
        assert_eq!(summary, raw);
        assert!(tid.is_none());
    }

    #[test]
    fn turn_failed_returns_err() {
        let input = concat!(
            r#"{"type":"thread.started","thread_id":"t2"}"#,
            "\n",
            r#"{"type":"turn.failed","error":{"message":"rate limit exceeded"}}"#,
            "\n",
        );
        let err = parse_codex_jsonl(input).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("rate limit exceeded"),
            "expected error message, got: {msg}"
        );
        assert!(
            msg.contains("t2"),
            "expected thread_id in error, got: {msg}"
        );
    }
}
