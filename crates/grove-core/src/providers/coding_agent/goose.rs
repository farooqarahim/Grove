//! Adapter for **Goose** (Block coding agent).
//!
//! Goose manages its own model and tool configuration; no CLI overrides needed.
//! Operates non-interactively by default.
//!
//! CLI reference: `goose <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct GooseAdapter;

impl CodingAgentAdapter for GooseAdapter {
    fn id(&self) -> &'static str {
        "goose"
    }

    fn default_command(&self) -> &str {
        "goose"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // Goose does not expose model selection or auto-approve via CLI flags.
        // The prompt is the single positional argument.
        standard_args(&[], None, None, None, None, prompt, false)
    }
}
