//! Adapter for **GitHub Copilot** CLI agent.
//!
//! Model selection is managed by GitHub Copilot internally; no CLI flag is available.
//!
//! CLI reference: `copilot --allow-all-tools <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct CopilotAdapter;

impl CodingAgentAdapter for CopilotAdapter {
    fn id(&self) -> &'static str {
        "copilot"
    }

    fn default_command(&self) -> &str {
        "copilot"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // --allow-all-tools: grant unrestricted tool access without confirmation
        // model selection is not supported via CLI; Copilot uses its internal model.
        // prompt is the last positional argument.
        standard_args(
            &[],
            Some("--allow-all-tools"),
            None,
            None,
            None,
            prompt,
            false,
        )
    }
}
