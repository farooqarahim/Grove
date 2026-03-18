use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::project_type::ProjectType;

/// Per-command statistics collected during a filter session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandStat {
    pub command: String,
    pub filter_type: String,
    pub raw_bytes: usize,
    pub filtered_bytes: usize,
    pub compression_level: u8,
}

/// Entry in the content deduplication cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeenEntry {
    pub command: String,
    pub invocation_index: usize,
}

/// Persistent session state shared between the grove-filter binary invocations
/// within a single agent run. Serialized as JSON to `.grove-filter-state.json`
/// in the worktree root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterState {
    pub run_id: String,
    pub project_types: Vec<ProjectType>,
    pub compression_level: u8,
    pub tokens_used: usize,
    pub window_size: usize,
    /// Content SHA-256 hash → previous occurrence info.
    pub seen_hashes: HashMap<String, SeenEntry>,
    /// Running count of command invocations (monotonically increasing).
    pub invocation_count: usize,
    /// Per-command raw/filtered byte statistics.
    pub stats: Vec<CommandStat>,

    // ── Configurable limits (from TokenFilterConfig) ─────────────────────
    /// Maximum tokens per command before truncation (~8K default × 4 bytes).
    #[serde(default = "default_max_tokens")]
    pub max_tokens_per_command: usize,
    /// Max lines per diff hunk before truncation.
    #[serde(default = "default_max_hunk_lines")]
    pub max_hunk_lines: usize,
    /// Max commits shown in git log.
    #[serde(default = "default_max_commits")]
    pub max_commits: usize,
    /// Max total diff lines.
    #[serde(default = "default_max_diff_lines")]
    pub max_diff_lines: usize,
    /// Max file lines for file_read filter.
    #[serde(default = "default_max_file_lines")]
    pub max_file_lines: usize,
}

fn default_max_tokens() -> usize {
    8_000
}
fn default_max_hunk_lines() -> usize {
    30
}
fn default_max_commits() -> usize {
    10
}
fn default_max_diff_lines() -> usize {
    500
}
fn default_max_file_lines() -> usize {
    500
}

impl FilterState {
    /// Create a fresh session state for a new run.
    pub fn new(run_id: String, project_types: Vec<ProjectType>, window_size: usize) -> Self {
        Self {
            run_id,
            project_types,
            compression_level: 1,
            tokens_used: 0,
            window_size,
            seen_hashes: HashMap::new(),
            invocation_count: 0,
            stats: Vec::new(),
            max_tokens_per_command: default_max_tokens(),
            max_hunk_lines: default_max_hunk_lines(),
            max_commits: default_max_commits(),
            max_diff_lines: default_max_diff_lines(),
            max_file_lines: default_max_file_lines(),
        }
    }

    /// Create state with custom config limits.
    pub fn with_config(
        run_id: String,
        project_types: Vec<ProjectType>,
        window_size: usize,
        config: &crate::config::TokenFilterConfig,
    ) -> Self {
        let mut state = Self::new(run_id, project_types, window_size);
        state.max_tokens_per_command = config.max_tokens_per_command;
        state.max_hunk_lines = config.max_hunk_lines;
        state.max_commits = config.max_commits;
        state.max_diff_lines = config.max_diff_lines;
        state.max_file_lines = config.max_file_lines;
        state
    }

    /// Load state from a JSON file. Returns `None` on any read/parse failure.
    pub fn load(path: &Path) -> Option<Self> {
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Write state to a JSON file. Best-effort — errors are logged but not fatal.
    pub fn save(&self, path: &Path) {
        match serde_json::to_string(self) {
            Ok(json) => {
                if let Err(e) = fs::write(path, json) {
                    tracing::warn!(error = %e, "failed to write filter state");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to serialize filter state");
            }
        }
    }

    /// Record a completed command invocation and update compression level.
    pub fn record_invocation(&mut self, stat: CommandStat) {
        self.tokens_used += super::token_count::estimate_tokens_from_bytes(stat.filtered_bytes);
        self.invocation_count += 1;
        self.compression_level =
            super::token_count::compute_level(self.tokens_used, self.window_size);
        self.stats.push(stat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_state() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");

        let mut state = FilterState::new("run-1".into(), vec![ProjectType::Rust], 200_000);
        state.record_invocation(CommandStat {
            command: "cargo test".into(),
            filter_type: "cargo".into(),
            raw_bytes: 10_000,
            filtered_bytes: 2_000,
            compression_level: 1,
        });
        state.save(&path);

        let loaded = FilterState::load(&path).unwrap();
        assert_eq!(loaded.run_id, "run-1");
        assert_eq!(loaded.stats.len(), 1);
        assert_eq!(loaded.invocation_count, 1);
    }

    #[test]
    fn load_missing_file_returns_none() {
        assert!(FilterState::load(Path::new("/nonexistent/state.json")).is_none());
    }
}
