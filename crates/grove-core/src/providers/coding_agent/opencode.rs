//! Adapter for **OpenCode** (TUI-based coding agent).
//!
//! OpenCode is a terminal UI that does not accept a prompt on the command line.
//! Instead, the prompt text is written to the process's stdin after startup
//! (`ExecutionMode::StdinInjection`).  Model selection is configured inside
//! OpenCode itself (not via CLI flag).
//!
//! CLI reference: `opencode`  (prompt → stdin)

use super::adapter::{CodingAgentAdapter, ExecutionMode};

pub struct OpenCodeAdapter;

impl CodingAgentAdapter for OpenCodeAdapter {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn default_command(&self) -> &str {
        "opencode"
    }

    fn execution_mode(&self) -> ExecutionMode {
        // Prompt is injected via stdin after the process starts; no CLI args needed.
        ExecutionMode::StdinInjection
    }

    fn build_args(&self, _model: Option<&str>, _prompt: &str) -> Vec<String> {
        // No arguments — opencode is launched bare and receives the prompt via stdin.
        vec![]
    }
}
