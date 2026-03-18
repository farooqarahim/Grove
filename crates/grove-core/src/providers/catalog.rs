use serde::Serialize;

/// A single selectable model for an agent.
#[derive(Debug, Clone, Serialize)]
pub struct ModelEntry {
    /// The model ID passed to the CLI (e.g. `"claude-sonnet-4-6"`, `"o3"`).
    pub id: &'static str,
    /// Short display name shown in the UI (e.g. `"Sonnet"`, `"O3"`).
    pub name: &'static str,
    /// One-line description of the model's characteristics.
    pub description: &'static str,
    /// Whether this is the agent's default when no model is specified.
    pub is_default: bool,
}

/// Per-agent catalog entry returned to the GUI for the agent + model selector.
#[derive(Debug, Clone, Serialize)]
pub struct AgentCatalogEntry {
    /// Provider ID matching `providers.default` / `providers.coding_agents` keys.
    pub id: &'static str,
    /// Human-readable agent name.
    pub name: &'static str,
    /// CLI binary name.
    pub cli: &'static str,
    /// CLI flag used to pass the model (e.g. `"--model"`). `None` = model selection not
    /// supported for this agent; the agent uses its own internally configured model.
    pub model_flag: Option<&'static str>,
    /// CLI flag that grants unrestricted tool use (e.g. `"--full-auto"`, `"--yolo"`).
    /// Used as the default when the agent is not configured in grove.yaml.
    pub auto_approve_flag: Option<&'static str>,
    /// CLI flag that precedes the initial prompt (e.g. `"-i"`, `"-p"`).
    /// `None` = prompt is the last positional argument.
    pub initial_prompt_flag: Option<&'static str>,
    /// When `true`, the agent must be spawned inside a PTY because it checks isatty(stdout).
    pub use_pty: bool,
    /// Available models. Empty = agent manages model selection internally.
    pub models: &'static [ModelEntry],
}

/// Returns the full static catalog of all supported agents and their models.
/// Called by the `get_agent_catalog` Tauri command.
pub fn all_agents() -> &'static [AgentCatalogEntry] {
    &CATALOG
}

/// Look up a single entry by provider id.
pub fn get_agent(id: &str) -> Option<&'static AgentCatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

// ── Static catalog ─────────────────────────────────────────────────────────────

static CATALOG: &[AgentCatalogEntry] = &[
    AgentCatalogEntry {
        id: "claude_code",
        name: "Claude Code",
        cli: "claude",
        model_flag: Some("--model"),
        auto_approve_flag: Some("--dangerously-skip-permissions"),
        initial_prompt_flag: None,
        use_pty: false,
        models: &[
            ModelEntry {
                id: "claude-opus-4-6",
                name: "Opus",
                description: "Most powerful — best for complex, multi-step tasks",
                is_default: false,
            },
            ModelEntry {
                id: "claude-sonnet-4-6",
                name: "Sonnet",
                description: "Balanced speed and intelligence — recommended default",
                is_default: true,
            },
            ModelEntry {
                id: "claude-haiku-4-5-20251001",
                name: "Haiku",
                description: "Fast and lightweight — ideal for simple tasks",
                is_default: false,
            },
        ],
    },
    AgentCatalogEntry {
        id: "codex",
        name: "Codex (OpenAI)",
        cli: "codex",
        model_flag: Some("--model"),
        auto_approve_flag: Some("--full-auto"),
        initial_prompt_flag: None,
        use_pty: true, // codex checks isatty(stdout) and refuses to run without a TTY
        models: &[
            ModelEntry {
                id: "o4-mini",
                name: "O4-mini",
                description: "Fast and cost-efficient reasoning — codex default",
                is_default: true,
            },
            ModelEntry {
                id: "o3",
                name: "O3",
                description: "OpenAI's most powerful reasoning model",
                is_default: false,
            },
            ModelEntry {
                id: "gpt-4.1",
                name: "GPT-4.1",
                description: "Latest GPT-4 generation, general purpose",
                is_default: false,
            },
        ],
    },
    AgentCatalogEntry {
        id: "gemini",
        name: "Gemini (Google)",
        cli: "gemini",
        model_flag: Some("--model"),
        auto_approve_flag: Some("--yolo"),
        initial_prompt_flag: Some("-i"),
        use_pty: false,
        models: &[
            ModelEntry {
                id: "gemini-2.5-pro",
                name: "Gemini 2.5 Pro",
                description: "Most capable Gemini model",
                is_default: true,
            },
            ModelEntry {
                id: "gemini-2.5-flash",
                name: "Gemini 2.5 Flash",
                description: "Fast, efficient — great for most tasks",
                is_default: false,
            },
            ModelEntry {
                id: "gemini-2.0-flash",
                name: "Gemini 2.0 Flash",
                description: "Previous generation flash model",
                is_default: false,
            },
        ],
    },
    AgentCatalogEntry {
        id: "aider",
        name: "Aider",
        cli: "aider",
        model_flag: Some("--model"),
        auto_approve_flag: Some("--yes"),
        initial_prompt_flag: Some("--message"),
        use_pty: false,
        models: &[
            ModelEntry {
                id: "claude-sonnet-4-6",
                name: "Claude Sonnet",
                description: "Anthropic Sonnet via aider",
                is_default: true,
            },
            ModelEntry {
                id: "gpt-4o",
                name: "GPT-4o",
                description: "OpenAI GPT-4o via aider",
                is_default: false,
            },
            ModelEntry {
                id: "gemini/gemini-2.5-pro",
                name: "Gemini 2.5 Pro",
                description: "Google Gemini 2.5 Pro via aider",
                is_default: false,
            },
        ],
    },
    AgentCatalogEntry {
        id: "cursor",
        name: "Cursor",
        cli: "cursor-agent",
        model_flag: None,
        auto_approve_flag: Some("-f"),
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "copilot",
        name: "GitHub Copilot",
        cli: "copilot",
        model_flag: None,
        auto_approve_flag: Some("--allow-all-tools"),
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "qwen_code",
        name: "Qwen Code",
        cli: "qwen",
        model_flag: Some("--model"),
        auto_approve_flag: Some("--yolo"),
        initial_prompt_flag: Some("-i"),
        use_pty: false,
        models: &[ModelEntry {
            id: "qwen3-coder",
            name: "Qwen3 Coder",
            description: "Alibaba's latest coding-optimised model",
            is_default: true,
        }],
    },
    AgentCatalogEntry {
        id: "opencode",
        name: "OpenCode",
        cli: "opencode",
        model_flag: None,
        auto_approve_flag: None,
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "kimi",
        name: "Kimi",
        cli: "kimi",
        model_flag: Some("--model"),
        auto_approve_flag: Some("--yolo"),
        initial_prompt_flag: Some("-c"),
        use_pty: false,
        models: &[ModelEntry {
            id: "kimi-k2",
            name: "Kimi K2",
            description: "Moonshot AI's coding-optimised model",
            is_default: true,
        }],
    },
    AgentCatalogEntry {
        id: "amp",
        name: "Amp",
        cli: "amp",
        model_flag: None,
        auto_approve_flag: None,
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "goose",
        name: "Goose",
        cli: "goose",
        model_flag: None,
        auto_approve_flag: None,
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "cline",
        name: "Cline",
        cli: "cline",
        model_flag: None,
        auto_approve_flag: Some("--yolo"),
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "continue",
        name: "Continue",
        cli: "cn",
        model_flag: None,
        auto_approve_flag: None,
        initial_prompt_flag: Some("-p"),
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "kiro",
        name: "Kiro (AWS)",
        cli: "kiro-cli",
        model_flag: None,
        auto_approve_flag: None,
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "auggie",
        name: "Auggie (Augment Code)",
        cli: "auggie",
        model_flag: None,
        auto_approve_flag: None,
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
    AgentCatalogEntry {
        id: "kilocode",
        name: "Kilocode",
        cli: "kilocode",
        model_flag: None,
        auto_approve_flag: Some("--auto"),
        initial_prompt_flag: None,
        use_pty: false,
        models: &[],
    },
];
