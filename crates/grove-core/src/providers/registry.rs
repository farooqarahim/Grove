use std::collections::HashMap;

use crate::config::GroveConfig;
use crate::errors::{GroveError, GroveResult};

use super::{
    Provider,
    claude_code::ClaudeCodeProvider,
    coding_agent::{CodingAgentProvider, GenericAdapter, get_adapter},
    mock::MockProvider,
};

/// Holds named provider instances.
pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn Provider>>,
}

impl ProviderRegistry {
    /// Build a registry pre-populated with the standard providers.
    pub fn from_config(cfg: &GroveConfig) -> Self {
        let mut reg = Self {
            providers: HashMap::new(),
        };
        if cfg.providers.mock.enabled {
            reg.register("mock", Box::new(MockProvider));
        }
        reg.register(
            "claude_code",
            Box::new(
                ClaudeCodeProvider::new(
                    cfg.providers.claude_code.command.clone(),
                    cfg.providers.claude_code.timeout_seconds,
                    cfg.providers.claude_code.permission_mode.clone(),
                    cfg.providers.claude_code.allowed_tools.clone(),
                    cfg.providers.claude_code.gatekeeper_model.clone(),
                )
                .with_max_output_bytes(cfg.providers.claude_code.max_output_bytes)
                .with_resource_limits(
                    cfg.providers.claude_code.max_file_size_mb,
                    cfg.providers.claude_code.max_open_files,
                ),
            ),
        );
        for (id, agent_cfg) in &cfg.providers.coding_agents {
            if !agent_cfg.enabled {
                continue;
            }

            // Prefer a dedicated adapter; fall back to GenericAdapter for
            // custom agents defined only in grove.yaml.
            let adapter: Box<dyn super::coding_agent::CodingAgentAdapter> =
                if let Some(a) = get_adapter(id) {
                    a
                } else {
                    Box::new(GenericAdapter::from_config(
                        id.clone(),
                        agent_cfg.command.clone(),
                        agent_cfg.auto_approve_flag.clone(),
                        agent_cfg.initial_prompt_flag.clone(),
                        agent_cfg.use_keystroke_injection,
                        agent_cfg.use_pty,
                        agent_cfg.default_args.clone(),
                        agent_cfg.model_flag.clone(),
                    ))
                };

            // Command: grove.yaml override if set to a non-default path, otherwise
            // use the adapter's own default.
            let command = agent_cfg.command.clone();

            reg.register(
                id,
                Box::new(
                    CodingAgentProvider::new(adapter, command, agent_cfg.timeout_seconds)
                        .with_max_output_bytes(agent_cfg.max_output_bytes)
                        .with_resource_limits(agent_cfg.max_file_size_mb, agent_cfg.max_open_files),
                ),
            );
        }

        reg
    }

    /// Register a provider under `name`. Overwrites any existing entry.
    pub fn register(&mut self, name: &str, provider: Box<dyn Provider>) {
        self.providers.insert(name.to_string(), provider);
    }

    /// Retrieve a provider by name.
    pub fn get(&self, name: &str) -> GroveResult<&dyn Provider> {
        self.providers.get(name).map(|p| p.as_ref()).ok_or_else(|| {
            GroveError::Config(format!(
                "unknown provider '{name}'; available: {}",
                self.providers
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
    }

    /// Return the default provider as configured.
    pub fn default_provider<'a>(&'a self, cfg: &GroveConfig) -> GroveResult<&'a dyn Provider> {
        self.get(&cfg.providers.default)
    }
}
