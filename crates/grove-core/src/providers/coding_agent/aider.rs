//! Adapter for **Aider** (open-source pair programmer).
//!
//! Aider is provider-agnostic and works with Claude, GPT-4o, Gemini, and others.
//! The model is passed as a fully-qualified string understood by aider's LiteLLM
//! backend (e.g. `"claude-sonnet-4-6"`, `"gpt-4o"`, `"gemini/gemini-2.5-pro"`).
//!
//! CLI reference: `aider --yes [--model <model>] --message <prompt>`

use super::adapter::{CodingAgentAdapter, ExecutionMode, standard_args};

pub struct AiderAdapter;

impl CodingAgentAdapter for AiderAdapter {
    fn id(&self) -> &'static str {
        "aider"
    }

    fn default_command(&self) -> &str {
        "aider"
    }

    fn execution_mode(&self) -> ExecutionMode {
        ExecutionMode::Pipe
    }

    fn build_args(&self, model: Option<&str>, prompt: &str) -> Vec<String> {
        // --yes: auto-confirm all prompts (non-interactive)
        // --model <id>: optional model override via LiteLLM identifier
        // --message <prompt>: the task description
        standard_args(
            &[],
            Some("--yes"),
            Some("--model"),
            model,
            Some("--message"),
            prompt,
            false,
        )
    }
}
