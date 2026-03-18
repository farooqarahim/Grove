/// Estimate token count from text using the ~4 bytes/token heuristic.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().saturating_div(4).max(1)
}

/// Estimate token count from a raw byte count (no text needed).
pub fn estimate_tokens_from_bytes(byte_count: usize) -> usize {
    byte_count.saturating_div(4).max(1)
}

/// Return the approximate context window size for a given model string.
pub fn model_window_size(model: &str) -> usize {
    let m = model.to_lowercase();
    if m.contains("gemini") {
        1_000_000
    } else if m.contains("claude") {
        200_000
    } else {
        128_000
    }
}

/// Determine the compression level based on how much of the context window
/// has been consumed.
///
/// - Level 1 (<50% used): Light — ANSI strip + extreme outlier truncation
/// - Level 2 (50–75% used): Full per-command filters
/// - Level 3 (>75% used): Summary only — counts and file names, no raw content
pub fn compute_level(tokens_used: usize, window_size: usize) -> u8 {
    let pct = tokens_used.saturating_mul(100) / window_size.max(1);
    if pct >= 75 {
        3
    } else if pct >= 50 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_tokens_basic() {
        assert_eq!(estimate_tokens("hello world!"), 3); // 12 bytes / 4
    }

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(estimate_tokens(""), 1); // min 1
    }

    #[test]
    fn model_window_sizes() {
        assert_eq!(model_window_size("claude-sonnet-4-6"), 200_000);
        assert_eq!(model_window_size("gemini-2.5-pro"), 1_000_000);
        assert_eq!(model_window_size("codex-mini"), 128_000);
        assert_eq!(model_window_size("unknown-model"), 128_000);
    }

    #[test]
    fn compression_levels() {
        assert_eq!(compute_level(0, 200_000), 1);
        assert_eq!(compute_level(49_999, 200_000), 1);
        assert_eq!(compute_level(100_000, 200_000), 2);
        assert_eq!(compute_level(150_000, 200_000), 3);
        assert_eq!(compute_level(200_000, 200_000), 3);
    }

    #[test]
    fn compute_level_zero_window() {
        // Should not panic with zero window.
        assert_eq!(compute_level(100, 0), 3);
    }
}
