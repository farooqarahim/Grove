//! Adapter for **Kiro** (AWS coding agent CLI).
//!
//! Kiro requires a `chat` subcommand before the prompt.  Model selection is
//! managed internally; no CLI flag is available.
//!
//! CLI reference: `kiro-cli chat <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct KiroAdapter;

impl CodingAgentAdapter for KiroAdapter {
    fn id(&self) -> &'static str {
        "kiro"
    }

    fn default_command(&self) -> &str {
        "kiro-cli"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // "chat" subcommand is required before the prompt.
        // No auto-approve or model flags.
        standard_args(&["chat"], None, None, None, None, prompt, false)
    }
}
