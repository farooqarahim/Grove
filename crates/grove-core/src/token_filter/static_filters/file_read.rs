//! File read filter — handles cat, head, tail, less, bat output.
//!
//! Primary purpose: deduplicate repeated reads of the same file content
//! using the session content hash cache.

use sha2::{Digest, Sha256};

use super::super::session::{FilterState, SeenEntry};
use super::universal::strip_ansi;

/// Maximum lines to keep at level 2+.
const DEFAULT_MAX_FILE_LINES: usize = 500;

/// Filter file read output.
pub fn filter(output: &str, level: u8, state: &mut FilterState) -> String {
    let cleaned = strip_ansi(output);

    // Check session dedup cache
    let mut hasher = Sha256::new();
    hasher.update(cleaned.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    if let Some(seen) = state.seen_hashes.get(&hash) {
        return format!(
            "[grove: file content unchanged since {} — skipped]\n",
            seen.command
        );
    }

    // Record in cache
    state.seen_hashes.insert(
        hash,
        SeenEntry {
            command: format!("invocation #{}", state.invocation_count),
            invocation_index: state.invocation_count,
        },
    );

    if level <= 1 {
        return cleaned;
    }

    // Level 2+: truncate at max lines (use config value from state if set).
    let base = if state.max_file_lines > 0 {
        state.max_file_lines
    } else {
        DEFAULT_MAX_FILE_LINES
    };
    let max_lines = if level >= 3 { base / 2 } else { base };

    let lines: Vec<&str> = cleaned.lines().collect();
    if lines.len() <= max_lines {
        return cleaned;
    }

    let kept: String = lines[..max_lines].join("\n");
    let omitted = lines.len() - max_lines;
    format!(
        "{}\n[grove: +{} lines omitted — file has {} total lines]\n",
        kept,
        omitted,
        lines.len()
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
    fn dedup_same_content() {
        let mut state = test_state();
        let content = "fn main() {\n    println!(\"hello\");\n}\n";

        let first = filter(content, 1, &mut state);
        assert!(first.contains("fn main"));

        state.invocation_count = 1;
        let second = filter(content, 1, &mut state);
        assert!(second.contains("[grove: file content unchanged"));
    }

    #[test]
    fn truncate_long_file() {
        let mut state = test_state();
        let line = "some code here\n";
        let content = line.repeat(1000);

        let result = filter(&content, 2, &mut state);
        assert!(result.contains("[grove: +"));
        assert!(result.contains("lines omitted"));
    }

    #[test]
    fn level1_passthrough() {
        let mut state = test_state();
        let content = "line1\nline2\nline3\n";
        let result = filter(content, 1, &mut state);
        assert_eq!(result, content);
    }
}
