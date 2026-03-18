#![allow(dead_code)] // Public API used by CLI commands (Tasks 6+)

pub fn emit_json(val: &serde_json::Value) -> String {
    serde_json::to_string(val).unwrap_or_else(|_| "{}".to_string())
}

pub fn emit_json_pretty(val: &serde_json::Value) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|_| "{}".to_string())
}

/// Print a JSON error to stdout (used in --json mode).
pub fn emit_error_json(msg: &str, code: i32) -> String {
    emit_json(&serde_json::json!({ "error": msg, "code": code }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_json_produces_compact_json() {
        let val = serde_json::json!({"key": "value"});
        let out = emit_json(&val);
        assert_eq!(out, r#"{"key":"value"}"#);
    }

    #[test]
    fn emit_error_includes_code() {
        let out = emit_error_json("not found", 3);
        assert!(out.contains("\"code\":3"));
    }
}
