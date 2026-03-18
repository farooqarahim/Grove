//! Adapter for **Cursor** agent CLI.
//!
//! Model selection is managed internally by Cursor; no CLI flag is available.
//!
//! CLI reference: `cursor-agent -f <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct CursorAdapter;

impl CodingAgentAdapter for CursorAdapter {
    fn id(&self) -> &'static str {
        "cursor"
    }

    fn default_command(&self) -> &str {
        "cursor-agent"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, _model: Option<&str>, prompt: &str) -> Vec<String> {
        // -f: full/force mode — approves all tool operations
        // model selection is not supported via CLI; Cursor uses its internal setting.
        // prompt is the last positional argument.
        standard_args(&[], Some("-f"), None, None, None, prompt, false)
    }
}
