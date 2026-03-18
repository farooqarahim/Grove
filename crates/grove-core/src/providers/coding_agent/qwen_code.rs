//! Adapter for **Qwen Code** (Alibaba Cloud coding CLI).
//!
//! CLI reference: `qwen --yolo [--model <model>] -i <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct QwenCodeAdapter;

impl CodingAgentAdapter for QwenCodeAdapter {
    fn id(&self) -> &'static str {
        "qwen_code"
    }

    fn default_command(&self) -> &str {
        "qwen"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, model: Option<&str>, prompt: &str) -> Vec<String> {
        // --yolo: approve all tool calls without confirmation
        // --model <id>: optional model override (e.g. "qwen3-coder")
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
