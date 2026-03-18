use std::sync::LazyLock;

use regex::Regex;

/// Redact known secret patterns from a string before it is stored in the
/// audit log or emitted as structured output.
///
/// Patterns covered:
/// - Anthropic API keys: `sk-ant-api03-...`
/// - Generic bearer tokens: `Bearer <token>`
/// - AWS access key IDs: `AKIA[A-Z0-9]{16}`
/// - `password=<value>` key-value pairs (case-insensitive)
/// - `api_key=<value>` / `apikey=<value>` key-value pairs (case-insensitive)
///
/// Return `true` if `input` contains any pattern that would be redacted by [`redact`].
///
/// Used by `grove doctor` to retroactively scan stored event payloads for
/// secrets that may have slipped through redaction at write time.
pub fn contains_secret(input: &str) -> bool {
    SK_ANT.is_match(input)
        || BEARER.is_match(input)
        || AWS_AKIA.is_match(input)
        || PASSWORD.is_match(input)
        || API_KEY.is_match(input)
}

pub fn redact(input: &str) -> String {
    let s = SK_ANT.replace_all(input, "sk-ant-***REDACTED***");
    let s = BEARER.replace_all(&s, "Bearer ***REDACTED***");
    let s = AWS_AKIA.replace_all(&s, "***REDACTED***");
    let s = PASSWORD.replace_all(&s, "${pre}***REDACTED***");
    let s = API_KEY.replace_all(&s, "${pre}***REDACTED***");
    s.into_owned()
}

static SK_ANT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"sk-ant-[A-Za-z0-9_\-]{10,100}").expect("redaction regex: SK_ANT")
});

static BEARER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Bearer [A-Za-z0-9._\-]{8,200}").expect("redaction regex: BEARER")
});

static AWS_AKIA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"AKIA[A-Z0-9]{16}").expect("redaction regex: AWS_AKIA"));

static PASSWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?P<pre>password\s*[=:]\s*)[^\s&"']{4,}"#).expect("redaction regex: PASSWORD")
});

static API_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?P<pre>api_?key\s*[=:]\s*)[^\s&"']{4,}"#).expect("redaction regex: API_KEY")
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_key_is_redacted() {
        let s = "using key sk-ant-api03-ABCDEFGHIJ1234567890abcdefghij for auth";
        let out = redact(s);
        assert!(
            !out.contains("ABCDEFGHIJ1234567890"),
            "key must be redacted"
        );
        assert!(out.contains("***REDACTED***"));
    }

    #[test]
    fn bearer_token_is_redacted() {
        let s = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload";
        let out = redact(s);
        assert!(
            !out.contains("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"),
            "bearer must be redacted"
        );
        assert!(out.contains("***REDACTED***"));
    }

    #[test]
    fn aws_akia_key_is_redacted() {
        let s = "AWS key: AKIAIOSFODNN7EXAMPLE in config";
        let out = redact(s);
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(out.contains("***REDACTED***"));
    }

    #[test]
    fn password_value_is_redacted() {
        let s = "connection: host=localhost password=supersecret123 port=5432";
        let out = redact(s);
        assert!(!out.contains("supersecret123"));
        assert!(out.contains("***REDACTED***"));
    }

    #[test]
    fn api_key_value_is_redacted() {
        let s = "api_key=abc123xyz789 in payload";
        let out = redact(s);
        assert!(!out.contains("abc123xyz789"));
        assert!(out.contains("***REDACTED***"));
    }

    #[test]
    fn clean_string_passes_through_unchanged() {
        let s = "objective: refactor the authentication module";
        assert_eq!(redact(s), s);
    }
}
