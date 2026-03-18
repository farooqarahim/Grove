//! Adapter for **Amp** (Sourcegraph coding agent).
//!
//! Amp manages its own model configuration; no model flag is available via CLI.
//! No auto-approve flag is needed — Amp operates non-interactively by default.
//!
//! CLI reference: `amp <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct AmpAdapter;

impl CodingAgentAdapter for AmpAdapter {
    fn id(&self) -> &'static str {
        "amp"
    }

    fn default_command(&self) -> &str {
        "amp"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // Amp does not expose model selection or auto-approve via CLI flags.
        // The prompt is the single positional argument.
        standard_args(&[], None, None, None, None, prompt, false)
    }
}
