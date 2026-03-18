//! Adapter for **Gemini CLI** (Google).
//!
//! CLI reference: `gemini --yolo [--model <model>] -i <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct GeminiAdapter;

impl CodingAgentAdapter for GeminiAdapter {
    fn id(&self) -> &'static str {
        "gemini"
    }

    fn default_command(&self) -> &str {
        "gemini"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, model: Option<&str>, prompt: &str) -> Vec<String> {
        // --yolo: approve all tool calls without confirmation
        // --model <id>: optional model override (e.g. "gemini-2.5-pro")
        // -i <prompt>: prompt passed via flag
        standard_args(
            &[],
            Some("--yolo"),
            Some("--model"),
            model,
            Some("-i"),
            prompt,
            false,
        )
    }
}
