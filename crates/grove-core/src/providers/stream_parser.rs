use serde::Deserialize;

/// Events emitted by `claude --output-format stream-json` (NDJSON, one per line).
///
/// Covers both the original Claude Code format (`system`, `assistant`,
/// `tool_use`, `tool_result`, `result`, `question`) and the newer
/// Codex-style format (`thread.started`, `turn.started`, `item.completed`,
/// `turn.completed`, `turn.failed`).
///
/// Unknown `type` values are silently ignored via `parse_event` returning
/// `None`.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    // ── Original Claude Code format ─────────────────────────────────────
    System(SystemEvent),
    #[serde(rename = "assistant")]
    Assistant(AssistantEvent),
    #[serde(rename = "tool_use")]
    ToolUse(ToolUseEvent),
    #[serde(rename = "tool_result")]
    ToolResult(ToolResultEvent),
    Result(ResultEvent),
    /// A question emitted by the agent that requires user input.
    #[serde(rename = "question")]
    Question(QuestionEvent),

    // ── Codex / new-format events ───────────────────────────────────────
    /// Emitted at the start of a conversation thread (carries session ID).
    #[serde(rename = "thread.started")]
    ThreadStarted(ThreadStartedEvent),
    /// Emitted when a new turn begins. No payload needed.
    #[serde(rename = "turn.started")]
    TurnStarted {},
    /// Emitted when an item (message, tool call, file change) completes.
    #[serde(rename = "item.completed")]
    ItemCompleted(ItemCompletedEvent),
    /// Emitted when a turn finishes successfully. **Terminal event.**
    #[serde(rename = "turn.completed")]
    TurnCompleted(TurnCompletedEvent),
    /// Emitted when a turn fails (rate limit, context overflow, etc.). **Terminal event.**
    #[serde(rename = "turn.failed")]
    TurnFailed(TurnFailedEvent),
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemEvent {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssistantEvent {
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolUseEvent {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolResultEvent {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestionEvent {
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
    /// Whether the agent is blocked waiting for an answer.
    #[serde(default)]
    pub blocking: bool,
}

// ── Codex / new-format structs ──────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ThreadStartedEvent {
    #[serde(default)]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemCompletedEvent {
    #[serde(default)]
    pub item: Option<ItemPayload>,
}

/// Payload inside an `item.completed` event.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemPayload {
    /// Discriminator: `"agent_message"`, `"command_execution"`, `"file_change"`,
    /// `"todo_list"`, etc.
    #[serde(default, rename = "type")]
    pub item_type: Option<String>,
    /// Present when `item_type == "agent_message"`.
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TurnCompletedEvent {
    #[serde(default)]
    pub usage: Option<UsageInfo>,
}

/// Token usage reported by the agent runtime.
#[derive(Debug, Clone, Deserialize)]
pub struct UsageInfo {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
    #[serde(default)]
    pub cached_input_tokens: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TurnFailedEvent {
    #[serde(default)]
    pub error: Option<TurnError>,
}

/// Error detail inside a `turn.failed` event.
#[derive(Debug, Clone, Deserialize)]
pub struct TurnError {
    #[serde(default)]
    pub message: Option<String>,
}

// ── Original Claude Code structs ────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ResultEvent {
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub duration_api_ms: Option<u64>,
    #[serde(default)]
    pub num_turns: Option<u32>,
}

/// Aggregated output from a streamed invocation.
#[derive(Debug, Clone)]
pub struct StreamResult {
    /// The final result text from the `result` event.
    pub result_text: String,
    /// Whether the result was flagged as an error.
    pub is_error: bool,
    /// Cost in USD reported by the provider.
    pub cost_usd: Option<f64>,
    /// Claude Code session ID (for conversation resumption).
    pub session_id: Option<String>,
}

/// Parse a single NDJSON line into a `StreamEvent`.
///
/// Returns `None` for empty lines, unknown event types, or malformed JSON.
/// This is intentionally lenient — new event types from future Claude CLI
/// versions will be silently ignored.
pub fn parse_event(line: &str) -> Option<StreamEvent> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_system_event() {
        let line =
            r#"{"type":"system","session_id":"abc-123","message":"Claude Code session started"}"#;
        let event = parse_event(line).expect("should parse system event");
        match event {
            StreamEvent::System(sys) => {
                assert_eq!(sys.session_id.as_deref(), Some("abc-123"));
                assert_eq!(sys.message.as_deref(), Some("Claude Code session started"));
            }
            other => panic!("expected System, got {other:?}"),
        }
    }

    #[test]
    fn parse_result_event() {
        let line = r#"{"type":"result","result":"All done","cost_usd":0.05,"is_error":false,"session_id":"sess-456"}"#;
        let event = parse_event(line).expect("should parse result event");
        match event {
            StreamEvent::Result(res) => {
                assert_eq!(res.result, "All done");
                assert_eq!(res.cost_usd, Some(0.05));
                assert!(!res.is_error);
                assert_eq!(res.session_id.as_deref(), Some("sess-456"));
            }
            other => panic!("expected Result, got {other:?}"),
        }
    }

    #[test]
    fn parse_assistant_event() {
        let line = r#"{"type":"assistant","message":"I'll help you with that."}"#;
        let event = parse_event(line).expect("should parse assistant event");
        match event {
            StreamEvent::Assistant(a) => {
                assert_eq!(a.message.as_deref(), Some("I'll help you with that."));
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
    }

    #[test]
    fn parse_tool_use_event() {
        let line = r#"{"type":"tool_use","name":"Read"}"#;
        let event = parse_event(line).expect("should parse tool_use event");
        match event {
            StreamEvent::ToolUse(tu) => {
                assert_eq!(tu.name.as_deref(), Some("Read"));
            }
            other => panic!("expected ToolUse, got {other:?}"),
        }
    }

    #[test]
    fn parse_empty_line_returns_none() {
        assert!(parse_event("").is_none());
        assert!(parse_event("   ").is_none());
    }

    #[test]
    fn parse_unknown_type_returns_none() {
        let line = r#"{"type":"content_block_delta","delta":{"text":"hello"}}"#;
        assert!(parse_event(line).is_none());
    }

    #[test]
    fn parse_malformed_json_returns_none() {
        assert!(parse_event("{not valid json").is_none());
        assert!(parse_event("just a string").is_none());
    }

    // ── Codex / new-format event tests ──────────────────────────────────

    #[test]
    fn parse_thread_started() {
        let line =
            r#"{"type":"thread.started","thread_id":"019cd8c9-96cd-7fa3-8686-3542158093f8"}"#;
        let event = parse_event(line).expect("should parse thread.started");
        match event {
            StreamEvent::ThreadStarted(ts) => {
                assert_eq!(
                    ts.thread_id.as_deref(),
                    Some("019cd8c9-96cd-7fa3-8686-3542158093f8")
                );
            }
            other => panic!("expected ThreadStarted, got {other:?}"),
        }
    }

    #[test]
    fn parse_turn_started() {
        let line = r#"{"type":"turn.started"}"#;
        let event = parse_event(line).expect("should parse turn.started");
        assert!(matches!(event, StreamEvent::TurnStarted {}));
    }

    #[test]
    fn parse_item_completed_agent_message() {
        let line = r#"{"type":"item.completed","item":{"type":"agent_message","text":"Done!"}}"#;
        let event = parse_event(line).expect("should parse item.completed");
        match event {
            StreamEvent::ItemCompleted(ic) => {
                let item = ic.item.expect("item should be present");
                assert_eq!(item.item_type.as_deref(), Some("agent_message"));
                assert_eq!(item.text.as_deref(), Some("Done!"));
            }
            other => panic!("expected ItemCompleted, got {other:?}"),
        }
    }

    #[test]
    fn parse_item_completed_command_execution() {
        let line = r#"{"type":"item.completed","item":{"type":"command_execution","command":"ls","exit_code":0,"status":"completed"}}"#;
        let event = parse_event(line).expect("should parse item.completed");
        match event {
            StreamEvent::ItemCompleted(ic) => {
                let item = ic.item.expect("item should be present");
                assert_eq!(item.item_type.as_deref(), Some("command_execution"));
                assert!(item.text.is_none());
            }
            other => panic!("expected ItemCompleted, got {other:?}"),
        }
    }

    #[test]
    fn parse_turn_completed() {
        let line = r#"{"type":"turn.completed","usage":{"input_tokens":164973,"cached_input_tokens":152960,"output_tokens":5233}}"#;
        let event = parse_event(line).expect("should parse turn.completed");
        match event {
            StreamEvent::TurnCompleted(tc) => {
                let usage = tc.usage.expect("usage should be present");
                assert_eq!(usage.input_tokens, Some(164973));
                assert_eq!(usage.output_tokens, Some(5233));
                assert_eq!(usage.cached_input_tokens, Some(152960));
            }
            other => panic!("expected TurnCompleted, got {other:?}"),
        }
    }

    #[test]
    fn parse_turn_failed() {
        let line = r#"{"type":"turn.failed","error":{"message":"rate limit exceeded"}}"#;
        let event = parse_event(line).expect("should parse turn.failed");
        match event {
            StreamEvent::TurnFailed(tf) => {
                let err = tf.error.expect("error should be present");
                assert_eq!(err.message.as_deref(), Some("rate limit exceeded"));
            }
            other => panic!("expected TurnFailed, got {other:?}"),
        }
    }

    #[test]
    fn parse_turn_completed_minimal() {
        // turn.completed with no usage field
        let line = r#"{"type":"turn.completed"}"#;
        let event = parse_event(line).expect("should parse minimal turn.completed");
        match event {
            StreamEvent::TurnCompleted(tc) => {
                assert!(tc.usage.is_none());
            }
            other => panic!("expected TurnCompleted, got {other:?}"),
        }
    }
}
