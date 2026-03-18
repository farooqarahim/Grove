//! Adapter for **Cline** (VS Code extension CLI wrapper).
//!
//! Model selection is managed through Cline's VS Code settings; no CLI flag is
//! available.
//!
//! CLI reference: `cline --yolo <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct ClineAdapter;

impl CodingAgentAdapter for ClineAdapter {
    fn id(&self) -> &'static str {
        "cline"
    }

    fn default_command(&self) -> &str {
        "cline"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // --yolo: approve all tool calls without confirmation
        // model selection is not supported via CLI; use Cline's VS Code settings.
        // prompt is the last positional argument.
        standard_args(&[], Some("--yolo"), None, None, None, prompt, false)
    }
}
