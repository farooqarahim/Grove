/// Hardcoded model registry for supported LLM providers.
///
/// Models are kept as static data to avoid network fetches at startup.
/// Add new models here as providers release them.
///
/// Capabilities a model supports.
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    pub vision: bool,
    pub tools: bool,
    pub reasoning: bool,
}

/// Definition of a single model within a provider.
#[derive(Debug, Clone)]
pub struct ModelDef {
    /// Canonical model ID sent to the API (e.g. `"claude-sonnet-4-6"`).
    pub id: &'static str,
    /// Human-readable display name.
    pub name: &'static str,
    /// Maximum context window in tokens.
    pub context_window: u32,
    /// Maximum output tokens per response.
    pub max_output_tokens: u32,
    /// Cost per million *input* tokens in USD.
    pub cost_input_per_m: f64,
    /// Cost per million *output* tokens in USD.
    pub cost_output_per_m: f64,
    pub capabilities: ModelCapabilities,
}

/// Definition of an LLM provider.
#[derive(Debug, Clone)]
pub struct ProviderDef {
    /// Stable identifier used in auth.json and config (e.g. `"anthropic"`).
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Base URL for API calls (no trailing slash).
    pub base_url: &'static str,
    /// Environment variable that holds the API key (e.g. `"ANTHROPIC_API_KEY"`).
    pub env_key: &'static str,
    /// Ordered list of models (newest / most capable first).
    pub models: &'static [ModelDef],
}

// ── Anthropic ─────────────────────────────────────────────────────────────────

pub static ANTHROPIC_MODELS: &[ModelDef] = &[
    ModelDef {
        id: "claude-opus-4-6",
        name: "Claude Opus 4.6",
        context_window: 200_000,
        max_output_tokens: 32_000,
        cost_input_per_m: 15.0,
        cost_output_per_m: 75.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "claude-sonnet-4-6",
        name: "Claude Sonnet 4.6",
        context_window: 200_000,
        max_output_tokens: 64_000,
        cost_input_per_m: 3.0,
        cost_output_per_m: 15.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "claude-haiku-4-5-20251001",
        name: "Claude Haiku 4.5",
        context_window: 200_000,
        max_output_tokens: 16_000,
        cost_input_per_m: 0.80,
        cost_output_per_m: 4.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "claude-3-5-sonnet-20241022",
        name: "Claude 3.5 Sonnet",
        context_window: 200_000,
        max_output_tokens: 8_192,
        cost_input_per_m: 3.0,
        cost_output_per_m: 15.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "claude-3-5-haiku-20241022",
        name: "Claude 3.5 Haiku",
        context_window: 200_000,
        max_output_tokens: 8_192,
        cost_input_per_m: 1.0,
        cost_output_per_m: 5.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "claude-3-opus-20240229",
        name: "Claude 3 Opus",
        context_window: 200_000,
        max_output_tokens: 4_096,
        cost_input_per_m: 15.0,
        cost_output_per_m: 75.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
];

pub static ANTHROPIC: ProviderDef = ProviderDef {
    id: "anthropic",
    name: "Anthropic",
    base_url: "https://api.anthropic.com",
    env_key: "ANTHROPIC_API_KEY",
    models: ANTHROPIC_MODELS,
};

// ── OpenAI ────────────────────────────────────────────────────────────────────

pub static OPENAI_MODELS: &[ModelDef] = &[
    ModelDef {
        id: "gpt-4.1",
        name: "GPT-4.1",
        context_window: 1_047_576,
        max_output_tokens: 32_768,
        cost_input_per_m: 2.0,
        cost_output_per_m: 8.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "gpt-4.1-mini",
        name: "GPT-4.1 Mini",
        context_window: 1_047_576,
        max_output_tokens: 32_768,
        cost_input_per_m: 0.40,
        cost_output_per_m: 1.60,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "gpt-4o",
        name: "GPT-4o",
        context_window: 128_000,
        max_output_tokens: 16_384,
        cost_input_per_m: 2.50,
        cost_output_per_m: 10.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "gpt-4o-mini",
        name: "GPT-4o Mini",
        context_window: 128_000,
        max_output_tokens: 16_384,
        cost_input_per_m: 0.15,
        cost_output_per_m: 0.60,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "o1",
        name: "o1",
        context_window: 200_000,
        max_output_tokens: 100_000,
        cost_input_per_m: 15.0,
        cost_output_per_m: 60.0,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: true,
        },
    },
    ModelDef {
        id: "o3-mini",
        name: "o3-mini",
        context_window: 200_000,
        max_output_tokens: 100_000,
        cost_input_per_m: 1.10,
        cost_output_per_m: 4.40,
        capabilities: ModelCapabilities {
            vision: false,
            tools: true,
            reasoning: true,
        },
    },
    ModelDef {
        id: "o4-mini",
        name: "o4-mini",
        context_window: 200_000,
        max_output_tokens: 100_000,
        cost_input_per_m: 1.10,
        cost_output_per_m: 4.40,
        capabilities: ModelCapabilities {
            vision: true,
            tools: true,
            reasoning: true,
        },
    },
];

pub static OPENAI: ProviderDef = ProviderDef {
    id: "openai",
    name: "OpenAI",
    base_url: "https://api.openai.com",
    env_key: "OPENAI_API_KEY",
    models: OPENAI_MODELS,
};

// ── DeepSeek ──────────────────────────────────────────────────────────────────

pub static DEEPSEEK_MODELS: &[ModelDef] = &[
    ModelDef {
        id: "deepseek-chat",
        name: "DeepSeek V3 (Chat)",
        context_window: 64_000,
        max_output_tokens: 8_192,
        cost_input_per_m: 0.27,
        cost_output_per_m: 1.10,
        capabilities: ModelCapabilities {
            vision: false,
            tools: true,
            reasoning: false,
        },
    },
    ModelDef {
        id: "deepseek-reasoner",
        name: "DeepSeek R1 (Reasoner)",
        context_window: 64_000,
        max_output_tokens: 8_192,
        cost_input_per_m: 0.55,
        cost_output_per_m: 2.19,
        capabilities: ModelCapabilities {
            vision: false,
            tools: false,
            reasoning: true,
        },
    },
];

/// DeepSeek uses the OpenAI-compatible Chat Completions API.
pub static DEEPSEEK: ProviderDef = ProviderDef {
    id: "deepseek",
    name: "DeepSeek",
    base_url: "https://api.deepseek.com",
    env_key: "DEEPSEEK_API_KEY",
    models: DEEPSEEK_MODELS,
};

// ── Registry ──────────────────────────────────────────────────────────────────

// ── Inception Labs (Mercury) ───────────────────────────────────────────────────

pub static INCEPTION_MODELS: &[ModelDef] = &[
    ModelDef {
        id: "mercury-2",
        name: "Mercury 2 (Chat & Reasoning)",
        context_window: 32_768,
        max_output_tokens: 8_192,
        cost_input_per_m: 0.25,
        cost_output_per_m: 1.0,
        capabilities: ModelCapabilities {
            vision: false,
            tools: true,
            reasoning: true,
        },
    },
    ModelDef {
        id: "mercury-edit",
        name: "Mercury Edit (Code Editing)",
        context_window: 32_768,
        max_output_tokens: 8_192,
        cost_input_per_m: 0.25,
        cost_output_per_m: 1.0,
        capabilities: ModelCapabilities {
            vision: false,
            tools: false,
            reasoning: false,
        },
    },
];

/// Inception Labs uses an OpenAI-compatible Chat Completions API.
pub static INCEPTION: ProviderDef = ProviderDef {
    id: "inception",
    name: "Inception Labs",
    base_url: "https://api.inceptionlabs.ai/v1",
    env_key: "INCEPTION_API_KEY",
    models: INCEPTION_MODELS,
};

// ── Registry ──────────────────────────────────────────────────────────────────

/// All supported providers in display order.
pub static ALL_PROVIDERS: &[&ProviderDef] = &[&ANTHROPIC, &OPENAI, &DEEPSEEK, &INCEPTION];

/// Look up a provider by its `id` string.
pub fn find_provider(id: &str) -> Option<&'static ProviderDef> {
    ALL_PROVIDERS.iter().copied().find(|p| p.id == id)
}

/// Look up a model by `provider_id` and `model_id`.
pub fn find_model(provider_id: &str, model_id: &str) -> Option<&'static ModelDef> {
    find_provider(provider_id)?
        .models
        .iter()
        .find(|m| m.id == model_id)
}

/// Infer the provider for a model ID by scanning all providers.
///
/// Returns the first provider whose model list contains `model_id`.
pub fn provider_for_model(model_id: &str) -> Option<&'static ProviderDef> {
    ALL_PROVIDERS
        .iter()
        .copied()
        .find(|p| p.models.iter().any(|m| m.id == model_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_providers_have_models() {
        for p in ALL_PROVIDERS {
            assert!(!p.models.is_empty(), "provider {} has no models", p.id);
        }
    }

    #[test]
    fn find_provider_works() {
        assert!(find_provider("anthropic").is_some());
        assert!(find_provider("openai").is_some());
        assert!(find_provider("deepseek").is_some());
        assert!(find_provider("inception").is_some());
        assert!(find_provider("unknown").is_none());
    }

    #[test]
    fn find_model_works() {
        assert!(find_model("anthropic", "claude-sonnet-4-6").is_some());
        assert!(find_model("openai", "gpt-4o").is_some());
        assert!(find_model("deepseek", "deepseek-chat").is_some());
        assert!(find_model("anthropic", "gpt-4o").is_none());
    }

    #[test]
    fn provider_for_model_infers_correctly() {
        assert_eq!(
            provider_for_model("claude-sonnet-4-6").map(|p| p.id),
            Some("anthropic")
        );
        assert_eq!(provider_for_model("gpt-4o").map(|p| p.id), Some("openai"));
        assert_eq!(
            provider_for_model("deepseek-chat").map(|p| p.id),
            Some("deepseek")
        );
        assert_eq!(
            provider_for_model("mercury-2").map(|p| p.id),
            Some("inception")
        );
        assert!(provider_for_model("unknown-model").is_none());
    }

    #[test]
    fn costs_are_positive() {
        for p in ALL_PROVIDERS {
            for m in p.models {
                assert!(
                    m.cost_input_per_m >= 0.0,
                    "{}/{} negative input cost",
                    p.id,
                    m.id
                );
                assert!(
                    m.cost_output_per_m >= 0.0,
                    "{}/{} negative output cost",
                    p.id,
                    m.id
                );
            }
        }
    }
}
