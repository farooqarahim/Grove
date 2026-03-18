use std::collections::HashMap;

use crate::errors::GroveResult;

use super::{
    AgentModelsConfig, BudgetsConfig, CheckpointConfig, ClaudeCodeConfig, CodingAgentConfig,
    ConflictStrategy, DbConfig, GroveConfig, HooksConfig, LinterConfig, LlmProviderConfig,
    MergeConfig, MergeStrategy, MergeTarget, MockProviderConfig, NetworkConfig,
    ObservabilityConfig, OrchestrationConfig, PermissionMode, ProjectConfig, ProvidersConfig,
    PublishConfig, PublishPrMode, PublishTarget, RuntimeConfig, SparseConfig, SyncBeforeRun,
    TrackerConfig, WatchdogConfig, WorktreeConfig,
};

/// Return a fully populated `GroveConfig` with sensible defaults.
/// This is the canonical source of truth for default values — it does not
/// read from disk.
pub fn default_config() -> GroveConfig {
    GroveConfig {
        project: ProjectConfig {
            name: "my-local-project".to_string(),
            default_branch: "main".to_string(),
        },
        runtime: RuntimeConfig {
            max_agents: 3,
            max_run_minutes: 60,
            max_concurrent_runs: 4,
            log_level: "info".to_string(),
            lock_wait_timeout_secs: 5,
        },
        providers: ProvidersConfig {
            default: "claude_code".to_string(),
            mock: MockProviderConfig { enabled: true },
            claude_code: ClaudeCodeConfig {
                enabled: true,
                command: "claude".to_string(),
                timeout_seconds: 28800,
                long_lived_run_host: false,
                permission_mode: PermissionMode::SkipAll,
                allowed_tools: vec![],
                gatekeeper_model: None,
                max_output_bytes: 10 * 1024 * 1024,
                max_file_size_mb: None,
                max_open_files: None,
            },
            llm: LlmProviderConfig::default(),
            coding_agents: default_coding_agents(),
        },
        budgets: BudgetsConfig {
            default_run_usd: 5.0,
            warning_threshold_percent: 80,
            hard_stop_percent: 100,
        },
        orchestration: OrchestrationConfig {
            enforce_design_first: true,
            enable_retries: true,
            max_retries_per_session: 2,
            enable_run_mcp: true,
            max_spawn_depth: 3,
        },
        worktree: default_worktree_config(),
        publish: PublishConfig {
            enabled: true,
            target: PublishTarget::Github,
            remote: "origin".to_string(),
            auto_on_success: true,
            pr_mode: PublishPrMode::Conversation,
            retry_on_startup: true,
            comment_on_issue: true,
            comment_on_pr: true,
        },
        merge: MergeConfig {
            target: MergeTarget::Direct,
            strategy: MergeStrategy::LastWriterWins,
            priorities: std::collections::HashMap::new(),
            conflict_strategy: ConflictStrategy::Markers,
            conflict_timeout_secs: 300,
            binary_strategy: super::BinaryStrategy::LastWriter,
            lockfile_strategy: super::LockfileStrategy::Regenerate,
            lockfile_commands: HashMap::new(),
        },
        checkpoint: CheckpointConfig {
            enabled: true,
            save_on_stage_transition: true,
        },
        observability: ObservabilityConfig {
            emit_json_logs: true,
            redact_secrets: true,
        },
        network: NetworkConfig {
            allow_provider_network: false,
        },
        watchdog: WatchdogConfig::default(),
        hooks: HooksConfig::default(),
        sparse: SparseConfig::default(),
        tracker: TrackerConfig::default(),
        linter: LinterConfig::default(),
        discipline: crate::orchestrator::scope::DisciplineConfig::default(),
        webhook: super::WebhookConfig::default(),
        notifications: super::NotificationsConfig::default(),
        agents: Default::default(),
        token_filter: super::TokenFilterConfig::default(),
        retry: super::RetryConfig::default(),
        db: DbConfig::default(),
        agent_models: AgentModelsConfig {
            models: {
                let mut m = HashMap::new();
                m.insert("build_prd".to_string(), "claude-sonnet-4-6".to_string());
                m.insert(
                    "plan_system_design".to_string(),
                    "claude-sonnet-4-6".to_string(),
                );
                m.insert("builder".to_string(), "claude-sonnet-4-6".to_string());
                m.insert("reviewer".to_string(), "claude-sonnet-4-6".to_string());
                m.insert("judge".to_string(), "claude-sonnet-4-6".to_string());
                m.insert("default".to_string(), "claude-sonnet-4-6".to_string());
                m
            },
        },
    }
}

/// Default `WorktreeConfig` used both by `default_config()` and as the
/// `#[serde(default)]` provider for `GroveConfig.worktree`.
pub fn default_worktree_config() -> WorktreeConfig {
    WorktreeConfig {
        root: ".grove/worktrees".to_string(),
        fetch_before_run: true,
        sync_before_run: SyncBeforeRun::Merge,
        copy_ignored: default_copy_ignored(),
        branch_prefix: default_branch_prefix(),
        cleanup_remote_branches: false,
        min_disk_bytes: default_min_disk_bytes(),
        pull_before_publish: default_pull_before_publish(),
        pull_before_publish_timeout_secs: default_pull_before_publish_timeout_secs(),
    }
}

/// Maximum GROVE_SPAWN.json nesting depth (default: 3 waves).
pub fn default_max_spawn_depth() -> u8 {
    3
}

/// Default value for `worktree.fetch_before_run` (used by `#[serde(default)]`).
pub fn default_fetch_before_run() -> bool {
    true
}

/// Default value for `worktree.sync_before_run` (used by `#[serde(default)]`).
pub fn default_sync_before_run() -> SyncBeforeRun {
    SyncBeforeRun::Merge
}

/// Default value for `worktree.copy_ignored` (used by `#[serde(default)]`).
pub fn default_copy_ignored() -> Vec<String> {
    vec![
        ".env".to_string(),
        ".env.*".to_string(),
        ".env.*.local".to_string(),
        ".envrc".to_string(),
        "docker-compose.override.yml".to_string(),
    ]
}

/// Default value for `worktree.branch_prefix` (used by `#[serde(default)]`).
pub fn default_branch_prefix() -> String {
    "grove".to_string()
}

/// Default minimum free disk space (1 GiB) before Grove refuses to create
/// a new worktree. Used by `#[serde(default)]` on `WorktreeConfig.min_disk_bytes`.
pub fn default_min_disk_bytes() -> u64 {
    1_073_741_824 // 1 GiB
}

/// Default value for `worktree.pull_before_publish` (used by `#[serde(default)]`).
pub fn default_pull_before_publish() -> bool {
    true
}

/// Default value for `worktree.pull_before_publish_timeout_secs` (used by `#[serde(default)]`).
pub fn default_pull_before_publish_timeout_secs() -> u64 {
    120
}

/// Default coding-agent CLI provider configurations for the 12 supported agents.
pub fn default_coding_agents() -> HashMap<String, CodingAgentConfig> {
    let max_output = 10 * 1024 * 1024usize;
    let mut m = HashMap::new();

    #[allow(clippy::too_many_arguments)]
    let mut add = |id: &str,
                   command: &str,
                   auto_approve: Option<&str>,
                   prompt_flag: Option<&str>,
                   keystroke: bool,
                   use_pty: bool,
                   extra: Vec<&str>,
                   model_flag: Option<&str>| {
        m.insert(
            id.to_string(),
            CodingAgentConfig {
                enabled: true,
                command: command.to_string(),
                timeout_seconds: 28800,
                auto_approve_flag: auto_approve.map(str::to_string),
                initial_prompt_flag: prompt_flag.map(str::to_string),
                use_keystroke_injection: keystroke,
                use_pty,
                default_args: extra.into_iter().map(str::to_string).collect(),
                model_flag: model_flag.map(str::to_string),
                max_output_bytes: max_output,
                max_file_size_mb: None,
                max_open_files: None,
            },
        );
    };

    //               id            cmd              auto_approve              prompt_flag        keystroke  use_pty  extra                     model_flag
    // codex uses `codex exec` subcommand (Pipe mode) — no PTY needed.
    add(
        "codex",
        "codex",
        Some("--full-auto"),
        None,
        false,
        false,
        vec![],
        Some("--model"),
    );
    add(
        "gemini",
        "gemini",
        Some("--yolo"),
        Some("-i"),
        false,
        false,
        vec![],
        Some("--model"),
    );
    add(
        "aider",
        "aider",
        Some("--yes"),
        Some("--message"),
        false,
        false,
        vec![],
        Some("--model"),
    );
    add(
        "cursor",
        "cursor-agent",
        Some("-f"),
        None,
        false,
        false,
        vec![],
        None,
    );
    add(
        "copilot",
        "copilot",
        Some("--allow-all-tools"),
        None,
        false,
        false,
        vec![],
        None,
    );
    add(
        "qwen_code",
        "qwen",
        Some("--yolo"),
        Some("-i"),
        false,
        false,
        vec![],
        Some("--model"),
    );
    add(
        "opencode",
        "opencode",
        None,
        None,
        true,
        false,
        vec![],
        None,
    );
    add(
        "kimi",
        "kimi",
        Some("--yolo"),
        Some("-c"),
        false,
        false,
        vec![],
        Some("--model"),
    );
    add("amp", "amp", None, None, false, false, vec![], None);
    add("goose", "goose", None, None, false, false, vec![], None);
    add(
        "cline",
        "cline",
        Some("--yolo"),
        None,
        false,
        false,
        vec![],
        None,
    );
    add(
        "continue",
        "cn",
        None,
        Some("-p"),
        false,
        false,
        vec![],
        None,
    );
    add(
        "kiro",
        "kiro-cli",
        None,
        None,
        false,
        false,
        vec!["chat"],
        None,
    );
    add(
        "auggie",
        "auggie",
        None,
        None,
        false,
        false,
        vec!["--allow-indexing"],
        None,
    );
    add(
        "kilocode",
        "kilocode",
        Some("--auto"),
        None,
        false,
        false,
        vec![],
        None,
    );

    m
}

/// Parse the given YAML string into a `GroveConfig`. Falls back to `default_config()`
/// for any fields not present in the YAML.
pub fn from_yaml(yaml: &str) -> GroveResult<GroveConfig> {
    let cfg: GroveConfig = serde_yaml::from_str(yaml)?;
    Ok(cfg)
}
