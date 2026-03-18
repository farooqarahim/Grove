//! Universal baseline filter applied to all unrecognised commands.
//!
//! 1. Strip ANSI escape codes
//! 2. Remove consecutive duplicate lines
//! 3. Check session dedup cache — return cache-hit message if seen before
//! 4. Truncate at configurable byte ceiling
//! 5. Append `[grove: +N lines omitted]` hint if truncated

use std::sync::OnceLock;

use sha2::{Digest, Sha256};

use super::super::session::{FilterState, SeenEntry};

/// Default maximum bytes per command output (~8K tokens × 4 bytes/token).
const DEFAULT_MAX_BYTES: usize = 32_000;

static ANSI_RE: OnceLock<regex::Regex> = OnceLock::new();

fn ansi_regex() -> &'static regex::Regex {
    ANSI_RE.get_or_init(|| {
        regex::Regex::new(
            r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b\].*?\x1b\\|\x1b[()][A-B0-2]|\x1b",
        )
        .expect("ANSI regex is valid")
    })
}

/// Strip ANSI/VT100 escape sequences from text.
pub fn strip_ansi(text: &str) -> String {
    ansi_regex().replace_all(text, "").into_owned()
}

/// Remove consecutive duplicate lines.
pub fn dedup_consecutive_lines(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev: Option<&str> = None;
    let mut dup_count: usize = 0;

    for line in text.lines() {
        if prev == Some(line) {
            dup_count += 1;
            continue;
        }
        if dup_count > 0 {
            result.push_str(&format!(
                "[grove: previous line repeated {} times]\n",
                dup_count
            ));
            dup_count = 0;
        }
        result.push_str(line);
        result.push('\n');
        prev = Some(line);
    }
    if dup_count > 0 {
        result.push_str(&format!(
            "[grove: previous line repeated {} times]\n",
            dup_count
        ));
    }

    result
}

/// Compute SHA-256 hex digest of the given text.
fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Apply the universal baseline filter.
pub fn filter(output: &str, level: u8, state: &mut FilterState) -> String {
    // Stage 1: strip ANSI codes
    let cleaned = strip_ansi(output);

    // Stage 2: dedup consecutive lines
    let deduped = dedup_consecutive_lines(&cleaned);

    // Stage 3: check session content cache
    let hash = content_hash(&deduped);
    if let Some(seen) = state.seen_hashes.get(&hash) {
        return format!(
            "[grove: identical output as {} (invocation #{}) — skipped]\n",
            seen.command, seen.invocation_index
        );
    }
    // Record this output in the dedup cache
    state.seen_hashes.insert(
        hash,
        SeenEntry {
            command: format!("invocation #{}", state.invocation_count),
            invocation_index: state.invocation_count,
        },
    );

    // Stage 4: truncate at byte ceiling (scaled by level).
    // Use config value (max_tokens_per_command × 4 bytes/token) or fall back to default.
    let base_bytes = state
        .max_tokens_per_command
        .saturating_mul(4)
        .max(DEFAULT_MAX_BYTES);
    let max_bytes = match level {
        3 => base_bytes / 4, // ~2K tokens at level 3
        2 => base_bytes / 2, // ~4K tokens at level 2
        _ => base_bytes,     // ~8K tokens at level 1
    };
    truncate_with_hint(&deduped, max_bytes)
}

/// Truncate text to a maximum byte count on a line boundary, appending a hint
/// with the number of omitted lines if any content was cut.
pub fn truncate_with_hint(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }

    let mut cut_at = max_bytes;
    // Walk back to the nearest newline to avoid splitting a line.
    while cut_at > 0 && text.as_bytes().get(cut_at).is_none_or(|&b| b != b'\n') {
        cut_at -= 1;
    }
    if cut_at == 0 {
        cut_at = max_bytes; // no newline found — hard cut
    }

    let kept = &text[..cut_at];
    let omitted_lines = text[cut_at..].lines().count();
    format!(
        "{}\n[grove: +{} lines omitted — rerun for full output]\n",
        kept.trim_end(),
        omitted_lines
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_filter::session::FilterState;

    fn test_state() -> FilterState {
        FilterState::new("test-run".into(), vec![], 200_000)
    }

    #[test]
    fn strip_ansi_codes() {
        let input = "\x1b[31mERROR\x1b[0m: something failed";
        assert_eq!(strip_ansi(input), "ERROR: something failed");
    }

    #[test]
    fn strip_ansi_osc() {
        let input = "\x1b]0;title\x07some text";
        assert_eq!(strip_ansi(input), "some text");
    }

    #[test]
    fn dedup_consecutive() {
        let input = "line1\nline2\nline2\nline2\nline3\n";
        let result = dedup_consecutive_lines(input);
        assert!(result.contains("[grove: previous line repeated 2 times]"));
        assert!(result.contains("line1"));
        assert!(result.contains("line3"));
    }

    #[test]
    fn truncate_long_output() {
        let line = "a".repeat(100) + "\n";
        let text = line.repeat(500); // ~50KB
        let result = truncate_with_hint(&text, 1_000);
        assert!(result.len() < 1_200); // some overhead for the hint
        assert!(result.contains("[grove: +"));
        assert!(result.contains("lines omitted"));
    }

    #[test]
    fn session_dedup_cache_hit() {
        let mut state = test_state();
        let output = "some output text\n";

        let first = filter(output, 1, &mut state);
        assert!(!first.contains("[grove: identical output"));

        // Second invocation with identical content should hit the cache.
        state.invocation_count = 1;
        let second = filter(output, 1, &mut state);
        assert!(second.contains("[grove: identical output"));
    }

    #[test]
    fn level_3_truncates_more_aggressively() {
        // Use unique lines so dedup_consecutive_lines does not collapse them.
        let text: String = (0..500)
            .map(|i| format!("line {} {}\n", i, "x".repeat(100)))
            .collect();
        let mut state1 = test_state();
        let mut state2 = test_state();

        let l1 = filter(&text, 1, &mut state1);
        let l3 = filter(&text, 3, &mut state2);

        assert!(
            l3.len() < l1.len(),
            "Level 3 ({}) should produce shorter output than Level 1 ({})",
            l3.len(),
            l1.len()
        );
    }
}
