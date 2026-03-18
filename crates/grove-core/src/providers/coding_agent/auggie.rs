//! Adapter for **Auggie** (Augment Code CLI agent).
//!
//! Requires `--allow-indexing` to permit the agent to index the codebase.
//! Model selection is managed by Augment Code's configuration; no CLI flag is
//! available.
//!
//! CLI reference: `auggie --allow-indexing <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct AuggieAdapter;

impl CodingAgentAdapter for AuggieAdapter {
    fn id(&self) -> &'static str {
        "auggie"
    }

    fn default_command(&self) -> &str {
        "auggie"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // --allow-indexing: allow the agent to index and read the codebase
        // No auto-approve or model flags; model is set in Augment Code settings.
        standard_args(&["--allow-indexing"], None, None, None, None, prompt, false)
    }
}
