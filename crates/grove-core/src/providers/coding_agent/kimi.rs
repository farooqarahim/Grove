//! Adapter for **Kimi** (Moonshot AI coding CLI).
//!
//! CLI reference: `kimi --yolo [--model <model>] -c <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct KimiAdapter;

impl CodingAgentAdapter for KimiAdapter {
    fn id(&self) -> &'static str {
        "kimi"
    }

    fn default_command(&self) -> &str {
        "kimi"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, model: Option<&str>, prompt: &str) -> Vec<String> {
        // --yolo: approve all tool calls without confirmation
        // --model <id>: optional model override (e.g. "kimi-k2")
        // -c <prompt>: prompt passed via flag
        standard_args(
            &[],
            Some("--yolo"),
            Some("--model"),
            model,
            Some("-c"),
            prompt,
            false,
        )
    }
}
