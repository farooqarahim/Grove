//! Per-command output filters dispatched by command name.

pub mod cargo;
pub mod file_read;
pub mod git;
pub mod go;
pub mod node;
pub mod pytest;
pub mod universal;

use super::session::FilterState;

/// Result of applying a filter to command output.
pub struct FilterResult {
    pub text: String,
    pub filter_type: String,
    pub raw_bytes: usize,
    pub filtered_bytes: usize,
}

/// Dispatch to the appropriate filter based on command name.
///
/// Falls back to the universal filter for unrecognised commands.
pub fn apply(command_name: &str, output: &str, level: u8, state: &mut FilterState) -> FilterResult {
    let raw_bytes = output.len();

    let (text, filter_type) = match command_name {
        "git" => (git::filter(output, level, state), "git"),
        "cargo" => (cargo::filter(output, level), "cargo"),
        "pytest" | "python" | "ruff" | "mypy" => (pytest::filter(output, level), "pytest"),
        "tsc" | "eslint" | "vitest" | "jest" | "npx" | "node" | "next" | "pnpm" | "npm"
        | "yarn" | "bun" => (node::filter(output, level), "node"),
        "go" => (go::filter(output, level), "go"),
        "cat" | "head" | "tail" | "less" | "bat" => {
            (file_read::filter(output, level, state), "file_read")
        }
        _ => (universal::filter(output, level, state), "universal"),
    };

    let filtered_bytes = text.len();
    FilterResult {
        text,
        filter_type: filter_type.to_string(),
        raw_bytes,
        filtered_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token_filter::session::FilterState;

    fn test_state() -> FilterState {
        FilterState::new("test-run".into(), vec![], 200_000)
    }

    #[test]
    fn dispatch_git() {
        let mut state = test_state();
        let result = apply("git", "diff --git a/foo\n", 1, &mut state);
        assert_eq!(result.filter_type, "git");
    }

    #[test]
    fn dispatch_unknown_falls_back_to_universal() {
        let mut state = test_state();
        let result = apply("some-random-tool", "output\n", 1, &mut state);
        assert_eq!(result.filter_type, "universal");
    }
}
