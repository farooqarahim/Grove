//! Adapter for **Continue** (open-source AI code assistant CLI).
//!
//! `continue` is a Rust reserved keyword so this module is named `continue_agent`.
//!
//! Model selection is managed through Continue's `config.yaml`; no CLI flag is
//! available.  The prompt is passed via the `-p` flag.
//!
//! CLI reference: `cn -p <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct ContinueAdapter;

impl CodingAgentAdapter for ContinueAdapter {
    fn id(&self) -> &'static str {
        "continue"
    }

    fn default_command(&self) -> &str {
        "cn"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // -p <prompt>: pass the prompt via flag
        // No auto-approve or model flags; Continue uses its config file for these.
        standard_args(&[], None, None, None, Some("-p"), prompt, false)
    }
}
