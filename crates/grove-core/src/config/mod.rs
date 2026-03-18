use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod agent_config;
pub mod defaults;
pub mod loader;
pub mod paths;
pub mod validator;

pub const DEFAULT_CONFIG_YAML: &str = r#"project:
  name: "my-local-project"
  default_branch: "main"

runtime:
  max_agents: 3
  max_run_minutes: 60
  max_concurrent_runs: 4
  log_level: "info"

providers:
  # "claude_code"  — run agents via the installed Claude Code CLI (default)
  # "anthropic"    — call the Anthropic Messages API directly
  # "openai"       — call the OpenAI Chat Completions API directly
  # "deepseek"     — call DeepSeek's OpenAI-compatible API directly
  # "inception"    — call Inception Labs (Mercury) API directly
  default: "claude_code"
  mock:
    enabled: true
  claude_code:
    enabled: true
    command: "claude"
    timeout_seconds: 300
    permission_mode: "skip_all"
    allowed_tools: []
    max_output_bytes: 10485760
  # llm: model override for direct LLM API providers (leave unset to use provider default)
  # llm:
  #   model: "claude-sonnet-4-6"

  # Third-party coding-agent CLI providers.
  # Set `default: "<id>"` to route all agent runs through one of these.
  coding_agents:
    codex:
      enabled: true
      command: "codex"
      timeout_seconds: 300
      auto_approve_flag: "--full-auto"
      model_flag: "--model"
    gemini:
      enabled: true
      command: "gemini"
      timeout_seconds: 300
      auto_approve_flag: "--yolo"
      initial_prompt_flag: "-i"
      model_flag: "--model"
    aider:
      enabled: true
      command: "aider"
      timeout_seconds: 300
      auto_approve_flag: "--yes"
      initial_prompt_flag: "--message"
      model_flag: "--model"
    cursor:
      enabled: true
      command: "cursor-agent"
      timeout_seconds: 300
      auto_approve_flag: "-f"
    copilot:
      enabled: true
      command: "copilot"
      timeout_seconds: 300
      auto_approve_flag: "--allow-all-tools"
    qwen_code:
      enabled: true
      command: "qwen"
      timeout_seconds: 300
      auto_approve_flag: "--yolo"
      initial_prompt_flag: "-i"
      model_flag: "--model"
    opencode:
      enabled: true
      command: "opencode"
      timeout_seconds: 300
      use_keystroke_injection: true
    kimi:
      enabled: true
      command: "kimi"
      timeout_seconds: 300
      auto_approve_flag: "--yolo"
      initial_prompt_flag: "-c"
      model_flag: "--model"
    amp:
      enabled: true
      command: "amp"
      timeout_seconds: 300
    goose:
      enabled: true
      command: "goose"
      timeout_seconds: 300
    cline:
      enabled: true
      command: "cline"
      timeout_seconds: 300
      auto_approve_flag: "--yolo"
    continue:
      enabled: true
      command: "cn"
      timeout_seconds: 300
      initial_prompt_flag: "-p"
    kiro:
      enabled: true
      command: "kiro-cli"
      timeout_seconds: 300
      default_args: ["chat"]
    auggie:
      enabled: true
      command: "auggie"
      timeout_seconds: 300
      default_args: ["--allow-indexing"]
    kilocode:
      enabled: true
      command: "kilocode"
      timeout_seconds: 300
      auto_approve_flag: "--auto"

budgets:
  default_run_usd: 5.0
  warning_threshold_percent: 80
  hard_stop_percent: 100

orchestration:
  enforce_design_first: true
  enable_retries: true
  max_retries_per_session: 2

worktree:
  root: ".grove/worktrees"
  fetch_before_run: true
  sync_before_run: "merge"
  copy_ignored: [".env", ".env.*", ".env.*.local", ".envrc", "docker-compose.override.yml"]
  branch_prefix: "grove"
  cleanup_remote_branches: false
  pull_before_publish: true

publish:
  enabled: true
  target: "github"
  remote: "origin"
  auto_on_success: true
  pr_mode: "conversation"
  retry_on_startup: true
  comment_on_issue: true
  comment_on_pr: true

merge:
  # target: "direct"   # default — merge conversation branch into default branch locally
  # target: "github"   # push conversation branch + open PR via gh CLI
  strategy: "last_writer_wins"

checkpoint:
  enabled: true
  save_on_stage_transition: true

observability:
  emit_json_logs: true
  redact_secrets: true

network:
  allow_provider_network: false

watchdog:
  enabled: true
  boot_timeout_secs: 120
  stale_threshold_secs: 300
  zombie_threshold_secs: 600
  max_agent_lifetime_secs: 3600
  max_run_lifetime_secs: 7200
  poll_interval_secs: 30

tracker:
  mode: "disabled"
  # To enable external issue tracking (e.g. GitHub Issues):
  #   mode: "external"
  #   external:
  #     provider: "github"
  #     create: "gh issue create --title '{title}' --body '{body}' --json number,title,state,labels"
  #     show: "gh issue view {id} --json number,title,state,labels,body"
  #     list: "gh issue list --state open --json number,title,state,labels"
  #     close: "gh issue close {id}"
  #     ready: "gh issue list --label ready --json number,title,state,labels"

hooks:
  # Commands to run in project_root after every successful run.
  # Use this to reinstall dependencies added during the run.
  # Examples:
  #   post_run: ["npm install"]
  #   post_run: ["pip install -r requirements.txt"]
  #   post_run: ["cargo build"]
  post_run: []

webhook:
  enabled: false
  port: 8473
  secret: ""

notifications:
  defaults:
    on_failure: []
    on_success: []

agent_models:
  architect: "claude-opus-4-6"
  builder: "claude-sonnet-4-6"
  tester: "claude-haiku-4-5-20251001"
  documenter: "claude-haiku-4-5-20251001"
  security: "claude-opus-4-6"
  reviewer: "claude-sonnet-4-6"
  debugger: "claude-sonnet-4-6"
  refactorer: "claude-haiku-4-5-20251001"
  validator: "claude-sonnet-4-6"
  default: "claude-sonnet-4-6"

agents:
  architect:
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
  builder:
    timeout_secs: 300
    max_retries: 2
    custom_instructions: ""
  tester:
    timeout_secs: 300
    max_retries: 2
    custom_instructions: ""
  reviewer:
    enabled: true
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    on_fail: "block"
    max_retry_cycles: 1
  debugger:
    enabled: true
    timeout_secs: 300
    max_retries: 2
    custom_instructions: ""
    trigger: "on_failure"
  security:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
    on_critical: "block"
    on_high: "warn"
    auto_tools: true
  refactorer:
    timeout_secs: 600
    max_retries: 3
    custom_instructions: ""
    verify_after_each_change: true
  documenter:
    enabled: false
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    update_readme: true
    update_changelog: true
    update_inline_comments: true
  validator:
    enabled: true
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    on_partial: "warn"
    on_failed: "fail"
  prd:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
  spec:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
  judge:
    enabled: false
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    on_needs_work: "warn"
  qa:
    enabled: false
    timeout_secs: 300
    max_retries: 2
    custom_instructions: ""
    on_fail: "block"
  devops:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
  optimizer:
    enabled: false
    timeout_secs: 600
    max_retries: 2
    custom_instructions: ""
  accessibility:
    enabled: false
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    on_fail: "warn"
  compliance:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
    on_non_compliant: "warn"
  dependency_manager:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
    min_fix_cvss: 7.0
  reporter:
    enabled: false
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
  migration_planner:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
  project_manager:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
"#;

/// Watchdog configuration for stale/zombie agent detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchdogConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_boot_timeout")]
    pub boot_timeout_secs: u64,
    #[serde(default = "default_stale_threshold")]
    pub stale_threshold_secs: u64,
    #[serde(default = "default_zombie_threshold")]
    pub zombie_threshold_secs: u64,
    #[serde(default = "default_max_agent_lifetime")]
    pub max_agent_lifetime_secs: u64,
    #[serde(default = "default_max_run_lifetime")]
    pub max_run_lifetime_secs: u64,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
}

fn default_boot_timeout() -> u64 {
    120
}
fn default_stale_threshold() -> u64 {
    300
}
fn default_zombie_threshold() -> u64 {
    600
}
fn default_max_agent_lifetime() -> u64 {
    3600
}
fn default_max_run_lifetime() -> u64 {
    7200
}
fn default_poll_interval() -> u64 {
    30
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            boot_timeout_secs: 120,
            stale_threshold_secs: 300,
            zombie_threshold_secs: 600,
            max_agent_lifetime_secs: 3600,
            max_run_lifetime_secs: 7200,
            poll_interval_secs: 30,
        }
    }
}

/// Issue tracker configuration for external issue integration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerConfig {
    #[serde(default)]
    pub mode: TrackerMode,
    #[serde(default)]
    pub external: ExternalTrackerConfig,
    #[serde(default)]
    pub github: GitHubTrackerConfig,
    #[serde(default)]
    pub jira: JiraTrackerConfig,
    #[serde(default)]
    pub linear: LinearTrackerConfig,
    #[serde(default)]
    pub write_back: WriteBackConfig,
    #[serde(default)]
    pub sync: SyncConfig,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            mode: TrackerMode::Disabled,
            external: ExternalTrackerConfig::default(),
            github: GitHubTrackerConfig::default(),
            jira: JiraTrackerConfig::default(),
            linear: LinearTrackerConfig::default(),
            write_back: WriteBackConfig::default(),
            sync: SyncConfig::default(),
        }
    }
}

// ── Write-back configuration ──────────────────────────────────────────────────

/// Controls whether and how Grove posts comments / transitions issues after runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteBackConfig {
    /// Master switch — all write-back is disabled when false.
    #[serde(default = "default_write_back_enabled")]
    pub enabled: bool,
    /// Post a success comment on the linked issue when a run completes.
    #[serde(default = "default_true")]
    pub comment_on_complete: bool,
    /// Post a failure comment on the linked issue when a run fails.
    #[serde(default = "default_true")]
    pub comment_on_failure: bool,
    /// Transition the issue to this status after the merge succeeds.
    /// `null` / unset = no transition.
    #[serde(default)]
    pub transition_on_merge: Option<String>,
    /// Transition the issue to this status after a run is closed/cancelled.
    #[serde(default)]
    pub transition_on_close: Option<String>,
    /// Mustache-style template rendered as the success comment body.
    #[serde(default = "default_comment_template")]
    pub comment_template: String,
    /// Template rendered as the failure comment body.
    #[serde(default = "default_failure_template")]
    pub failure_template: String,
}

fn default_write_back_enabled() -> bool {
    false
}
fn default_comment_template() -> String {
    "Grove run completed in {duration}s\nAgent cost: ${cost_usd}\nPR: {pr_url}\nRun ID: {run_id}"
        .into()
}
fn default_failure_template() -> String {
    "Grove run failed: {error}\nRun ID: {run_id}".into()
}

impl Default for WriteBackConfig {
    fn default() -> Self {
        Self {
            enabled: default_write_back_enabled(),
            comment_on_complete: true,
            comment_on_failure: true,
            transition_on_merge: None,
            transition_on_close: None,
            comment_template: default_comment_template(),
            failure_template: default_failure_template(),
        }
    }
}

// ── Sync configuration ────────────────────────────────────────────────────────

/// Controls how Grove syncs issues from external trackers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Automatically sync every N seconds in the background.
    /// `null` / unset = sync only when explicitly requested (CLI or Tauri command).
    #[serde(default)]
    pub auto_sync_interval_secs: Option<u64>,
    /// When `true`, only fetch issues updated after the last sync cursor.
    /// When `false`, always do a full re-fetch (slower but catches deletions).
    #[serde(default = "default_sync_incremental")]
    pub incremental: bool,
    /// Minimum seconds between syncs for the same provider (debounce).
    #[serde(default = "default_debounce_secs")]
    pub debounce_secs: u64,
}

fn default_sync_incremental() -> bool {
    true
}
fn default_debounce_secs() -> u64 {
    30
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            auto_sync_interval_secs: None,
            incremental: true,
            debounce_secs: default_debounce_secs(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TrackerMode {
    #[default]
    Disabled,
    External,
    GitHub,
    Jira,
    Linear,
    Multi,
}

/// GitHub issue tracker configuration (uses `gh` CLI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubTrackerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_github_labels_ready")]
    pub labels_ready: Vec<String>,
}

fn default_github_labels_ready() -> Vec<String> {
    vec!["ready".to_string()]
}

impl Default for GitHubTrackerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            labels_ready: default_github_labels_ready(),
        }
    }
}

/// Jira issue tracker configuration (direct REST API).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JiraTrackerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub site_url: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub project_key: String,
    #[serde(default = "default_jira_jql_ready")]
    pub jql_ready: String,
}

fn default_jira_jql_ready() -> String {
    "status = 'Ready for Dev'".to_string()
}

impl Default for JiraTrackerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            site_url: String::new(),
            email: String::new(),
            project_key: String::new(),
            jql_ready: default_jira_jql_ready(),
        }
    }
}

/// Linear issue tracker configuration (GraphQL API).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinearTrackerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub team_key: String,
    #[serde(default = "default_linear_label_ready")]
    pub label_ready: String,
}

fn default_linear_label_ready() -> String {
    "ready".to_string()
}

impl Default for LinearTrackerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            team_key: String::new(),
            label_ready: default_linear_label_ready(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalTrackerConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default = "default_gh_create")]
    pub create: String,
    #[serde(default = "default_gh_show")]
    pub show: String,
    #[serde(default = "default_gh_list")]
    pub list: String,
    #[serde(default = "default_gh_close")]
    pub close: String,
    #[serde(default = "default_gh_ready")]
    pub ready: String,
    #[serde(default = "default_gh_search")]
    pub search_cmd: String,
}

fn default_gh_create() -> String {
    "gh issue create --title '{title}' --body '{body}' --json number,title,state,labels".into()
}
fn default_gh_show() -> String {
    "gh issue view {id} --json number,title,state,labels,body".into()
}
fn default_gh_list() -> String {
    "gh issue list --state open --json number,title,state,labels".into()
}
fn default_gh_close() -> String {
    "gh issue close {id}".into()
}
fn default_gh_ready() -> String {
    "gh issue list --label ready --json number,title,state,labels".into()
}
fn default_gh_search() -> String {
    "gh issue list --search '{query}' --limit {limit} --json number,title,state,labels,body,assignees,url".into()
}

impl Default for ExternalTrackerConfig {
    fn default() -> Self {
        Self {
            provider: "github".into(),
            create: default_gh_create(),
            show: default_gh_show(),
            list: default_gh_list(),
            close: default_gh_close(),
            ready: default_gh_ready(),
            search_cmd: default_gh_search(),
        }
    }
}

// ── Linter integration ────────────────────────────────────────────────────────

/// Configuration for running linters and creating auto-fix runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub commands: Vec<LintCommandConfig>,
    #[serde(default)]
    pub auto_fix: bool,
}

/// A single lint command entry from grove.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintCommandConfig {
    pub name: String,
    pub command: String,
    #[serde(default = "default_lint_parser")]
    pub parser: String,
}

fn default_lint_parser() -> String {
    "line".to_string()
}

/// Webhook server configuration for receiving incoming automation triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_webhook_port")]
    pub port: u16,
    #[serde(default)]
    pub secret: String,
}

fn default_webhook_port() -> u16 {
    8473
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8473,
            secret: String::new(),
        }
    }
}

/// Global notification configuration for automations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub defaults: NotificationDefaults,
}

/// Default notification targets applied when an automation doesn't specify its own.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationDefaults {
    #[serde(default)]
    pub on_failure: Vec<crate::automation::NotificationTarget>,
    #[serde(default)]
    pub on_success: Vec<crate::automation::NotificationTarget>,
}

// ── Token filter configuration ─────────────────────────────────────────────

fn default_max_tokens_per_command() -> usize {
    8_000
}
fn default_max_hunk_lines() -> usize {
    30
}
fn default_max_commits() -> usize {
    10
}
fn default_max_diff_lines() -> usize {
    500
}
fn default_max_file_lines() -> usize {
    500
}

/// Configuration for the token reduction filter pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenFilterConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_tokens_per_command")]
    pub max_tokens_per_command: usize,
    #[serde(default = "default_max_hunk_lines")]
    pub max_hunk_lines: usize,
    #[serde(default = "default_max_commits")]
    pub max_commits: usize,
    #[serde(default = "default_max_diff_lines")]
    pub max_diff_lines: usize,
    #[serde(default = "default_max_file_lines")]
    pub max_file_lines: usize,
}

impl Default for TokenFilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_tokens_per_command: default_max_tokens_per_command(),
            max_hunk_lines: default_max_hunk_lines(),
            max_commits: default_max_commits(),
            max_diff_lines: default_max_diff_lines(),
            max_file_lines: default_max_file_lines(),
        }
    }
}

/// Shell commands run in `project_root` after every successful grove run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HooksConfig {
    /// Each entry is a shell command string, e.g. `"npm install"`.
    #[serde(default)]
    pub post_run: Vec<String>,
    /// Per-event lifecycle hooks.
    #[serde(default)]
    pub on: HashMap<HookEvent, Vec<HookDefinition>>,
    /// Per-agent-type capability guards (file path and tool restrictions).
    #[serde(default)]
    pub guards: HashMap<String, CapabilityGuard>,
}

/// Lifecycle events that hooks can listen to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    PostToolUse,
    Stop,
    PreCompact,
    PostRun,
    /// Fired after a git merge succeeds but before the merge commit is written.
    /// A non-zero exit from a blocking hook aborts the merge commit.
    PreMerge,
}

/// A single hook definition attached to a lifecycle event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    pub command: String,
    #[serde(default)]
    pub blocking: bool,
    #[serde(default = "default_hook_timeout")]
    pub timeout_secs: u64,
}

fn default_hook_timeout() -> u64 {
    30
}

/// Guards that restrict what files and tools an agent type can access.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilityGuard {
    #[serde(default)]
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub blocked_paths: Vec<String>,
    #[serde(default)]
    pub blocked_tools: Vec<String>,
}

fn default_true() -> bool {
    true
}
fn default_on_fail_block() -> String {
    "block".to_string()
}
fn default_on_fail_warn() -> String {
    "warn".to_string()
}
fn default_on_fail_fail() -> String {
    "fail".to_string()
}
fn default_trigger_on_failure() -> String {
    "on_failure".to_string()
}
fn default_timeout_300() -> u64 {
    300
}
fn default_timeout_600() -> u64 {
    600
}
fn default_max_retries_1() -> u8 {
    1
}
fn default_max_retries_2() -> u8 {
    2
}
fn default_max_retries_3() -> u8 {
    3
}
fn default_max_retry_cycles_1() -> u8 {
    1
}

/// Configuration for the Architect agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchitectConfig {
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for ArchitectConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Builder agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderConfig {
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_2")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 300,
            max_retries: 2,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Tester agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TesterConfig {
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_2")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for TesterConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 300,
            max_retries: 2,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Reviewer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "block" | "retry" | "warn"
    #[serde(default = "default_on_fail_block")]
    pub on_fail: String,
    #[serde(default = "default_max_retry_cycles_1")]
    pub max_retry_cycles: u8,
}

impl Default for ReviewerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
            on_fail: "block".to_string(),
            max_retry_cycles: 1,
        }
    }
}

/// Configuration for the Debugger agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuggerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_2")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "on_failure" | "on_test_failure" | "always" | "never"
    #[serde(default = "default_trigger_on_failure")]
    pub trigger: String,
}

impl Default for DebuggerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: 300,
            max_retries: 2,
            custom_instructions: String::new(),
            trigger: "on_failure".to_string(),
        }
    }
}

/// Configuration for the Security agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "block" | "warn"
    #[serde(default = "default_on_fail_block")]
    pub on_critical: String,
    /// "block" | "warn"
    #[serde(default = "default_on_fail_warn")]
    pub on_high: String,
    #[serde(default = "default_true")]
    pub auto_tools: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
            on_critical: "block".to_string(),
            on_high: "warn".to_string(),
            auto_tools: true,
        }
    }
}

/// Configuration for the Refactorer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorerConfig {
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_3")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    #[serde(default = "default_true")]
    pub verify_after_each_change: bool,
}

impl Default for RefactorerConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 600,
            max_retries: 3,
            custom_instructions: String::new(),
            verify_after_each_change: true,
        }
    }
}

/// Configuration for the Documenter agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumenterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    #[serde(default = "default_true")]
    pub update_readme: bool,
    #[serde(default = "default_true")]
    pub update_changelog: bool,
    #[serde(default = "default_true")]
    pub update_inline_comments: bool,
}

impl Default for DocumenterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
            update_readme: true,
            update_changelog: true,
            update_inline_comments: true,
        }
    }
}

/// Configuration for the Validator agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "warn" | "fail"
    #[serde(default = "default_on_fail_warn")]
    pub on_partial: String,
    /// "warn" | "fail"
    #[serde(default = "default_on_fail_fail")]
    pub on_failed: String,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
            on_partial: "warn".to_string(),
            on_failed: "fail".to_string(),
        }
    }
}

/// Configuration for the PRD agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrdConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for PrdConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Spec agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for SpecConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Judge agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "block" | "warn"
    #[serde(default = "default_on_fail_block")]
    pub on_needs_work: String,
}

impl Default for JudgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
            on_needs_work: "warn".to_string(),
        }
    }
}

/// Configuration for the QA agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_2")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "block" | "warn"
    #[serde(default = "default_on_fail_block")]
    pub on_fail: String,
}

impl Default for QaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 300,
            max_retries: 2,
            custom_instructions: String::new(),
            on_fail: "block".to_string(),
        }
    }
}

/// Configuration for the DevOps agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevOpsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for DevOpsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Optimizer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_2")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 2,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Accessibility agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "block" | "warn"
    #[serde(default = "default_on_fail_warn")]
    pub on_fail: String,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
            on_fail: "warn".to_string(),
        }
    }
}

/// Configuration for the Compliance agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// "block" | "warn"
    #[serde(default = "default_on_fail_block")]
    pub on_non_compliant: String,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
            on_non_compliant: "warn".to_string(),
        }
    }
}

/// Configuration for the DependencyManager agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyManagerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
    /// Minimum CVSS score to require immediate fix (0.0–10.0).
    #[serde(default = "default_cvss_threshold")]
    pub min_fix_cvss: f32,
}

fn default_cvss_threshold() -> f32 {
    7.0
}

impl Default for DependencyManagerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
            min_fix_cvss: 7.0,
        }
    }
}

/// Configuration for the Reporter agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReporterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for ReporterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the MigrationPlanner agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlannerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for MigrationPlannerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the ProjectManager agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManagerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for ProjectManagerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Researcher agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearcherConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for ResearcherConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the DataMigrator agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMigratorConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for DataMigratorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the ApiDesigner agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiDesignerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for ApiDesignerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Deployer agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployerConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_2")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for DeployerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 2,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Monitor agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_300")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 300,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Integrator agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegratorConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_2")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for IntegratorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 2,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Performance agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Configuration for the Coordinator agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_timeout_600")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_retries_1")]
    pub max_retries: u8,
    #[serde(default)]
    pub custom_instructions: String,
}

impl Default for CoordinatorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            timeout_secs: 600,
            max_retries: 1,
            custom_instructions: String::new(),
        }
    }
}

/// Retry policy configuration for transient failure recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_base_delay_ms")]
    pub base_delay_ms: u64,
    #[serde(default = "default_retry_max_delay_ms")]
    pub max_delay_ms: u64,
}

fn default_max_retries() -> u32 {
    3
}
fn default_retry_base_delay_ms() -> u64 {
    1000
}
fn default_retry_max_delay_ms() -> u64 {
    30000
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            base_delay_ms: default_retry_base_delay_ms(),
            max_delay_ms: default_retry_max_delay_ms(),
        }
    }
}

/// Top-level per-agent configuration container.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    #[serde(default)]
    pub architect: ArchitectConfig,
    #[serde(default)]
    pub builder: BuilderConfig,
    #[serde(default)]
    pub tester: TesterConfig,
    #[serde(default)]
    pub reviewer: ReviewerConfig,
    #[serde(default)]
    pub debugger: DebuggerConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub refactorer: RefactorerConfig,
    #[serde(default)]
    pub documenter: DocumenterConfig,
    #[serde(default)]
    pub validator: ValidatorConfig,
    #[serde(default)]
    pub prd: PrdConfig,
    #[serde(default)]
    pub spec: SpecConfig,
    #[serde(default)]
    pub judge: JudgeConfig,
    #[serde(default)]
    pub qa: QaConfig,
    #[serde(default)]
    pub devops: DevOpsConfig,
    #[serde(default)]
    pub optimizer: OptimizerConfig,
    #[serde(default)]
    pub accessibility: AccessibilityConfig,
    #[serde(default)]
    pub compliance: ComplianceConfig,
    #[serde(default)]
    pub dependency_manager: DependencyManagerConfig,
    #[serde(default)]
    pub reporter: ReporterConfig,
    #[serde(default)]
    pub migration_planner: MigrationPlannerConfig,
    #[serde(default)]
    pub project_manager: ProjectManagerConfig,
    #[serde(default)]
    pub researcher: ResearcherConfig,
    #[serde(default)]
    pub data_migrator: DataMigratorConfig,
    #[serde(default)]
    pub api_designer: ApiDesignerConfig,
    #[serde(default)]
    pub deployer: DeployerConfig,
    #[serde(default)]
    pub monitor: MonitorConfig,
    #[serde(default)]
    pub integrator: IntegratorConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub coordinator: CoordinatorConfig,
}

/// Per-agent Claude model overrides.
///
/// Keys are agent type names (e.g. "architect", "builder") or "default".
/// Resolution priority: agent-specific key → "default" key → None.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentModelsConfig {
    #[serde(flatten)]
    pub models: HashMap<String, String>,
}

impl AgentModelsConfig {
    /// Resolve the model for `agent_type`.
    ///
    /// Priority: agent-specific key → "default" key → None.
    pub fn resolve(&self, agent_type: &str) -> Option<&str> {
        self.models
            .get(agent_type)
            .or_else(|| self.models.get("default"))
            .map(String::as_str)
    }
}

// ── Database pool configuration ───────────────────────────────────────────────

fn default_pool_size() -> u32 {
    8
}

fn default_connection_timeout_ms() -> u64 {
    10_000
}

/// Configuration for the SQLite connection pool.
///
/// All fields have sensible defaults so that existing `grove.yaml` files
/// without a `db:` section continue to work unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbConfig {
    /// Maximum number of connections kept open in the pool.
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    /// How long (in milliseconds) to wait for a connection before
    /// returning a pool-exhaustion error.
    #[serde(default = "default_connection_timeout_ms")]
    pub connection_timeout_ms: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            pool_size: default_pool_size(),
            connection_timeout_ms: default_connection_timeout_ms(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroveConfig {
    pub project: ProjectConfig,
    pub runtime: RuntimeConfig,
    pub providers: ProvidersConfig,
    pub budgets: BudgetsConfig,
    pub orchestration: OrchestrationConfig,
    pub worktree: WorktreeConfig,
    #[serde(default)]
    pub publish: PublishConfig,
    pub merge: MergeConfig,
    pub checkpoint: CheckpointConfig,
    pub observability: ObservabilityConfig,
    pub network: NetworkConfig,
    #[serde(default)]
    pub watchdog: WatchdogConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
    #[serde(default)]
    pub agent_models: AgentModelsConfig,
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub sparse: SparseConfig,
    #[serde(default)]
    pub tracker: TrackerConfig,
    #[serde(default)]
    pub linter: LinterConfig,
    #[serde(default)]
    pub discipline: crate::orchestrator::scope::DisciplineConfig,
    #[serde(default)]
    pub webhook: WebhookConfig,
    #[serde(default)]
    pub notifications: NotificationsConfig,
    #[serde(default)]
    pub token_filter: TokenFilterConfig,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub db: DbConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PublishTarget {
    #[default]
    Github,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PublishPrMode {
    #[default]
    Conversation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub target: PublishTarget,
    #[serde(default = "default_publish_remote")]
    pub remote: String,
    #[serde(default = "default_true")]
    pub auto_on_success: bool,
    #[serde(default)]
    pub pr_mode: PublishPrMode,
    #[serde(default = "default_true")]
    pub retry_on_startup: bool,
    #[serde(default = "default_true")]
    pub comment_on_issue: bool,
    #[serde(default = "default_true")]
    pub comment_on_pr: bool,
}

fn default_publish_remote() -> String {
    "origin".to_string()
}

impl Default for PublishConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            target: PublishTarget::Github,
            remote: default_publish_remote(),
            auto_on_success: true,
            pr_mode: PublishPrMode::Conversation,
            retry_on_startup: true,
            comment_on_issue: true,
            comment_on_pr: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_agents: u16,
    pub max_run_minutes: u32,
    /// Maximum number of conversations that can have an active (executing) run
    /// at the same time. Each conversation is limited to 1 active run.
    /// Default: 4, valid range: 1–10.
    #[serde(default = "default_max_concurrent_runs")]
    pub max_concurrent_runs: u16,
    pub log_level: String,
    /// How long to wait for a run slot or concurrency cap to free up before
    /// giving up, in seconds.
    ///
    /// `0` means fail immediately with an error message.
    /// When > 0, Grove polls the slot with exponential backoff (1s → 2s → 4s …
    /// capped at 30s) and logs "Waiting for run slot…" progress messages until
    /// either the slot is acquired or this timeout is reached.
    /// Default: 30s.
    #[serde(default = "default_lock_wait_timeout_secs")]
    pub lock_wait_timeout_secs: u64,
}

fn default_max_concurrent_runs() -> u16 {
    4
}
fn default_lock_wait_timeout_secs() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    pub default: String,
    pub mock: MockProviderConfig,
    pub claude_code: ClaudeCodeConfig,
    /// Direct LLM API provider settings (Anthropic, OpenAI, DeepSeek, Inception).
    /// Only used when `default` is set to a provider id other than `claude_code`.
    #[serde(default)]
    pub llm: LlmProviderConfig,
    /// Per-agent CLI provider settings for third-party coding agents (Codex, Gemini,
    /// Cursor, Copilot, Qwen, OpenCode, Kimi, Cline, Continue, Kiro, Auggie, Kilocode).
    /// Each key is the provider id used in `default:` (e.g. `"codex"`, `"gemini"`).
    #[serde(default)]
    pub coding_agents: HashMap<String, CodingAgentConfig>,
}

/// Settings for direct LLM API providers (non-claude_code).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmProviderConfig {
    /// Model override for the selected provider.
    /// When `None`, each provider uses its own built-in default.
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockProviderConfig {
    pub enabled: bool,
}

/// Controls how Grove handles Claude Code tool-permission requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Pass `--dangerously-skip-permissions` — all tools auto-approved (default).
    #[default]
    SkipAll,
    /// Pause on each permission request and ask the human via TTY.
    HumanGate,
    /// Spawn a lightweight gatekeeper Claude instance to decide autonomously.
    AutonomousGate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeCodeConfig {
    pub enabled: bool,
    pub command: String,
    pub timeout_seconds: u64,
    /// Prefer a single long-lived Claude host for classic `run` execution.
    /// When disabled or when host startup fails, Grove falls back to one-shot
    /// phase execution.
    #[serde(default)]
    pub long_lived_run_host: bool,
    /// How tool-permission requests are handled. Defaults to `SkipAll`.
    #[serde(default)]
    pub permission_mode: PermissionMode,
    /// Seed set of allowed tools for `HumanGate` / `AutonomousGate` modes.
    /// Empty list = Claude starts with no pre-approved tools (every tool needs a gate).
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Model used by the gatekeeper in `AutonomousGate` mode.
    /// Defaults to `claude-haiku-4-5-20251001` if not set.
    #[serde(default)]
    pub gatekeeper_model: Option<String>,
    /// Maximum bytes of stdout that will be collected from one agent invocation.
    /// Defaults to 10 MiB (10_485_760). Exceeding this limit kills the child
    /// process and returns a `GroveError::Runtime`.
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    /// Maximum file size (in MiB) that the agent process may write via a single
    /// `write(2)` call.  Applied as `RLIMIT_FSIZE` on Unix.  `None` = no limit.
    #[serde(default)]
    pub max_file_size_mb: Option<u32>,
    /// Maximum number of open file descriptors for the agent process.
    /// Applied as `RLIMIT_NOFILE` on Unix.  `None` = no limit.
    #[serde(default)]
    pub max_open_files: Option<u32>,
}

fn default_max_output_bytes() -> usize {
    10 * 1024 * 1024 // 10 MiB
}

fn default_coding_agent_timeout() -> u64 {
    300
}

/// Configuration for a generic coding-agent CLI provider (Codex, Gemini, Cursor, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodingAgentConfig {
    /// When `false` this agent is skipped during registry construction.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// CLI binary name or absolute path (e.g. `"codex"`, `"gemini"`, `"cursor-agent"`).
    pub command: String,
    /// Per-invocation wall-clock timeout in seconds.
    #[serde(default = "default_coding_agent_timeout")]
    pub timeout_seconds: u64,
    /// Flag to grant unrestricted tool use (e.g. `"--yolo"`, `"--full-auto"`).
    /// `None` = no auto-approve flag is passed.
    #[serde(default)]
    pub auto_approve_flag: Option<String>,
    /// CLI flag that precedes the initial prompt (e.g. `"-i"`, `"-c"`, `"-p"`).
    /// `None` = prompt is passed as a positional argument (last arg).
    #[serde(default)]
    pub initial_prompt_flag: Option<String>,
    /// When `true`, the prompt is written to the process's stdin after startup
    /// instead of being passed as a CLI argument (for TUI agents with no prompt flag).
    #[serde(default)]
    pub use_keystroke_injection: bool,
    /// When `true`, the agent process is spawned inside a pseudo-terminal (PTY) so
    /// that `isatty(stdout)` returns `true`. Required for agents like codex that
    /// refuse to run when stdout is not a TTY.
    #[serde(default)]
    pub use_pty: bool,
    /// Additional arguments prepended before the auto-approve flag.
    /// Example: `["chat"]` for Kiro, `["--allow-indexing"]` for Auggie.
    #[serde(default)]
    pub default_args: Vec<String>,
    /// CLI flag used to pass a model override (e.g. `"--model"`).
    /// `None` = this agent does not support per-invocation model selection.
    #[serde(default)]
    pub model_flag: Option<String>,
    /// Maximum bytes of stdout collected per invocation. Defaults to 10 MiB.
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
    /// Maximum file size (in MiB) the agent process may write via a single
    /// `write(2)` call.  Applied as `RLIMIT_FSIZE` on Unix.  `None` = no limit.
    #[serde(default)]
    pub max_file_size_mb: Option<u32>,
    /// Maximum number of open file descriptors for the agent process.
    /// Applied as `RLIMIT_NOFILE` on Unix.  `None` = no limit.
    #[serde(default)]
    pub max_open_files: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetsConfig {
    pub default_run_usd: f64,
    pub warning_threshold_percent: u8,
    pub hard_stop_percent: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub enforce_design_first: bool,
    pub enable_retries: bool,
    pub max_retries_per_session: u8,
    /// Inject a dedicated Run MCP server into classic `run` execution.
    /// Hive/graph execution continues to use its separate MCP surface.
    #[serde(default = "default_true")]
    pub enable_run_mcp: bool,
    /// Maximum number of GROVE_SPAWN.json waves allowed per run.
    /// Prevents unbounded recursive agent spawning. Default: 3.
    #[serde(default = "defaults::default_max_spawn_depth")]
    pub max_spawn_depth: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default = "defaults::default_worktree_config")]
pub struct WorktreeConfig {
    pub root: String,
    /// Fetch from upstream before creating run worktrees so agents always
    /// work on the latest code. On fetch failure, logs a warning and
    /// proceeds — never blocks a run on network failure. Default: true.
    #[serde(default = "defaults::default_fetch_before_run")]
    pub fetch_before_run: bool,
    /// How to sync stale conversation branches with the default branch before
    /// a run starts. Default: `Merge` (merge main into conv branch, auto-resolve
    /// conflicts). Accepts `"merge"`, `"rebase"`, `"none"`, or legacy booleans
    /// (`true` → Rebase, `false` → None) for backward compatibility.
    #[serde(
        default = "defaults::default_sync_before_run",
        deserialize_with = "deserialize_sync_before_run"
    )]
    pub sync_before_run: SyncBeforeRun,
    /// Glob patterns for gitignored/untracked files to copy into new
    /// worktrees. Defaults to common env-file patterns. Set to `[]` to
    /// disable entirely.
    #[serde(default = "defaults::default_copy_ignored")]
    pub copy_ignored: Vec<String>,
    /// Git branch name prefix for all Grove-managed branches.
    /// Default: `"grove"` → branches named `grove/s_<id>`.
    /// Must be a valid git ref component (validated on startup).
    #[serde(default = "defaults::default_branch_prefix")]
    pub branch_prefix: String,
    /// Delete the remote tracking branch when a worktree's local branch is
    /// removed. Default: false (Grove branches are local-only artifacts).
    #[serde(default)]
    pub cleanup_remote_branches: bool,
    /// Minimum free disk space required (in bytes) before Grove creates a new
    /// worktree. Grove refuses to create worktrees when available space on the
    /// target filesystem drops below this threshold.
    /// Default: 1 073 741 824 (1 GiB). Set to 0 to disable the check.
    #[serde(default = "defaults::default_min_disk_bytes")]
    pub min_disk_bytes: u64,
    /// Fetch + merge the remote conversation branch before publishing,
    /// ensuring the push is always fast-forward. If the remote has
    /// diverged, the conflict resolution agent resolves automatically.
    /// Default: true.
    #[serde(default = "defaults::default_pull_before_publish")]
    pub pull_before_publish: bool,
    /// Timeout in seconds for the conflict resolution agent during
    /// pre-publish pull. Default: 120.
    #[serde(default = "defaults::default_pull_before_publish_timeout_secs")]
    pub pull_before_publish_timeout_secs: u64,
}

/// How Grove syncs a stale conversation branch with the project's default
/// branch before each run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncBeforeRun {
    /// Merge `origin/main` INTO the conversation branch. If the merge
    /// conflicts, a conflict-resolution agent resolves automatically.
    Merge,
    /// Legacy: rebase the conversation branch onto `main`. Fails immediately
    /// on conflict.
    Rebase,
    /// Skip sync entirely — the agent runs on whatever the branch has.
    None,
}

/// Custom deserializer that accepts both the new enum strings (`"merge"`,
/// `"rebase"`, `"none"`) and legacy booleans (`true` → Rebase, `false` → None)
/// for backward compatibility with `rebase_before_run: true/false`.
fn deserialize_sync_before_run<'de, D>(deserializer: D) -> Result<SyncBeforeRun, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct SyncVisitor;

    impl<'de> de::Visitor<'de> for SyncVisitor {
        type Value = SyncBeforeRun;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str(r#""merge", "rebase", "none", true, or false"#)
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<SyncBeforeRun, E> {
            Ok(if v {
                SyncBeforeRun::Rebase
            } else {
                SyncBeforeRun::None
            })
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<SyncBeforeRun, E> {
            match v {
                "merge" => Ok(SyncBeforeRun::Merge),
                "rebase" => Ok(SyncBeforeRun::Rebase),
                "none" => Ok(SyncBeforeRun::None),
                other => Err(de::Error::unknown_variant(
                    other,
                    &["merge", "rebase", "none"],
                )),
            }
        }
    }

    deserializer.deserialize_any(SyncVisitor)
}

/// Controls how Grove's FS-fallback merge resolves same-file conflicts.
///
/// The git merge path (`git merge --no-ff`) always uses git's own 3-way merge
/// and is unaffected by this setting.  This enum controls only the hash-based
/// `merge_worktrees()` path used when git worktrees are unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Legacy file-level last-writer-wins (current behaviour).
    #[default]
    LastWriterWins,
    /// 3-way line-level merge via `git merge-file`.
    ThreeWay,
    /// Ask Claude to resolve conflicts that `git merge-file` cannot auto-resolve.
    ///
    /// Requires a configured Anthropic API key (env `ANTHROPIC_API_KEY` or
    /// `grove auth set anthropic <key>`). Falls back to `ThreeWay` when the key
    /// is unavailable, and falls back to `LastWriterWins` when git is also absent.
    /// Binary files always use `LastWriterWins` regardless of this setting.
    AiResolve,
}

/// How conversation branches are merged into the project's target branch.
///
/// | Strategy | Behaviour |
/// |----------|-----------|
/// | `direct` | `git merge --no-ff` into the default branch locally. |
/// | `github` | Push the conversation branch, open a PR via `gh`. |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MergeTarget {
    /// Merge conversation branch directly into the default branch locally.
    #[default]
    Direct,
    /// Push the conversation branch to `origin` and open a GitHub PR.
    /// Requires `gh` to be installed and authenticated. No local merge occurs.
    Github,
}

/// Controls what happens when the merge layer detects a same-file conflict.
///
/// Applied by the engine after `merge_worktrees()` returns, keeping the merge
/// layer pure and testable without TTY mocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Last-writer-wins: silently overwrites (legacy behaviour, safe for CI).
    Auto,
    /// Write conflict markers into files, save artifacts, continue execution.
    #[default]
    Markers,
    /// Pause execution and prompt user to resolve (TTY only).
    /// Auto-degrades to `Fail` when stdin is not a TTY.
    Pause,
    /// Treat any unresolved conflict as a fatal error. Run fails immediately.
    Fail,
}

/// How to handle binary file conflicts during merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BinaryStrategy {
    /// Last writer wins: overwrite with the higher-priority agent's version.
    #[default]
    LastWriter,
    /// Treat binary conflict as a fatal error.
    Fail,
    /// Keep the base (common ancestor) version — discard both agents' changes.
    KeepBase,
}

/// How to handle lockfile conflicts during merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LockfileStrategy {
    /// Regenerate the lockfile after merge using the appropriate package manager.
    #[default]
    Regenerate,
    /// Last writer wins (same as binary).
    LastWriter,
    /// Treat lockfile conflict as a fatal error.
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConfig {
    /// Where conversation branches are merged: `"direct"` (default) | `"github"`.
    #[serde(default, alias = "promotion")]
    pub target: MergeTarget,
    /// FS-fallback merge strategy.  Accepts `"last_writer_wins"` or `"three_way"`.
    #[serde(default)]
    pub strategy: MergeStrategy,
    /// Optional per-agent merge priority overrides. Lower number = higher priority
    /// = applied last = wins conflicts.
    #[serde(default)]
    pub priorities: std::collections::HashMap<String, u8>,
    /// How to handle conflicts that the merge strategy cannot auto-resolve.
    #[serde(default)]
    pub conflict_strategy: ConflictStrategy,
    /// Timeout in seconds for `Pause` mode interactive prompts. 0 = no timeout.
    #[serde(default = "default_conflict_timeout")]
    pub conflict_timeout_secs: u64,
    /// How to handle binary file conflicts.
    #[serde(default)]
    pub binary_strategy: BinaryStrategy,
    /// How to handle lockfile conflicts.
    #[serde(default)]
    pub lockfile_strategy: LockfileStrategy,
    /// Custom lockfile regeneration commands. Keys are filenames (e.g. `"Cargo.lock"`),
    /// values are shell commands. Overrides built-in defaults.
    #[serde(default)]
    pub lockfile_commands: HashMap<String, String>,
}

fn default_conflict_timeout() -> u64 {
    300 // 5 minutes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    pub enabled: bool,
    pub save_on_stage_transition: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub emit_json_logs: bool,
    pub redact_secrets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub allow_provider_network: bool,
}

/// Controls git sparse checkout for agent worktrees.
///
/// When enabled, each agent's worktree only materialises the files matching
/// its profile patterns, dramatically reducing disk I/O on large repos.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SparseConfig {
    /// Whether to use sparse checkout. Default: `false` (opt-in).
    #[serde(default)]
    pub enabled: bool,
    /// Per-agent-type sparse profiles. Keys are agent type names (e.g. `"builder"`),
    /// values are lists of patterns (gitignore syntax, `--no-cone` mode).
    /// An empty list or missing key means full checkout for that agent.
    #[serde(default)]
    pub profiles: HashMap<String, Vec<String>>,
}

impl GroveConfig {
    pub fn load_or_create(project_root: &Path) -> crate::errors::GroveResult<Self> {
        loader::load_or_create(project_root)
    }

    pub fn write_default(project_root: &Path) -> crate::errors::GroveResult<PathBuf> {
        use std::fs;
        let p = paths::config_path(project_root);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&p, DEFAULT_CONFIG_YAML)?;
        Ok(p)
    }

    pub fn validate(&self) -> crate::errors::GroveResult<()> {
        validator::validate(self)
    }

    /// Serialize this config back to YAML and write it to `.grove/grove.yaml`.
    pub fn save(&self, project_root: &Path) -> crate::errors::GroveResult<()> {
        loader::save_config(project_root, self)
    }
}

// Re-export path helpers so existing callers (`use crate::config::grove_dir`) keep working.
pub use paths::{
    checkpoints_dir, config_path, db_path, grove_dir, logs_dir, reports_dir, worktrees_dir,
};
