//! Adapter for **Kilocode** coding agent CLI.
//!
//! Model selection is managed internally; no CLI flag is available.
//!
//! CLI reference: `kilocode --auto <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct KilocodeAdapter;

impl CodingAgentAdapter for KilocodeAdapter {
    fn id(&self) -> &'static str {
        "kilocode"
    }

    fn default_command(&self) -> &str {
        "kilocode"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // --auto: non-interactive / auto-approve mode
        // No model flag; model is managed internally.
        // prompt is the last positional argument.
        standard_args(&[], Some("--auto"), None, None, None, prompt, false)
    }
}
