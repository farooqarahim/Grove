//! stream-json framing for `claude -p --input-format stream-json`.
//!
//! The CLI reads one JSON object per line on stdin and emits one per line
//! on stdout. We encode user turns as `{"type":"user","message":{...}}`
//! and decode assistant/system/result events from stdout.

use crate::errors::{GroveError, GroveResult};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
struct UserTurnEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    message: UserMessage<'a>,
}

#[derive(Debug, Serialize)]
struct UserMessage<'a> {
    role: &'static str,
    content: &'a str,
}

/// Encode a single user turn as one line of stream-json, without the
/// trailing newline — callers append `\n` when writing to stdin.
pub fn encode_user_turn(prompt: &str) -> GroveResult<String> {
    let env = UserTurnEnvelope {
        kind: "user",
        message: UserMessage {
            role: "user",
            content: prompt,
        },
    };
    serde_json::to_string(&env).map_err(|e| GroveError::Runtime(format!("encode_user_turn: {e}")))
}

/// Normalized event extracted from one line of Claude's stdout.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    System {
        session_id: Option<String>,
        raw: Value,
    },
    AssistantText(String),
    Result {
        session_id: Option<String>,
        cost_usd: f64,
        is_error: bool,
    },
    /// Line that parsed as JSON but didn't match any known shape — kept for
    /// forward compatibility; caller logs and moves on.
    Unknown(Value),
}

/// Decode one line of Claude's stdout into a [`StreamEvent`].
///
/// Returns `Ok(None)` for blank lines (harmless — the CLI occasionally
/// emits them). Returns `Err` only when the line is not valid JSON at all;
/// JSON with an unrecognized `type` field yields `Unknown(...)`.
pub fn decode_stream_event(line: &str) -> GroveResult<Option<StreamEvent>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(trimmed)
        .map_err(|e| GroveError::Runtime(format!("decode_stream_event: {e}: line={trimmed}")))?;
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    Ok(Some(match kind {
        "system" => StreamEvent::System {
            session_id: v
                .get("session_id")
                .and_then(Value::as_str)
                .map(String::from),
            raw: v,
        },
        "assistant" => {
            let text = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter_map(|b| b.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            StreamEvent::AssistantText(text)
        }
        "result" => StreamEvent::Result {
            session_id: v
                .get("session_id")
                .and_then(Value::as_str)
                .map(String::from),
            cost_usd: v.get("cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
            is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        },
        _ => StreamEvent::Unknown(v),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_user_turn_produces_well_formed_json() {
        let line = encode_user_turn("hello").unwrap();
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["type"], "user");
        assert_eq!(parsed["message"]["role"], "user");
        assert_eq!(parsed["message"]["content"], "hello");
    }

    #[test]
    fn encode_user_turn_escapes_newlines() {
        let line = encode_user_turn("line1\nline2").unwrap();
        assert!(
            !line.contains('\n'),
            "newline must be JSON-escaped, not literal"
        );
        let parsed: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(parsed["message"]["content"], "line1\nline2");
    }

    #[test]
    fn decode_system_event() {
        let ev = decode_stream_event(r#"{"type":"system","session_id":"S-1","model":"sonnet"}"#)
            .unwrap()
            .unwrap();
        match ev {
            StreamEvent::System { session_id, .. } => {
                assert_eq!(session_id.as_deref(), Some("S-1"))
            }
            other => panic!("expected System, got {other:?}"),
        }
    }

    #[test]
    fn decode_assistant_event() {
        let ev = decode_stream_event(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hi"}]}}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(ev, StreamEvent::AssistantText("hi".into()));
    }

    #[test]
    fn decode_result_event() {
        let ev = decode_stream_event(
            r#"{"type":"result","subtype":"success","session_id":"S-1","cost_usd":0.005,"is_error":false}"#,
        ).unwrap().unwrap();
        assert_eq!(
            ev,
            StreamEvent::Result {
                session_id: Some("S-1".into()),
                cost_usd: 0.005,
                is_error: false,
            }
        );
    }

    #[test]
    fn decode_blank_line_is_none() {
        assert!(decode_stream_event("").unwrap().is_none());
        assert!(decode_stream_event("   \t  ").unwrap().is_none());
    }

    #[test]
    fn decode_unknown_type_is_unknown_not_err() {
        let ev = decode_stream_event(r#"{"type":"progress","pct":42}"#)
            .unwrap()
            .unwrap();
        assert!(matches!(ev, StreamEvent::Unknown(_)));
    }

    #[test]
    fn decode_invalid_json_is_err() {
        assert!(decode_stream_event("not json").is_err());
    }
}
