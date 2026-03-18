pub mod abort;
pub mod abort_handle;
pub mod conversation;
pub mod engine;
pub mod handoff;
pub mod instructions;
pub mod intent;
pub mod interactive;
pub mod issue_context;
pub mod pipeline;
pub mod plan_steps_repo;
pub mod planner;
pub mod qa_source;
pub mod resume;
pub mod run_memory;
pub mod scope;
pub mod spawn;
pub mod state_enums;
pub mod state_machine;
pub mod task_decomposer;
pub mod transitions;
pub mod verdict;
pub mod workspace;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::checkpoint::{self, BudgetSnapshot, CheckpointPayload};
use crate::config::{GroveConfig, PermissionMode};
use crate::db::DbHandle;
use crate::errors::{GroveError, GroveResult};
use crate::events;
use crate::providers::Provider;
use crate::reporting;
use crate::worktree;

const MERGE_PUBLISH_COMMAND_TIMEOUT_SECS: u64 = 60;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RunState {
    Created,
    Planning,
    Executing,
    WaitingForGate,
    Verifying,
    Publishing,
    Merging,
    Completed,
    Failed,
    Paused,
}

impl RunState {
    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Created => "created",
            RunState::Planning => "planning",
            RunState::Executing => "executing",
            RunState::WaitingForGate => "waiting_for_gate",
            RunState::Verifying => "verifying",
            RunState::Publishing => "publishing",
            RunState::Merging => "merging",
            RunState::Completed => "completed",
            RunState::Failed => "failed",
            RunState::Paused => "paused",
        }
    }

    #[allow(clippy::should_implement_trait)]
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "created" => Some(RunState::Created),
            "planning" => Some(RunState::Planning),
            "executing" => Some(RunState::Executing),
            "waiting_for_gate" => Some(RunState::WaitingForGate),
            "verifying" => Some(RunState::Verifying),
            "publishing" => Some(RunState::Publishing),
            "merging" => Some(RunState::Merging),
            "completed" => Some(RunState::Completed),
            "failed" => Some(RunState::Failed),
            "paused" => Some(RunState::Paused),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub objective: String,
    pub state: String,
    pub budget_usd: f64,
    pub cost_used_usd: f64,
    pub publish_status: String,
    pub publish_error: Option<String>,
    pub final_commit_sha: Option<String>,
    pub pr_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub conversation_id: Option<String>,
    pub pipeline: Option<String>,
    pub current_agent: Option<String>,
}

/// Type aliases for the parallel-stage plan representation.
pub type AgentStage = Vec<crate::agents::AgentType>;
pub type AgentPlan = Vec<AgentStage>;

pub struct RunOptions {
    pub budget_usd: Option<f64>,
    pub max_agents: Option<u16>,
    /// Claude model to use for all agents in this run (e.g. "claude-opus-4-6").
    /// `None` means use the provider's default.
    pub model: Option<String>,
    /// Pause interactively after every agent (requires a TTY).
    pub interactive: bool,
    /// Pause after these specific agent types.
    pub pause_after: Vec<crate::agents::AgentType>,
    /// Disable pipeline-defined phase gates for this run.
    pub disable_phase_gates: bool,
    /// Override the configured permission mode for this run.
    /// `None` means use `cfg.providers.claude_code.permission_mode`.
    pub permission_mode: Option<PermissionMode>,
    /// Named pipeline override. `None` = auto-detect or AI planner.
    pub pipeline: Option<pipeline::PipelineKind>,
    /// Explicit conversation ID to attach this run to.
    pub conversation_id: Option<String>,
    /// Continue the most recent active conversation for this project.
    pub continue_last: bool,
    /// Override the DB path for this run. When set, `execute_objective` uses
    /// this path for DB access instead of deriving from `project_root`.
    /// Used by Grove Desktop where the DB lives in a central workspace directory.
    pub db_path: Option<std::path::PathBuf>,
    /// Abort handle for subprocess termination. When set, the provider registers
    /// subprocess PIDs and the engine checks the abort flag between stages.
    pub abort_handle: Option<abort_handle::AbortHandle>,
    /// External issue ID to link this run to (e.g. "PROJ-123", "42").
    /// If set without `issue`, the issue will be fetched from the tracker.
    pub issue_id: Option<String>,
    /// Full fetched issue data. When present, the issue context is seeded into
    /// the conversation and the objective is enriched before the run starts.
    pub issue: Option<crate::tracker::Issue>,
    /// Coding-agent provider override for this run (e.g. `"codex"`, `"gemini"`).
    /// When `Some`, overrides `cfg.providers.default` for the duration of the run.
    /// `None` = use the project's default provider.
    pub provider: Option<String>,
    /// Provider-native session ID to resume for this run.
    ///
    /// When `Some`, the first agent in the run passes this to the provider
    /// (e.g. `codex exec resume <thread_id>`) to continue the prior provider
    /// thread when detached resume is supported. `None` = start fresh and rely
    /// on Grove-managed continuity context.
    pub resume_provider_session_id: Option<String>,
    /// Called immediately after the run record is inserted into the DB, with
    /// the new run_id. Lets callers (e.g. Grove Desktop) push an event to the
    /// frontend the instant the run exists — no polling lag.
    pub on_run_created: Option<Box<dyn Fn(String) + Send + 'static>>,
    /// Callback for registering the agent's stdin handle (for interactive Q&A).
    /// Threaded through the engine into `ProviderRequest.input_handle_callback`.
    pub input_handle_callback: Option<
        std::sync::Arc<dyn Fn(crate::providers::agent_input::AgentInputHandle) + Send + Sync>,
    >,
    /// Callback for registering a persistent-run control handle keyed by run_id.
    pub run_control_callback: Option<
        std::sync::Arc<
            dyn Fn(String, crate::providers::claude_code_persistent::PersistentRunControlHandle)
                + Send
                + Sync,
        >,
    >,
}

impl std::fmt::Debug for RunOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunOptions")
            .field("budget_usd", &self.budget_usd)
            .field("max_agents", &self.max_agents)
            .field("model", &self.model)
            .field("interactive", &self.interactive)
            .field("pause_after", &self.pause_after)
            .field("disable_phase_gates", &self.disable_phase_gates)
            .field("permission_mode", &self.permission_mode)
            .field("pipeline", &self.pipeline)
            .field("conversation_id", &self.conversation_id)
            .field("continue_last", &self.continue_last)
            .field("db_path", &self.db_path)
            .field("abort_handle", &self.abort_handle)
            .field("issue_id", &self.issue_id)
            .field("issue", &self.issue)
            .field(
                "resume_provider_session_id",
                &self.resume_provider_session_id,
            )
            .field(
                "on_run_created",
                &self.on_run_created.as_ref().map(|_| "<callback>"),
            )
            .field(
                "input_handle_callback",
                &self.input_handle_callback.as_ref().map(|_| "<callback>"),
            )
            .field(
                "run_control_callback",
                &self.run_control_callback.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunExecutionResult {
    pub run_id: String,
    pub state: String,
    pub objective: String,
    pub report_path: Option<String>,
    pub plan: Vec<String>,
}

/// A task in the sequential work queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub objective: String,
    pub state: String,
    pub budget_usd: Option<f64>,
    pub priority: i64,
    pub run_id: Option<String>,
    pub queued_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub publish_status: Option<String>,
    pub publish_error: Option<String>,
    pub final_commit_sha: Option<String>,
    pub pr_url: Option<String>,
    /// Optional model override stored at queue time (e.g. `"claude-sonnet-4-6"`, `"o3"`).
    pub model: Option<String>,
    /// Optional coding-agent provider chosen at queue time (e.g. `"claude_code"`, `"codex"`).
    /// `None` = use `cfg.providers.default` at drain time.
    pub provider: Option<String>,
    /// Optional conversation thread to continue when this task executes.
    pub conversation_id: Option<String>,
    /// Provider-native session ID to resume when this task executes.
    ///
    /// When set, the first agent in the run passes this ID to the provider
    /// (e.g. `codex exec resume <thread_id>`) so the conversation thread
    /// continues from where the previous run left off. Sourced from the prior
    /// run's canonical `runs.provider_thread_id` when detached resume is safe.
    pub resume_provider_session_id: Option<String>,
    /// Named pipeline override stored at queue time (e.g. `"full"`, `"quick"`).
    /// `None` = auto-detect or AI planner at drain time.
    pub pipeline: Option<String>,
    /// Permission mode override stored at queue time (e.g. `"skip_all"`, `"human_gate"`).
    /// `None` = use `cfg.providers.claude_code.permission_mode` at drain time.
    pub permission_mode: Option<String>,
    /// Disable pipeline-defined phase gates when this queued task executes.
    pub disable_phase_gates: bool,
}

// ── Sub-task types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtaskRecord {
    pub id: String,
    pub run_id: String,
    pub session_id: Option<String>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub priority: i64,
    pub depends_on: Vec<String>,
    pub assigned_agent: Option<String>,
    pub files_hint: Vec<String>,
    pub todos: Vec<String>,
    pub result_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ── Plan step types ───────────────────────────────────────────────────────────

/// A single step inside a GROVE_PLAN file produced by the Planner agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrovePlanStep {
    pub id: String,
    pub agent_type: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub todos: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

/// The root object of a GROVE_PLAN_{run_id}.json file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrovePlanFile {
    #[serde(default)]
    pub summary: String,
    pub steps: Vec<GrovePlanStep>,
}

/// A persisted plan step row from the `plan_steps` DB table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub run_id: String,
    pub step_index: i64,
    pub wave: i64,
    pub agent_type: String,
    pub title: String,
    pub description: String,
    pub todos: Vec<String>,
    pub files: Vec<String>,
    pub depends_on: Vec<String>,
    pub status: String,
    pub session_id: Option<String>,
    pub result_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ── Cost report types ─────────────────────────────────────────────────────────

/// Spend breakdown for a single agent type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCostSummary {
    pub agent_type: String,
    pub total_cost_usd: f64,
    pub session_count: i64,
    pub avg_cost_usd: f64,
}

/// Spend summary for a single completed run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunCostSummary {
    pub run_id: String,
    pub cost_used_usd: f64,
    pub objective: String,
    pub created_at: String,
}

/// Full cost report returned by [`cost_report`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostReport {
    pub total_spent_usd: f64,
    pub total_runs: i64,
    pub by_agent: Vec<AgentCostSummary>,
    pub recent_runs: Vec<RunCostSummary>,
}

// ── Provider construction ─────────────────────────────────────────────────────

pub fn parse_permission_mode(value: Option<&str>) -> Option<PermissionMode> {
    match value {
        Some("skip_all") => Some(PermissionMode::SkipAll),
        Some("human_gate") => Some(PermissionMode::HumanGate),
        Some("autonomous_gate") => Some(PermissionMode::AutonomousGate),
        _ => None,
    }
}

pub(super) fn effective_pause_after(
    pause_after: &[crate::agents::AgentType],
    pipeline_checkpoints: &[crate::agents::AgentType],
    disable_phase_gates: bool,
) -> Vec<crate::agents::AgentType> {
    let mut effective = pause_after.to_vec();
    let _ = disable_phase_gates;
    for agent in pipeline_checkpoints {
        if !effective.contains(agent) {
            effective.push(*agent);
        }
    }
    effective
}

pub fn task_terminal_state(run_state: &str) -> &'static str {
    match run_state {
        "paused" => "cancelled",
        "failed" => "failed",
        _ => "completed",
    }
}

/// Build a provider from config, optionally overriding the default with a
/// per-run `provider_override` (e.g. `"codex"`, `"gemini"`, `"claude_code"`).
/// When `provider_override` is `Some`, it takes precedence over `cfg.providers.default`.
///
/// Returns `Err` if the resolved provider is disabled or not recognised so the
/// caller can surface a meaningful error instead of running with a no-op stub.
pub fn build_provider(
    cfg: &GroveConfig,
    project_root: &Path,
    provider_override: Option<&str>,
    permission_override: Option<PermissionMode>,
) -> crate::errors::GroveResult<Arc<dyn Provider>> {
    use crate::errors::GroveError;
    use crate::llm::{LlmAuthMode, LlmProviderKind, LlmRouter};
    use crate::providers::coding_agent::{CodingAgentProvider, GenericAdapter, get_adapter};
    use crate::providers::{ClaudeCodePersistentProvider, ClaudeCodeProvider};

    let effective = provider_override.unwrap_or(&cfg.providers.default);

    // 1. Claude Code CLI
    if effective == "claude_code" || effective == "claude_code_persistent" {
        if !cfg.providers.claude_code.enabled {
            return Err(GroveError::Config(format!(
                "provider '{effective}' is disabled in config"
            )));
        }
        crate::capability::ensure_claude_code_authenticated(&cfg.providers.claude_code.command)
            .map_err(GroveError::Config)?;
        let provider = ClaudeCodeProvider::new(
            cfg.providers.claude_code.command.clone(),
            cfg.providers.claude_code.timeout_seconds,
            permission_override
                .unwrap_or_else(|| cfg.providers.claude_code.permission_mode.clone()),
            cfg.providers.claude_code.allowed_tools.clone(),
            cfg.providers.claude_code.gatekeeper_model.clone(),
        )
        .with_max_output_bytes(cfg.providers.claude_code.max_output_bytes)
        .with_resource_limits(
            cfg.providers.claude_code.max_file_size_mb,
            cfg.providers.claude_code.max_open_files,
        );
        // Always wrap Claude Code with the persistent provider — persistent
        // mode is now the default execution path for all agents.
        return Ok(Arc::new(ClaudeCodePersistentProvider::new(
            provider,
            cfg.providers.claude_code.long_lived_run_host,
        )));
    }

    // 2. Coding-agent CLI providers (codex, gemini, aider, cursor, …)
    //
    // Resolution order:
    //   a) Check grove.yaml for an enabled/disabled override.
    //   b) Look up the dedicated adapter (known built-in agents).
    //   c) Fall back to GenericAdapter for agents defined only in grove.yaml.
    {
        // Respect enabled/disabled flag from grove.yaml if present.
        if let Some(agent_cfg) = cfg.providers.coding_agents.get(effective) {
            if !agent_cfg.enabled {
                return Err(GroveError::Config(format!(
                    "coding agent '{effective}' is disabled in config"
                )));
            }
        }

        // Try the dedicated per-agent adapter first (built-in agents).
        if let Some(adapter) = get_adapter(effective) {
            // Command: grove.yaml override takes precedence; adapter default otherwise.
            let command = cfg
                .providers
                .coding_agents
                .get(effective)
                .map(|c| c.command.clone())
                .unwrap_or_else(|| adapter.default_command().to_string());
            let timeout = cfg
                .providers
                .coding_agents
                .get(effective)
                .map(|c| c.timeout_seconds)
                .unwrap_or(300);
            let max_bytes = cfg
                .providers
                .coding_agents
                .get(effective)
                .map(|c| c.max_output_bytes)
                .unwrap_or(10 * 1024 * 1024);
            let max_file_size_mb = cfg
                .providers
                .coding_agents
                .get(effective)
                .and_then(|c| c.max_file_size_mb);
            let max_open_files = cfg
                .providers
                .coding_agents
                .get(effective)
                .and_then(|c| c.max_open_files);
            return Ok(Arc::new(
                CodingAgentProvider::new(adapter, command, timeout)
                    .with_max_output_bytes(max_bytes)
                    .with_resource_limits(max_file_size_mb, max_open_files),
            ));
        }

        // Fallback: build a GenericAdapter from full grove.yaml config fields.
        // This handles custom agents the user has added to grove.yaml that have
        // no dedicated built-in adapter.
        if let Some(agent_cfg) = cfg.providers.coding_agents.get(effective) {
            let adapter = Box::new(GenericAdapter::from_config(
                effective.to_string(),
                agent_cfg.command.clone(),
                agent_cfg.auto_approve_flag.clone(),
                agent_cfg.initial_prompt_flag.clone(),
                agent_cfg.use_keystroke_injection,
                agent_cfg.use_pty,
                agent_cfg.default_args.clone(),
                agent_cfg.model_flag.clone(),
            ));
            return Ok(Arc::new(
                CodingAgentProvider::new(
                    adapter,
                    agent_cfg.command.clone(),
                    agent_cfg.timeout_seconds,
                )
                .with_max_output_bytes(agent_cfg.max_output_bytes)
                .with_resource_limits(agent_cfg.max_file_size_mb, agent_cfg.max_open_files),
            ));
        }
    }

    // 3. Explicit LLM provider in config (user-key mode)
    if effective != "auto" {
        if let Some(kind) = LlmProviderKind::from_str(effective) {
            let model_override = cfg.providers.llm.model.as_deref();
            return Ok(LlmRouter::build_provider(kind, model_override));
        }
        return Err(GroveError::Config(format!(
            "unknown provider '{effective}' — set a valid provider in config or task options"
        )));
    }

    // 4. "auto" — read workspace DB selection, apply credit gate if needed
    if let Ok(_db_result) = crate::db::initialize(project_root) {
        let db_path = crate::config::db_path(project_root);
        if let Ok(conn) = DbHandle::new(project_root).connect() {
            if let Ok(ws_id) = workspace::ensure_workspace(&conn) {
                if let Ok(Some(sel)) = LlmRouter::get_workspace_selection(&conn, &ws_id) {
                    return match sel.auth_mode {
                        LlmAuthMode::UserKey => {
                            Ok(LlmRouter::build_provider(sel.kind, sel.model.as_deref()))
                        }
                        LlmAuthMode::WorkspaceCredits => {
                            LlmRouter::build_credit_gated_provider(sel.kind, &ws_id, &db_path)
                                .map_err(|e| {
                                    GroveError::Config(format!(
                                        "credit-gated provider setup failed: {e}"
                                    ))
                                })
                        }
                    };
                }
            }
        }
    }

    Err(GroveError::Config(
        "no provider configured — set providers.default in grove.yml or choose a coding agent when starting a task".to_string(),
    ))
}

// ── Stage checkpoint helper ────────────────────────────────────────────────────

/// Save a checkpoint immediately before a stage transition.
///
/// Captures active session IDs and current budget spend so the run can be
/// resumed from this point if the process crashes during the next stage.
/// Reads `budget_usd` and `cost_usd` directly from the `runs` table so
/// callers (including `engine.rs`) need not thread budget state through.
/// Non-fatal: failures log a warning but do not abort the run.
pub(crate) fn save_stage_checkpoint(conn: &Connection, run_id: &str, stage: &str) {
    let cp_id = format!("cp_{}", Uuid::new_v4().simple());

    // Collect active session IDs for this run.
    let active_sessions: Vec<String> = conn
        .prepare(
            "SELECT id FROM sessions WHERE run_id = ?1
             AND state IN ('running','queued','waiting')",
        )
        .and_then(|mut s| {
            s.query_map([run_id], |r| r.get(0))
                .map(|rows| rows.flatten().collect())
        })
        .unwrap_or_default();

    // Read allocated budget from runs; sum actual spend from sessions.
    let allocated_usd: f64 = conn
        .query_row(
            "SELECT COALESCE(budget_usd, 0.0) FROM runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap_or(0.0);
    let used_usd: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM sessions WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let payload = CheckpointPayload {
        run_id: run_id.to_string(),
        stage: stage.to_string(),
        active_sessions,
        pending_tasks: vec![],
        ownership: vec![],
        budget: BudgetSnapshot {
            allocated_usd,
            used_usd,
        },
    };

    if let Err(e) = checkpoint::save(conn, &cp_id, &payload) {
        tracing::warn!(
            error = %e,
            run_id = %run_id,
            stage = %stage,
            "pre-transition checkpoint failed — continuing without snapshot"
        );
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

pub fn execute_objective(
    project_root: &Path,
    cfg: &GroveConfig,
    objective: &str,
    options: RunOptions,
    provider: Arc<dyn Provider>,
) -> GroveResult<RunExecutionResult> {
    execute_objective_with_sink(project_root, cfg, objective, options, provider, None)
}

pub fn execute_objective_with_sink(
    project_root: &Path,
    cfg: &GroveConfig,
    objective: &str,
    options: RunOptions,
    provider: Arc<dyn Provider>,
    sink: Option<&dyn crate::providers::StreamSink>,
) -> GroveResult<RunExecutionResult> {
    let null_sink = crate::providers::NullSink;
    let effective_sink: &dyn crate::providers::StreamSink = sink.unwrap_or(&null_sink);
    // Pre-flight capability check — warn or block early on missing tools.
    let cap_report = crate::capability::detect_capabilities_with_db(
        cfg,
        project_root,
        options.db_path.as_deref(),
    );
    // no_remote is a common, intentional state (local-only repos) — log at info.
    // Anything worse (no_git, no_claude, etc.) is a genuine operational concern.
    if cap_report.level >= crate::capability::DegradationLevel::NoRemote {
        if cap_report.level == crate::capability::DegradationLevel::NoRemote {
            tracing::info!(
                level = cap_report.level.as_str(),
                "no git remote configured"
            );
        } else {
            tracing::warn!(
                level = cap_report.level.as_str(),
                "degraded environment detected"
            );
        }
    }
    if let Err(msg) = crate::capability::preflight_check(&cap_report, true) {
        return Err(GroveError::Runtime(format!("preflight failed: {msg}")));
    }

    let handle = match &options.db_path {
        Some(p) => DbHandle::from_db_path(p.clone()),
        None => DbHandle::new(project_root),
    };
    // data_root: where .grove/ lives (for reports, logs, etc.).
    // In centralized mode db_path = <virtual_root>/.grove/grove.db → data_root = <virtual_root>.
    // In CLI mode db_path is None → data_root = project_root.
    let data_root = match &options.db_path {
        Some(p) => p
            .parent()
            .and_then(|g| g.parent())
            .unwrap_or(project_root)
            .to_path_buf(),
        None => project_root.to_path_buf(),
    };
    let mut conn = handle.connect()?;

    // Crash recovery: detect runs stuck in non-terminal state with no
    // running process and mark them failed.
    let recovered = recover_crashed_runs(&mut conn);
    if recovered > 0 {
        tracing::info!(recovered, "recovered crashed runs at startup");
    }
    let recovered_publishes =
        crate::publish::recover_interrupted_publishes(&mut conn, project_root, cfg)?;
    if !recovered_publishes.is_empty() {
        tracing::info!(
            count = recovered_publishes.len(),
            "recovered interrupted publish flows"
        );
    }

    // ── Resolve conversation FIRST (needed for per-conversation lock) ────────
    // resolve_conversation only touches conversations/workspaces/projects tables
    // — no conflict with the run lock.
    let conversation_id = conversation::resolve_conversation(
        &mut conn,
        project_root,
        options.conversation_id.as_deref(),
        options.continue_last,
        Some(&cfg.worktree.branch_prefix),
        None,
        conversation::RUN_CONVERSATION_KIND,
    )?;

    // ── Per-conversation run lock + global concurrency cap ─────────────────
    // Enforced atomically inside a BEGIN IMMEDIATE transaction to prevent
    // TOCTOU races when multiple threads/processes start runs concurrently.
    let run_id = new_run_id();
    let budget_usd = options.budget_usd.unwrap_or(cfg.budgets.default_run_usd);

    let pipeline_configs = crate::config::agent_config::load_pipelines(project_root).ok();
    let run_intent =
        intent::select_run_intent(objective, options.pipeline, pipeline_configs.as_ref());
    eprintln!(
        "[ORCHESTRATOR] Using intent: {} ({})",
        run_intent.label, run_intent.rationale
    );
    let plan = run_intent.plan.clone();
    let pipeline_checkpoints = run_intent.phase_gates.clone();

    let effective_pause_after = effective_pause_after(
        &options.pause_after,
        &pipeline_checkpoints,
        options.disable_phase_gates,
    );

    acquire_run_slot(
        &mut conn,
        &run_id,
        objective,
        budget_usd,
        &conversation_id,
        cfg.runtime.max_concurrent_runs,
        cfg.runtime.lock_wait_timeout_secs,
        options.disable_phase_gates,
        options.provider.as_deref(),
        options.model.as_deref(),
    )?;
    // Notify the caller immediately — the run now exists in the DB.
    if let Some(ref cb) = options.on_run_created {
        cb(run_id.clone());
    }
    events::emit(
        &conn,
        &run_id,
        None,
        "run_created",
        json!({ "objective": objective, "budget_usd": budget_usd }),
    )?;
    conversation::record_user_message(&mut conn, &conversation_id, &run_id, objective)?;

    // ── Issue context seeding ────────────────────────────────────────────────
    // If an issue_id was provided without full issue data, fetch it from the
    // tracker. Then seed the issue context into the conversation and enrich
    // the objective so agents see the full problem description.
    let mut resolved_issue = options.issue.clone();
    if resolved_issue.is_none() {
        if let Some(ref issue_id) = options.issue_id {
            let registry =
                crate::tracker::registry::TrackerRegistry::from_config(cfg, project_root);
            if registry.is_active() {
                match registry.find_issue(issue_id) {
                    Ok(Some(issue)) => {
                        resolved_issue = Some(issue);
                    }
                    Ok(None) => {
                        tracing::warn!(issue_id = issue_id, "issue not found in any tracker");
                    }
                    Err(e) => {
                        tracing::warn!(issue_id = issue_id, error = %e, "failed to fetch issue");
                    }
                }
            }
        }
    }

    let effective_objective = if let Some(ref issue) = resolved_issue {
        // Seed issue context into the conversation as a system message
        issue_context::seed_issue_context(&mut conn, &conversation_id, &run_id, issue)?;

        // Cache the issue and link it to this run
        let project_id = conversation::derive_project_id(project_root);
        let _ = crate::tracker::cache_issue(&conn, issue, &project_id);
        let _ = crate::tracker::link_run_to_issue(&conn, &run_id, &issue.external_id);

        events::emit(
            &conn,
            &run_id,
            None,
            "issue_linked",
            json!({
                "issue_id": issue.external_id,
                "provider": issue.provider,
                "title": issue.title,
            }),
        )?;

        issue_context::enrich_objective(issue, objective)
    } else {
        objective.to_string()
    };

    save_stage_checkpoint(&conn, &run_id, "before_planning");
    transitions::apply_transition(&conn, &run_id, RunState::Created, RunState::Planning)?;

    // With the new pipeline system, the plan is fully determined by
    // PipelineKind::agents(). No separate planner agent needed.
    let effective_plan = plan;
    let db_plan_steps: Option<Vec<PlanStep>> = None;

    // Flatten effective_plan for display / logging.
    let flat_plan: Vec<&str> = effective_plan
        .iter()
        .flat_map(|s| s.iter().map(|a| a.as_str()))
        .collect();
    events::emit(
        &conn,
        &run_id,
        None,
        "plan_generated",
        json!({ "plan": flat_plan }),
    )?;
    if let Err(err) = run_memory::write_plan_log(
        project_root,
        &conversation_id,
        &run_id,
        objective,
        &effective_objective,
        &run_intent,
    ) {
        tracing::warn!(
            run_id = %run_id,
            conversation_id = %conversation_id,
            error = %err,
            "failed to write classic run plan log"
        );
    }

    // Git is optional — worktrees fall back to plain directories without it.
    let is_git_project = worktree::git_available()
        && worktree::git_ops::is_git_repo(project_root)
        && worktree::git_ops::has_commits(project_root);
    if !worktree::git_available() {
        eprintln!(
            "warning: git not found — worktrees will use plain directories, no commit history"
        );
    }

    // ── One-time git project setup (idempotent) ──────────────────────────────
    // Ensures `.grove/` is excluded from git tracking.
    if is_git_project {
        let _ = worktree::git_ops::git_ensure_grove_excluded(project_root);
        let _ = worktree::git_ops::git_ensure_grove_in_gitignore(project_root);
    }

    save_stage_checkpoint(&conn, &run_id, "before_executing");
    transitions::apply_transition(&conn, &run_id, RunState::Planning, RunState::Executing)?;

    let seeded_provider_thread_id = if provider.session_continuity_policy()
        == crate::providers::SessionContinuityPolicy::DetachedResume
    {
        options.resume_provider_session_id.as_deref()
    } else {
        None
    };
    // Record the provider, model, and any seeded resumable thread for
    // "Continue Task" and same-run detached-resume continuity.
    let _ = conn.execute(
        "UPDATE runs SET provider = ?1, model = ?2, provider_thread_id = ?3, disable_phase_gates = ?4, pipeline = ?5 WHERE id = ?6",
        params![
            provider.name(),
            options.model.as_deref(),
            seeded_provider_thread_id,
            options.disable_phase_gates,
            run_intent.label.as_str(),
            run_id
        ],
    );

    // Inject abort handle into provider so it can register subprocess PIDs.
    if let Some(ref handle) = options.abort_handle {
        provider.set_abort_handle(handle.clone());
    }

    match engine::run_agents(
        &mut conn,
        &run_id,
        &effective_objective,
        &effective_plan,
        Arc::clone(&provider),
        cfg,
        project_root,
        options.model.as_deref(),
        Some(run_intent.shared_context.as_str()),
        Some(&run_intent.agent_briefs),
        options.interactive,
        &effective_pause_after,
        db_plan_steps.as_deref(),
        Some(&conversation_id),
        options.abort_handle.as_ref(),
        options.resume_provider_session_id.clone(),
        effective_sink,
        options.input_handle_callback.as_ref(),
        options.run_control_callback.as_ref(),
    ) {
        Ok(()) => {
            let cost_used: f64 = conn
                .query_row(
                    "SELECT cost_used_usd FROM runs WHERE id=?1",
                    [&run_id],
                    |r| r.get(0),
                )
                .unwrap_or(0.0);

            conn.execute(
                "UPDATE runs SET cost_used_usd=?1, updated_at=?2 WHERE id=?3",
                params![cost_used, Utc::now().to_rfc3339(), run_id],
            )?;

            events::emit(
                &conn,
                &run_id,
                None,
                "run_completed",
                json!({ "provider": provider.name() }),
            )?;

            let report = reporting::generate_report_with_conn(&conn, &data_root, &run_id)?;
            if let Err(err) = run_memory::write_verdict_log(
                project_root,
                &conversation_id,
                &run_id,
                objective,
                &run_intent,
                RunState::Completed.as_str(),
                None,
                Some(report.as_path()),
            ) {
                tracing::warn!(
                    run_id = %run_id,
                    conversation_id = %conversation_id,
                    error = %err,
                    "failed to write classic run verdict log"
                );
            }

            // Record token filter stats and clean up shim files.
            if let Some(filter_state) = crate::token_filter::metrics::read_stats(project_root) {
                if let Err(e) =
                    crate::token_filter::metrics::record_to_db(&conn, &run_id, &filter_state)
                {
                    tracing::debug!(error = %e, "failed to record token filter metrics");
                }
            }
            crate::token_filter::shim::teardown(project_root);

            let flat_plan_owned: Vec<String> = effective_plan
                .iter()
                .flat_map(|s| s.iter().map(|a| a.as_str().to_string()))
                .collect();

            Ok(RunExecutionResult {
                run_id,
                state: RunState::Completed.as_str().to_string(),
                objective: objective.to_string(),
                report_path: Some(report.to_string_lossy().to_string()),
                plan: flat_plan_owned,
            })
        }

        Err(GroveError::Aborted) => {
            // Abort: subprocess(es) already killed. Transition to Paused
            // (abort_run may have already done this; tolerate double-transition).
            let current_state: Option<String> = conn
                .query_row("SELECT state FROM runs WHERE id = ?1", [&run_id], |r| {
                    r.get(0)
                })
                .ok();
            if let Some(ref st) = current_state {
                if st != "paused" && st != "failed" && st != "completed" {
                    if let Some(from_state) = RunState::from_str(st) {
                        let _ = transitions::apply_transition(
                            &conn,
                            &run_id,
                            from_state,
                            RunState::Paused,
                        );
                    }
                }
            }
            if let Err(err) = run_memory::write_verdict_log(
                project_root,
                &conversation_id,
                &run_id,
                objective,
                &run_intent,
                RunState::Paused.as_str(),
                Some("Run was paused or aborted before completion."),
                None,
            ) {
                tracing::warn!(
                    run_id = %run_id,
                    conversation_id = %conversation_id,
                    error = %err,
                    "failed to write classic run verdict log"
                );
            }

            let flat_plan_owned: Vec<String> = effective_plan
                .iter()
                .flat_map(|s| s.iter().map(|a| a.as_str().to_string()))
                .collect();
            Ok(RunExecutionResult {
                run_id,
                state: RunState::Paused.as_str().to_string(),
                objective: objective.to_string(),
                report_path: None,
                plan: flat_plan_owned,
            })
        }

        Err(err) => {
            tracing::error!(run_id = %run_id, error = %err, "run_agents failed");

            // Transition the run to Failed in the DB so it doesn't stay stuck
            // at Executing forever (which also blocks future runs via the lock).
            // The engine's fail_run() may have already done this for some error
            // paths, so we read the current state first and only transition if
            // still in a non-terminal state.
            let current_state: Option<String> = conn
                .query_row("SELECT state FROM runs WHERE id = ?1", [&run_id], |r| {
                    r.get(0)
                })
                .ok();
            if let Some(ref st) = current_state {
                if st != "failed" && st != "completed" {
                    let from = RunState::from_str(st);
                    if let Some(from_state) = from {
                        let _ = transitions::apply_transition(
                            &conn,
                            &run_id,
                            from_state,
                            RunState::Failed,
                        );
                    }
                }
            }
            if let Err(write_err) = run_memory::write_verdict_log(
                project_root,
                &conversation_id,
                &run_id,
                objective,
                &run_intent,
                RunState::Failed.as_str(),
                Some(&err.to_string()),
                None,
            ) {
                tracing::warn!(
                    run_id = %run_id,
                    conversation_id = %conversation_id,
                    error = %write_err,
                    "failed to write classic run verdict log"
                );
            }

            let flat_plan_owned: Vec<String> = effective_plan
                .iter()
                .flat_map(|s| s.iter().map(|a| a.as_str().to_string()))
                .collect();
            Ok(RunExecutionResult {
                run_id,
                state: RunState::Failed.as_str().to_string(),
                objective: objective.to_string(),
                report_path: None,
                plan: flat_plan_owned,
            })
        }
    }
}

pub fn list_runs(project_root: &Path, limit: i64) -> GroveResult<Vec<RunRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let mut stmt = conn.prepare_cached(
        "SELECT id, objective, state, budget_usd, cost_used_usd, publish_status, publish_error, final_commit_sha, pr_url, created_at, updated_at, conversation_id, pipeline, current_agent
         FROM runs ORDER BY created_at DESC LIMIT ?1",
    )?;

    let rows = stmt.query_map([limit], |r| {
        Ok(RunRecord {
            id: r.get(0)?,
            objective: r.get(1)?,
            state: r.get(2)?,
            budget_usd: r.get(3)?,
            cost_used_usd: r.get(4)?,
            publish_status: r.get(5)?,
            publish_error: r.get(6)?,
            final_commit_sha: r.get(7)?,
            pr_url: r.get(8)?,
            created_at: r.get(9)?,
            updated_at: r.get(10)?,
            conversation_id: r.get(11)?,
            pipeline: r.get(12)?,
            current_agent: r.get(13)?,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    derive_run_publish_statuses(project_root, &conn, &mut out)?;
    Ok(out)
}

pub fn list_runs_for_conversation(
    project_root: &Path,
    conversation_id: &str,
) -> GroveResult<Vec<RunRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let mut stmt = conn.prepare_cached(
        "SELECT id, objective, state, budget_usd, cost_used_usd, publish_status, publish_error, final_commit_sha, pr_url, created_at, updated_at, conversation_id, pipeline, current_agent
         FROM runs WHERE conversation_id = ?1 ORDER BY created_at DESC",
    )?;

    let rows = stmt.query_map(params![conversation_id], |r| {
        Ok(RunRecord {
            id: r.get(0)?,
            objective: r.get(1)?,
            state: r.get(2)?,
            budget_usd: r.get(3)?,
            cost_used_usd: r.get(4)?,
            publish_status: r.get(5)?,
            publish_error: r.get(6)?,
            final_commit_sha: r.get(7)?,
            pr_url: r.get(8)?,
            created_at: r.get(9)?,
            updated_at: r.get(10)?,
            conversation_id: r.get(11)?,
            pipeline: r.get(12)?,
            current_agent: r.get(13)?,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    derive_run_publish_statuses(project_root, &conn, &mut out)?;
    Ok(out)
}

fn derive_run_publish_statuses(
    _workspace_root: &Path,
    conn: &Connection,
    runs: &mut [RunRecord],
) -> GroveResult<()> {
    // ── Batch-fetch conversations and projects (2 queries total) ────────
    let conv_ids: Vec<&str> = runs
        .iter()
        .filter_map(|r| r.conversation_id.as_deref())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let conv_map = crate::db::repositories::conversations_repo::get_batch(conn, &conv_ids)?;

    let project_ids: Vec<&str> = conv_map
        .values()
        .map(|c| c.project_id.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let project_map = crate::db::repositories::projects_repo::get_batch(conn, &project_ids)?;

    // ── Derive publish status per run (zero DB queries in loop) ────────
    for run in runs.iter_mut() {
        let Some(conversation_id) = run.conversation_id.as_ref() else {
            continue;
        };

        let conv = match conv_map.get(conversation_id) {
            Some(c) => c,
            None => continue,
        };
        let branch_name = conv.branch_name.as_deref();
        let repo_root = project_map
            .get(&conv.project_id)
            .map(|p| std::path::PathBuf::from(&p.root_path));

        let project_root = match repo_root.as_deref() {
            Some(r) if crate::worktree::git_ops::is_git_repo(r) => r,
            _ => continue,
        };

        let owned_commit = run
            .final_commit_sha
            .clone()
            .or_else(|| detect_run_owned_commit_sha(project_root, branch_name, &run.id));

        if run.final_commit_sha.is_none() {
            run.final_commit_sha = owned_commit.clone();
        }

        let is_remote_published = owned_commit.as_ref().is_some_and(|sha| {
            branch_name.is_some_and(|branch| {
                crate::worktree::git_ops::git_remote_branch_exists(project_root, "origin", branch)
                    && crate::worktree::git_ops::git_ref_contains_commit(
                        project_root,
                        sha,
                        &format!("refs/remotes/origin/{branch}"),
                    )
            })
        });
        let default_branch_contains = owned_commit.as_ref().is_some_and(|sha| {
            crate::worktree::git_ops::detect_default_branch(project_root)
                .ok()
                .is_some_and(|default_branch| {
                    crate::worktree::git_ops::git_remote_branch_exists(
                        project_root,
                        "origin",
                        &default_branch,
                    ) && crate::worktree::git_ops::git_ref_contains_commit(
                        project_root,
                        sha,
                        &format!("refs/remotes/origin/{default_branch}"),
                    )
                })
        });
        let preserved_published = run.publish_status == "published" && run.pr_url.is_some();

        run.publish_status =
            if is_remote_published || default_branch_contains || preserved_published {
                "published".to_string()
            } else if owned_commit.is_some() {
                if run.publish_status == "failed" {
                    "failed".to_string()
                } else {
                    "pending_retry".to_string()
                }
            } else if run.publish_status == "failed" {
                "failed".to_string()
            } else {
                "skipped_no_changes".to_string()
            };
    }

    Ok(())
}

fn detect_run_owned_commit_sha(
    project_root: &Path,
    branch_name: Option<&str>,
    run_id: &str,
) -> Option<String> {
    let branch_ref = branch_name
        .filter(|branch| !branch.trim().is_empty())
        .map(|branch| format!("refs/heads/{branch}"))
        .unwrap_or_else(|| "HEAD".to_string());
    let output = std::process::Command::new("git")
        .args([
            "log",
            "-n",
            "200",
            "--format=%H%x1f%s%x1f%b%x1e",
            &branch_ref,
        ])
        .current_dir(project_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let needle = format!("[run: {run_id}");
    let body_marker = format!("Grove-Run: {run_id}");
    let text = String::from_utf8_lossy(&output.stdout);
    for record in text.split('\u{1e}') {
        let trimmed = record.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut fields = trimmed.split('\u{1f}');
        let Some(sha) = fields.next().map(str::trim) else {
            continue;
        };
        let subject = fields.next().unwrap_or_default();
        let body = fields.next().unwrap_or_default();
        if subject.contains(&needle) || body.contains(&body_marker) {
            return Some(sha.to_string());
        }
    }
    None
}

pub fn abort_run(project_root: &Path, run_id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let (objective, state_str, budget_usd): (String, String, f64) = conn.query_row(
        "SELECT objective, state, budget_usd FROM runs WHERE id=?1",
        [run_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    let current_state = RunState::from_str(&state_str)
        .ok_or_else(|| GroveError::Runtime(format!("unknown run state '{state_str}'")))?;

    abort::abort_gracefully(&conn, run_id, &objective, budget_usd, current_state)
}

pub fn resume_run(project_root: &Path, run_id: &str) -> GroveResult<RunExecutionResult> {
    let handle = DbHandle::new(project_root);
    let mut conn = handle.connect()?;
    let cfg = GroveConfig::load_or_create(project_root)?;

    let state_str: String =
        conn.query_row("SELECT state FROM runs WHERE id=?1", [run_id], |r| r.get(0))?;

    let current_state = RunState::from_str(&state_str)
        .ok_or_else(|| GroveError::Runtime(format!("unknown run state '{state_str}'")))?;

    let provider: Arc<dyn Provider> = Arc::new(crate::providers::MockProvider);
    resume::resume_from_checkpoint(
        &mut conn,
        run_id,
        project_root,
        &cfg,
        provider,
        current_state,
    )
}

pub fn retry_publish_run(
    project_root: &Path,
    run_id: &str,
) -> GroveResult<crate::publish::PublishResult> {
    crate::publish::retry_publish(project_root, run_id)
}

pub fn run_events(project_root: &Path, run_id: &str) -> GroveResult<Vec<events::EventRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    // Cap display output at 200 rows to prevent unbounded memory use on
    // long-running runs with many events.
    events::list_for_run_tail(&conn, run_id, 200)
}

// ── Task queue API ────────────────────────────────────────────────────────────

/// Insert a task into the queue using an existing DB connection.
///
/// This is the low-level insertion function. It does NOT validate the
/// conversation — the caller is responsible for ensuring the conversation
/// exists and is valid. Use `queue_task` for the validated public API.
#[allow(clippy::too_many_arguments)]
pub fn insert_queued_task(
    conn: &Connection,
    objective: &str,
    budget_usd: Option<f64>,
    priority: i64,
    model: Option<&str>,
    provider: Option<&str>,
    conversation_id: Option<&str>,
    resume_provider_session_id: Option<&str>,
    pipeline: Option<&str>,
    permission_mode: Option<&str>,
    disable_phase_gates: bool,
) -> GroveResult<TaskRecord> {
    let task_id = format!(
        "task_{}_{}",
        Utc::now().format("%Y%m%d_%H%M%S"),
        &Uuid::new_v4().simple().to_string()[..8]
    );
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO tasks (id, objective, state, budget_usd, priority, run_id, queued_at, \
                            model, provider, conversation_id, resume_provider_session_id, \
                            pipeline, permission_mode, disable_phase_gates)
         VALUES (?1, ?2, 'queued', ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            task_id,
            objective,
            budget_usd,
            priority,
            now,
            model,
            provider,
            conversation_id,
            resume_provider_session_id,
            pipeline,
            permission_mode,
            disable_phase_gates,
        ],
    )?;

    Ok(TaskRecord {
        id: task_id,
        objective: objective.to_string(),
        state: "queued".to_string(),
        budget_usd,
        priority,
        run_id: None,
        queued_at: now,
        started_at: None,
        completed_at: None,
        publish_status: None,
        publish_error: None,
        final_commit_sha: None,
        pr_url: None,
        model: model.map(str::to_string),
        provider: provider.map(str::to_string),
        conversation_id: conversation_id.map(str::to_string),
        resume_provider_session_id: resume_provider_session_id.map(str::to_string),
        pipeline: pipeline.map(str::to_string),
        permission_mode: permission_mode.map(str::to_string),
        disable_phase_gates,
    })
}

/// Add an objective to the task queue. Returns the new `TaskRecord`.
///
/// If `conversation_id` is provided, the conversation must exist, be active,
/// and belong to the same project as `project_root`. This prevents cross-project
/// conversation leakage and ensures conversations are created before tasks.
#[allow(clippy::too_many_arguments)]
pub fn queue_task(
    project_root: &Path,
    objective: &str,
    budget_usd: Option<f64>,
    priority: i64,
    model: Option<&str>,
    provider: Option<&str>,
    conversation_id: Option<&str>,
    resume_provider_session_id: Option<&str>,
    pipeline: Option<&str>,
    permission_mode: Option<&str>,
    disable_phase_gates: bool,
) -> GroveResult<TaskRecord> {
    // Validate inputs before opening the database connection.
    if objective.trim().is_empty() {
        return Err(GroveError::ValidationError {
            field: "objective".into(),
            message: "objective cannot be empty".into(),
        });
    }
    if let Some(budget) = budget_usd {
        if budget <= 0.0 {
            return Err(GroveError::ValidationError {
                field: "budget_usd".into(),
                message: "budget must be positive".into(),
            });
        }
    }

    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    // Validate conversation_id at queue time (fail early, not at drain time).
    if let Some(conv_id) = conversation_id {
        let conv = crate::db::repositories::conversations_repo::get(&conn, conv_id)?;
        let project_id = conversation::derive_project_id(project_root);
        if conv.project_id != project_id {
            return Err(GroveError::Runtime(format!(
                "conversation '{conv_id}' belongs to a different project. \
                 Tasks can only be queued into conversations from the same project."
            )));
        }
        if conv.state != "active" {
            return Err(GroveError::Runtime(format!(
                "conversation '{conv_id}' is not active (state: '{}'). \
                 Tasks can only be queued into active conversations.",
                conv.state
            )));
        }
    }

    insert_queued_task(
        &conn,
        objective,
        budget_usd,
        priority,
        model,
        provider,
        conversation_id,
        resume_provider_session_id,
        pipeline,
        permission_mode,
        disable_phase_gates,
    )
}

/// Returns `true` if the given conversation currently has an active run.
pub fn has_active_run_for_conversation(conn: &Connection, conversation_id: &str) -> bool {
    conn.query_row(
        "SELECT id FROM runs WHERE conversation_id = ?1 \
         AND state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging') LIMIT 1",
        params![conversation_id],
        |r| r.get::<_, String>(0),
    )
    .is_ok()
}

/// List all tasks ordered by priority (desc) then queued_at (asc).
pub fn list_tasks(project_root: &Path) -> GroveResult<Vec<TaskRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let mut stmt = conn.prepare_cached(
        "SELECT id, objective, state, budget_usd, priority, run_id,
                queued_at, started_at, completed_at, publish_status, publish_error,
                final_commit_sha, pr_url, model, conversation_id, provider,
                resume_provider_session_id, pipeline, permission_mode, disable_phase_gates
         FROM tasks
         ORDER BY CASE state WHEN 'queued' THEN 0 WHEN 'running' THEN 1 ELSE 2 END,
                  priority DESC, queued_at ASC",
    )?;

    let rows = stmt.query_map([], |r| {
        Ok(TaskRecord {
            id: r.get(0)?,
            objective: r.get(1)?,
            state: r.get(2)?,
            budget_usd: r.get(3)?,
            priority: r.get(4)?,
            run_id: r.get(5)?,
            queued_at: r.get(6)?,
            started_at: r.get(7)?,
            completed_at: r.get(8)?,
            publish_status: r.get(9)?,
            publish_error: r.get(10)?,
            final_commit_sha: r.get(11)?,
            pr_url: r.get(12)?,
            model: r.get(13)?,
            conversation_id: r.get(14)?,
            provider: r.get(15)?,
            resume_provider_session_id: r.get(16)?,
            pipeline: r.get(17)?,
            permission_mode: r.get(18)?,
            disable_phase_gates: r.get(19)?,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// List tasks for a specific conversation, ordered by priority (desc) then queued_at (asc).
pub fn list_tasks_for_conversation(
    project_root: &Path,
    conversation_id: &str,
) -> GroveResult<Vec<TaskRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let mut stmt = conn.prepare_cached(
        "SELECT id, objective, state, budget_usd, priority, run_id,
                queued_at, started_at, completed_at, publish_status, publish_error,
                final_commit_sha, pr_url, model, conversation_id, provider,
                resume_provider_session_id, pipeline, permission_mode, disable_phase_gates
         FROM tasks
         WHERE conversation_id = ?1
         ORDER BY CASE state WHEN 'queued' THEN 0 WHEN 'running' THEN 1 ELSE 2 END,
                  priority DESC, queued_at ASC",
    )?;

    let rows = stmt.query_map([conversation_id], |r| {
        Ok(TaskRecord {
            id: r.get(0)?,
            objective: r.get(1)?,
            state: r.get(2)?,
            budget_usd: r.get(3)?,
            priority: r.get(4)?,
            run_id: r.get(5)?,
            queued_at: r.get(6)?,
            started_at: r.get(7)?,
            completed_at: r.get(8)?,
            publish_status: r.get(9)?,
            publish_error: r.get(10)?,
            final_commit_sha: r.get(11)?,
            pr_url: r.get(12)?,
            model: r.get(13)?,
            conversation_id: r.get(14)?,
            provider: r.get(15)?,
            resume_provider_session_id: r.get(16)?,
            pipeline: r.get(17)?,
            permission_mode: r.get(18)?,
            disable_phase_gates: r.get(19)?,
        })
    })?;

    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Cancel a queued task. Only tasks in 'queued' state can be cancelled.
pub fn cancel_task(project_root: &Path, task_id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let affected = conn.execute(
        "UPDATE tasks SET state='cancelled', completed_at=?1 WHERE id=?2 AND state='queued'",
        params![Utc::now().to_rfc3339(), task_id],
    )?;

    if affected == 0 {
        return Err(GroveError::Runtime(format!(
            "task '{task_id}' not found or is not in 'queued' state"
        )));
    }
    Ok(())
}

/// Reconcile stale tasks stuck in 'running' state.
///
/// Detects tasks that claim to be running but whose associated conversation
/// has no active run (the run completed, failed, was aborted, or the process
/// crashed). Marks them as 'failed' so the queue can move forward.
///
/// Returns the number of tasks reconciled.
pub fn reconcile_stale_tasks(project_root: &Path) -> GroveResult<usize> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let now = Utc::now().to_rfc3339();

    // A task is stale if it's 'running' but its conversation has no active run.
    // Tasks with no conversation_id that are stuck running are also reconciled.
    let affected = conn.execute(
        "UPDATE tasks SET state='failed', completed_at=?1
         WHERE state='running'
           AND (conversation_id IS NULL
                OR NOT EXISTS (
                    SELECT 1 FROM runs r
                    WHERE r.conversation_id = tasks.conversation_id
                      AND r.state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging')
                ))",
        params![now],
    )?;
    Ok(affected)
}

/// Returns `true` if there is currently an active run, including a gate wait.
pub fn has_active_run(project_root: &Path) -> GroveResult<bool> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let active: Option<String> = conn
        .query_row(
            "SELECT id FROM runs WHERE state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging') LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    Ok(active.is_some())
}

/// Returns the number of conversations that currently have an active run.
pub fn active_conversation_count(project_root: &Path) -> GroveResult<i64> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT conversation_id) FROM runs \
         WHERE state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging') \
         AND conversation_id IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// Pop the highest-priority queued task and mark it as 'running'.
///
/// Atomically selects and claims a task inside `BEGIN IMMEDIATE` to prevent
/// two concurrent drainers from picking up the same task.
///
/// Skips tasks whose conversation already has an active run — this enforces
/// the 1-active-run-per-conversation invariant at the queue level.
/// Returns `None` if no eligible task is available.
pub fn dequeue_next_task(project_root: &Path) -> GroveResult<Option<TaskRecord>> {
    let handle = DbHandle::new(project_root);
    let mut conn = handle.connect()?;

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // Find the highest-priority queued task whose conversation does NOT
    // already have an active run. Tasks with no conversation_id are always eligible.
    let task: Option<TaskRecord> = tx
        .query_row(
            "SELECT t.id, t.objective, t.state, t.budget_usd, t.priority, t.run_id,
                    t.queued_at, t.started_at, t.completed_at, t.model, t.provider,
                    t.conversation_id, t.resume_provider_session_id,
                    t.pipeline, t.permission_mode, t.disable_phase_gates
             FROM tasks t
             WHERE t.state = 'queued'
               AND (t.conversation_id IS NULL
                    OR NOT EXISTS (
                        SELECT 1 FROM runs r
                        WHERE r.conversation_id = t.conversation_id
                          AND r.state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging')
                    ))
             ORDER BY t.priority DESC, t.queued_at ASC LIMIT 1",
            [],
            |r| {
                Ok(TaskRecord {
                    id: r.get(0)?,
                    objective: r.get(1)?,
                    state: r.get(2)?,
                    budget_usd: r.get(3)?,
                    priority: r.get(4)?,
                    run_id: r.get(5)?,
                    queued_at: r.get(6)?,
                    started_at: r.get(7)?,
                    completed_at: r.get(8)?,
                    publish_status: None,
                    publish_error: None,
                    final_commit_sha: None,
                    pr_url: None,
                    model: r.get(9)?,
                    provider: r.get(10)?,
                    conversation_id: r.get(11)?,
                    resume_provider_session_id: r.get(12)?,
                    pipeline: r.get(13)?,
                    permission_mode: r.get(14)?,
                    disable_phase_gates: r.get(15)?,
                })
            },
        )
        .ok();

    if let Some(ref t) = task {
        let now = Utc::now().to_rfc3339();
        tx.execute(
            "UPDATE tasks SET state='running', started_at=?1 WHERE id=?2",
            params![now, t.id],
        )?;
    }

    tx.commit()?;
    Ok(task)
}

/// Mark a task as completed or failed, recording the resulting `run_id`.
///
/// Only updates tasks that are still in 'running' state — this prevents
/// overwriting a state already set by the abort handler.
pub fn finish_task(
    project_root: &Path,
    task_id: &str,
    new_state: &str, // "completed" or "failed" or "cancelled"
    run_id: Option<&str>,
) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let (publish_status, publish_error, final_commit_sha, pr_url) = if let Some(run_id) = run_id {
        conn.query_row(
            "SELECT publish_status, publish_error, final_commit_sha, pr_url FROM runs WHERE id = ?1",
            [run_id],
            |r| Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            )),
        ).unwrap_or((None, None, None, None))
    } else {
        (None, None, None, None)
    };

    conn.execute(
        "UPDATE tasks
         SET state=?1, run_id=?2, completed_at=?3, publish_status=?4, publish_error=?5, final_commit_sha=?6, pr_url=?7
         WHERE id=?8 AND state='running'",
        params![
            new_state,
            run_id,
            Utc::now().to_rfc3339(),
            publish_status,
            publish_error,
            final_commit_sha,
            pr_url,
            task_id
        ],
    )?;
    Ok(())
}

/// Mark a task as completed/failed and publish a `TaskFinished` event to the
/// automation event bus so the workflow engine can advance any dependent DAG.
///
/// This is a thin wrapper around `finish_task`. Callers that don't have an
/// event bus can continue to use `finish_task` directly.
pub fn finish_task_and_notify(
    project_root: &Path,
    task_id: &str,
    new_state: &str,
    run_id: Option<&str>,
    event_bus: Option<&crate::automation::event_bus::EventBus>,
) -> GroveResult<()> {
    finish_task(project_root, task_id, new_state, run_id)?;
    if let Some(bus) = event_bus {
        bus.publish(
            crate::automation::event_bus::AutomationEvent::TaskFinished {
                task_id: task_id.to_string(),
                state: new_state.to_string(),
                run_id: run_id.map(str::to_string),
            },
        );
    }
    Ok(())
}

/// Cancel all 'running' tasks for a conversation. Called when a run is aborted
/// so the task queue immediately reflects the abort and the next task can start.
pub fn cancel_running_tasks_for_conversation(
    project_root: &Path,
    conversation_id: &str,
) -> GroveResult<usize> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let now = Utc::now().to_rfc3339();
    let affected = conn.execute(
        "UPDATE tasks SET state='cancelled', completed_at=?1 WHERE conversation_id=?2 AND state='running'",
        params![now, conversation_id],
    )?;
    Ok(affected)
}

/// Delete a task from the database. Any state is valid.
pub fn delete_task(project_root: &Path, task_id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let affected = conn.execute("DELETE FROM tasks WHERE id = ?1", params![task_id])?;
    if affected == 0 {
        return Err(GroveError::Runtime(format!("task '{task_id}' not found")));
    }
    Ok(())
}

/// Clear all terminal-state tasks (failed, completed, cancelled).
/// Returns the number of tasks deleted.
pub fn clear_terminal_tasks(project_root: &Path) -> GroveResult<usize> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let affected = conn.execute(
        "DELETE FROM tasks WHERE state IN ('failed', 'completed', 'cancelled')",
        [],
    )?;
    Ok(affected)
}

/// Re-queue a failed task: creates a new 'queued' task with the same
/// objective, budget, model, and conversation. Returns the new task.
pub fn retry_task(project_root: &Path, task_id: &str) -> GroveResult<TaskRecord> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let old = conn.query_row(
        "SELECT objective, budget_usd, model, provider, conversation_id, disable_phase_gates FROM tasks WHERE id = ?1",
        params![task_id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<f64>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, bool>(5)?,
            ))
        },
    ).map_err(|_| GroveError::Runtime(format!("task '{task_id}' not found")))?;

    let (objective, budget_usd, model, provider, conversation_id, disable_phase_gates) = old;
    insert_queued_task(
        &conn,
        &objective,
        budget_usd,
        0,
        model.as_deref(),
        provider.as_deref(),
        conversation_id.as_deref(),
        None, // retry starts a fresh provider session
        None, // retry inherits pipeline from config
        None, // retry inherits permission_mode from config
        disable_phase_gates,
    )
}

/// Delete a completed or cancelled task (used for auto-cleanup after drain).
pub fn delete_completed_task(project_root: &Path, task_id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    conn.execute(
        "DELETE FROM tasks WHERE id = ?1 AND state IN ('completed', 'cancelled')",
        params![task_id],
    )?;
    Ok(())
}

/// Build a cost breakdown report.
///
/// - `total_spent_usd` / `total_runs` come from the `runs` table (always accurate).
/// - `by_agent` is grouped from `sessions.cost_usd` (NULL rows excluded).
/// - `recent_runs` lists the most recent `recent_run_limit` completed runs.
pub fn cost_report(project_root: &Path, recent_run_limit: i64) -> GroveResult<CostReport> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let (total_spent_usd, total_runs): (f64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(cost_used_usd),0), COUNT(*) FROM runs WHERE state='completed'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let mut by_agent_stmt = conn.prepare_cached(
        "SELECT agent_type, SUM(cost_usd), COUNT(*), AVG(cost_usd)
         FROM sessions
         WHERE state='completed' AND cost_usd > 0
         GROUP BY agent_type
         ORDER BY SUM(cost_usd) DESC",
    )?;
    let by_agent: Vec<AgentCostSummary> = by_agent_stmt
        .query_map([], |r| {
            Ok(AgentCostSummary {
                agent_type: r.get(0)?,
                total_cost_usd: r.get(1)?,
                session_count: r.get(2)?,
                avg_cost_usd: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut recent_stmt = conn.prepare_cached(
        "SELECT id, cost_used_usd, objective, created_at
         FROM runs
         WHERE state='completed'
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let recent_runs: Vec<RunCostSummary> = recent_stmt
        .query_map([recent_run_limit], |r| {
            Ok(RunCostSummary {
                run_id: r.get(0)?,
                cost_used_usd: r.get(1)?,
                objective: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(CostReport {
        total_spent_usd,
        total_runs,
        by_agent,
        recent_runs,
    })
}

/// List all sub-tasks for a run, ordered by priority (desc) then creation time.
pub fn list_subtasks(project_root: &Path, run_id: &str) -> GroveResult<Vec<SubtaskRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    let mut stmt = conn.prepare_cached(
        "SELECT id, run_id, session_id, title, description, status, priority,
                depends_on_json, assigned_agent, files_hint_json, todos_json,
                result_summary, created_at, updated_at
         FROM subtasks WHERE run_id=?1
         ORDER BY priority DESC, created_at ASC",
    )?;

    let rows = stmt.query_map([run_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, String>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, String>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, Option<String>>(11)?,
            r.get::<_, String>(12)?,
            r.get::<_, String>(13)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (
            id,
            run_id_col,
            session_id,
            title,
            description,
            status,
            priority,
            depends_on_json,
            assigned_agent,
            files_hint_json,
            todos_json,
            result_summary,
            created_at,
            updated_at,
        ) = row?;

        let depends_on: Vec<String> = serde_json::from_str(&depends_on_json).unwrap_or_default();
        let files_hint: Vec<String> = serde_json::from_str(&files_hint_json).unwrap_or_default();
        let todos: Vec<String> = serde_json::from_str(&todos_json).unwrap_or_default();

        out.push(SubtaskRecord {
            id,
            run_id: run_id_col,
            session_id,
            title,
            description,
            status,
            priority,
            depends_on,
            assigned_agent,
            files_hint,
            todos,
            result_summary,
            created_at,
            updated_at,
        });
    }
    Ok(out)
}

/// List all plan steps for a run, ordered by wave then step_index.
pub fn list_plan_steps(project_root: &Path, run_id: &str) -> GroveResult<Vec<PlanStep>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    plan_steps_repo::list_for_run(&conn, run_id)
}

/// List all sessions for a run.
///
/// Returns `Err` if the run ID does not exist so callers can distinguish
/// "no sessions yet" from "run not found".
pub fn list_sessions(
    project_root: &Path,
    run_id: &str,
) -> GroveResult<Vec<crate::agents::session_record::SessionRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    require_run_exists(&conn, run_id)?;
    crate::agents::lifecycle::list_for_run(&conn, run_id)
}

/// List currently held ownership locks. When `run_id` is `Some`, returns only
/// locks for that run using a targeted SQL query (no full-table scan).
pub fn list_ownership_locks(
    project_root: &Path,
    run_id: Option<&str>,
) -> GroveResult<Vec<crate::db::repositories::ownership_repo::OwnershipLockRow>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    if let Some(rid) = run_id {
        crate::db::repositories::ownership_repo::list_for_run(&conn, rid)
    } else {
        crate::db::repositories::ownership_repo::list_all(&conn)
    }
}

/// List all merge-queue entries for a conversation.
pub fn list_merge_queue(
    project_root: &Path,
    conversation_id: &str,
) -> GroveResult<Vec<crate::db::repositories::merge_queue_repo::MergeQueueRow>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::merge_queue_repo::list_for_conversation(&conn, conversation_id)
}

/// Return all events for a run (no tail limit).
///
/// Returns `Err` if the run ID does not exist so callers can distinguish
/// "no events yet" from "run not found".
pub fn run_events_all(project_root: &Path, run_id: &str) -> GroveResult<Vec<events::EventRecord>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    require_run_exists(&conn, run_id)?;
    events::list_for_run(&conn, run_id)
}

// ── Workspace & Project API ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectCreateRequest {
    OpenFolder {
        root_path: String,
        name: Option<String>,
    },
    CloneGitRepo {
        repo_url: String,
        target_path: String,
        name: Option<String>,
    },
    CreateRepo {
        provider: String,
        repo_name: String,
        target_path: String,
        owner: Option<String>,
        visibility: String,
        gitignore_template: Option<String>,
        #[serde(default)]
        gitignore_entries: Vec<String>,
        name: Option<String>,
    },
    ForkRepoToRemote {
        provider: String,
        source_path: String,
        target_path: String,
        repo_name: String,
        owner: Option<String>,
        visibility: String,
        remote_name: Option<String>,
        name: Option<String>,
    },
    ForkFolderToFolder {
        source_path: String,
        target_path: String,
        preserve_git: bool,
        name: Option<String>,
    },
    Ssh {
        host: String,
        remote_path: String,
        user: Option<String>,
        port: Option<u16>,
        name: Option<String>,
    },
}

fn project_name_override(name: Option<&str>) -> Option<String> {
    name.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn default_project_name(root_path: &str) -> String {
    Path::new(root_path)
        .file_name()
        .and_then(|segment| segment.to_str())
        .filter(|segment| !segment.is_empty())
        .unwrap_or("unnamed")
        .to_string()
}

fn ensure_local_directory_exists(path: &Path) -> GroveResult<PathBuf> {
    if !path.exists() {
        return Err(GroveError::Runtime(format!(
            "directory does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(GroveError::Runtime(format!(
            "path is not a directory: {}",
            path.display()
        )));
    }
    Ok(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
}

fn resolve_local_source_path(path: &str) -> GroveResult<PathBuf> {
    let input = PathBuf::from(path);
    let resolved = if input.is_absolute() {
        input
    } else {
        std::env::current_dir()?.join(input)
    };
    ensure_local_directory_exists(&resolved)
}

fn resolve_target_path(path: &str) -> GroveResult<PathBuf> {
    let input = PathBuf::from(path);
    Ok(if input.is_absolute() {
        input
    } else {
        std::env::current_dir()?.join(input)
    })
}

fn ensure_directory_available(path: &Path) -> GroveResult<()> {
    if path.exists() {
        if !path.is_dir() {
            return Err(GroveError::Runtime(format!(
                "path exists and is not a directory: {}",
                path.display()
            )));
        }
        let mut entries = fs::read_dir(path)?;
        if entries.next().transpose()?.is_some() {
            return Err(GroveError::Runtime(format!(
                "target directory must be empty: {}",
                path.display()
            )));
        }
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .env("PATH", crate::capability::shell_path())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn run_command(program: &str, args: &[&str], cwd: &Path) -> GroveResult<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("PATH", crate::capability::shell_path())
        .output()
        .map_err(|e| GroveError::Runtime(format!("failed to start {program}: {e}")))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() { stderr } else { stdout };
    Err(GroveError::Runtime(format!(
        "{program} {} failed: {detail}",
        args.join(" ")
    )))
}

fn normalize_repo_provider(provider: &str) -> GroveResult<String> {
    let normalized = provider.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "github" | "gh" => Ok("github".to_string()),
        "gitlab" | "gl" => Ok("gitlab".to_string()),
        "bitbucket" | "bb" => Ok("bitbucket".to_string()),
        _ => Err(GroveError::Runtime(format!(
            "unsupported repo provider '{provider}'. Use github, gitlab, or bitbucket."
        ))),
    }
}

fn infer_repo_provider_from_url(repo_url: &str) -> Option<String> {
    let normalized = repo_url.trim().to_ascii_lowercase();
    if normalized.contains("github.com") {
        Some("github".to_string())
    } else if normalized.contains("gitlab.com") {
        Some("gitlab".to_string())
    } else if normalized.contains("bitbucket.org") {
        Some("bitbucket".to_string())
    } else {
        None
    }
}

fn validate_repo_provider_create_supported(provider: &str) -> GroveResult<String> {
    let provider = normalize_repo_provider(provider)?;
    match provider.as_str() {
        "github" => {
            if !command_exists("gh") {
                return Err(GroveError::Runtime(
                    "gh is required to create GitHub repositories but is not installed".to_string(),
                ));
            }
        }
        "gitlab" => {
            if !command_exists("glab") {
                return Err(GroveError::Runtime(
                    "glab is required to create GitLab repositories but is not installed"
                        .to_string(),
                ));
            }
        }
        "bitbucket" => {
            return Err(GroveError::Runtime(
                "Bitbucket repo creation is not wired yet. Clone works today, but create/fork still needs provider tooling."
                    .to_string(),
            ));
        }
        _ => {}
    }
    Ok(provider)
}

fn render_gitignore(template: Option<&str>, extra_entries: &[String]) -> GroveResult<String> {
    let mut entries: Vec<String> = match template.map(|value| value.trim().to_ascii_lowercase()) {
        None => Vec::new(),
        Some(value) if value.is_empty() || value == "none" => Vec::new(),
        Some(value)
            if value == "node"
                || value == "nodejs"
                || value == "javascript"
                || value == "typescript" =>
        {
            vec![
                "node_modules/".to_string(),
                "dist/".to_string(),
                "build/".to_string(),
                "coverage/".to_string(),
                ".env".to_string(),
            ]
        }
        Some(value) if value == "python" => vec![
            "__pycache__/".to_string(),
            "*.py[cod]".to_string(),
            ".pytest_cache/".to_string(),
            ".mypy_cache/".to_string(),
            ".venv/".to_string(),
            "venv/".to_string(),
            ".env".to_string(),
        ],
        Some(value) if value == "rust" => vec!["target/".to_string(), "**/*.rs.bk".to_string()],
        Some(value) if value == "go" => vec!["bin/".to_string(), "coverage.out".to_string()],
        Some(value) if value == "java" => vec![
            "target/".to_string(),
            "*.class".to_string(),
            ".idea/".to_string(),
        ],
        Some(value) => {
            return Err(GroveError::Runtime(format!(
                "unsupported gitignore template '{value}'. Use one of: node, python, rust, go, java, none"
            )));
        }
    };

    entries.push(".grove/".to_string());
    for entry in extra_entries {
        let trimmed = entry.trim();
        if !trimmed.is_empty() {
            entries.push(trimmed.to_string());
        }
    }

    let mut deduped = Vec::new();
    for entry in entries {
        if !deduped.contains(&entry) {
            deduped.push(entry);
        }
    }
    Ok(format!("{}\n", deduped.join("\n")))
}

fn git_has_commits(project_root: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(project_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn git_remote_exists(project_root: &Path, remote_name: &str) -> bool {
    Command::new("git")
        .args(["remote", "get-url", remote_name])
        .current_dir(project_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn current_git_branch(project_root: &Path) -> String {
    run_command(
        "git",
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        project_root,
    )
    .ok()
    .filter(|value| !value.trim().is_empty())
    .unwrap_or_else(|| "main".to_string())
}

fn ensure_git_repository(project_root: &Path) -> GroveResult<()> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(project_root)
        .output()
        .map_err(|e| GroveError::Runtime(format!("failed to start git: {e}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(GroveError::Runtime(format!(
            "path is not a git repository: {}",
            project_root.display()
        )))
    }
}

fn prepare_remote_slot(project_root: &Path, remote_name: &str) -> GroveResult<()> {
    if !git_remote_exists(project_root, remote_name) {
        return Ok(());
    }
    if remote_name == "origin" && !git_remote_exists(project_root, "upstream") {
        run_command(
            "git",
            &["remote", "rename", "origin", "upstream"],
            project_root,
        )?;
        return Ok(());
    }
    Err(GroveError::Runtime(format!(
        "git remote '{remote_name}' already exists in {}. Choose a different remote name.",
        project_root.display()
    )))
}

fn create_symlink(target: &Path, link_path: &Path, is_dir: bool) -> GroveResult<()> {
    #[cfg(unix)]
    {
        let _ = is_dir;
        std::os::unix::fs::symlink(target, link_path).map_err(|e| {
            GroveError::Runtime(format!(
                "failed to create symlink {} -> {}: {e}",
                link_path.display(),
                target.display()
            ))
        })
    }
    #[cfg(windows)]
    {
        if is_dir {
            std::os::windows::fs::symlink_dir(target, link_path)
        } else {
            std::os::windows::fs::symlink_file(target, link_path)
        }
        .map_err(|e| {
            GroveError::Runtime(format!(
                "failed to create symlink {} -> {}: {e}",
                link_path.display(),
                target.display()
            ))
        })
    }
}

fn copy_directory_recursive(source: &Path, target: &Path, preserve_git: bool) -> GroveResult<()> {
    ensure_directory_available(target)?;
    fs::create_dir_all(target)?;

    let source_canonical = ensure_local_directory_exists(source)?;
    if let Some(parent) = target.parent() {
        let parent_canonical = parent
            .canonicalize()
            .unwrap_or_else(|_| parent.to_path_buf());
        let target_candidate =
            parent_canonical.join(target.file_name().ok_or_else(|| {
                GroveError::Runtime("target directory name is missing".to_string())
            })?);
        if target_candidate == source_canonical || target_candidate.starts_with(&source_canonical) {
            return Err(GroveError::Runtime(
                "target path must not be the same as or inside the source path".to_string(),
            ));
        }
    }

    fn copy_dir_inner(source: &Path, target: &Path, preserve_git: bool) -> GroveResult<()> {
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == ".grove" || (!preserve_git && name_str == ".git") {
                continue;
            }

            let source_path = entry.path();
            let target_path = target.join(&name);
            let file_type = entry.file_type()?;

            if file_type.is_dir() {
                fs::create_dir_all(&target_path)?;
                copy_dir_inner(&source_path, &target_path, preserve_git)?;
            } else if file_type.is_file() {
                fs::copy(&source_path, &target_path)?;
            } else if file_type.is_symlink() {
                let link_target = fs::read_link(&source_path)?;
                create_symlink(&link_target, &target_path, source_path.is_dir())?;
            }
        }
        Ok(())
    }

    copy_dir_inner(&source_canonical, target, preserve_git)?;
    ensure_local_directory_exists(target).map(|_| ())
}

fn create_provider_repo(
    provider: &str,
    repo_name: &str,
    owner: Option<&str>,
    visibility: &str,
    source_path: &Path,
    remote_name: &str,
) -> GroveResult<()> {
    let qualified_name = match owner {
        Some(owner) if !owner.trim().is_empty() => format!("{}/{}", owner.trim(), repo_name.trim()),
        _ => repo_name.trim().to_string(),
    };
    let source_path_str = source_path.to_string_lossy().to_string();

    match provider {
        "github" => {
            let visibility_flag = if visibility == "private" {
                "--private"
            } else {
                "--public"
            };
            run_command(
                "gh",
                &[
                    "repo",
                    "create",
                    &qualified_name,
                    visibility_flag,
                    "--source",
                    &source_path_str,
                    "--remote",
                    remote_name,
                ],
                source_path,
            )?;
        }
        "gitlab" => {
            if !command_exists("glab") {
                return Err(GroveError::Runtime(
                    "glab is required to create GitLab repositories but is not installed"
                        .to_string(),
                ));
            }
            let visibility_flag = if visibility == "private" {
                "--private"
            } else {
                "--public"
            };
            run_command(
                "glab",
                &[
                    "repo",
                    "create",
                    &qualified_name,
                    visibility_flag,
                    "--source",
                    &source_path_str,
                    "--remote-name",
                    remote_name,
                ],
                source_path,
            )?;
        }
        "bitbucket" => {
            return Err(GroveError::Runtime(
                "Bitbucket repo creation is not wired yet. Clone works today, but create/fork still needs provider tooling."
                    .to_string(),
            ));
        }
        _ => {
            return Err(GroveError::Runtime(format!(
                "unsupported repo provider '{provider}'"
            )));
        }
    }

    if git_has_commits(source_path) {
        let branch = current_git_branch(source_path);
        run_command("git", &["push", "-u", remote_name, &branch], source_path)?;
    }
    Ok(())
}

fn register_project_row(
    conn: &Connection,
    workspace_id: &str,
    root_path: &str,
    name: Option<&str>,
    source_kind: &str,
    source_details: Option<crate::db::repositories::projects_repo::ProjectSourceDetails>,
) -> GroveResult<crate::db::repositories::projects_repo::ProjectRow> {
    if let Some(existing) =
        crate::db::repositories::projects_repo::get_by_root_path(conn, root_path)?
    {
        if let Some(name) = project_name_override(name) {
            crate::db::repositories::projects_repo::update_name(conn, &existing.id, &name)?;
        }
        crate::db::repositories::projects_repo::update_source(
            conn,
            &existing.id,
            source_kind,
            source_details.as_ref(),
        )?;
        crate::db::repositories::projects_repo::set_state(conn, &existing.id, "active")?;
        return crate::db::repositories::projects_repo::get(conn, &existing.id);
    }

    let now = Utc::now().to_rfc3339();
    let row = crate::db::repositories::projects_repo::ProjectRow {
        id: conversation::derive_project_id(Path::new(root_path)),
        workspace_id: workspace_id.to_string(),
        name: Some(project_name_override(name).unwrap_or_else(|| default_project_name(root_path))),
        root_path: root_path.to_string(),
        state: "active".to_string(),
        created_at: now.clone(),
        updated_at: now,
        base_ref: None,
        source_kind: source_kind.to_string(),
        source_details,
    };
    crate::db::repositories::projects_repo::insert(conn, &row)?;
    crate::db::repositories::projects_repo::get(conn, &row.id)
}

#[allow(clippy::too_many_arguments)]
fn create_repo_project(
    provider: &str,
    repo_name: &str,
    target_path: &Path,
    owner: Option<&str>,
    visibility: &str,
    gitignore_template: Option<&str>,
    gitignore_entries: &[String],
    remote_name: &str,
) -> GroveResult<PathBuf> {
    if repo_name.trim().is_empty() {
        return Err(GroveError::Runtime("repo name is required".to_string()));
    }
    let provider = validate_repo_provider_create_supported(provider)?;
    match visibility {
        "public" | "private" => {}
        other => {
            return Err(GroveError::Runtime(format!(
                "unsupported visibility '{other}'. Use 'public' or 'private'."
            )));
        }
    }

    ensure_directory_available(target_path)?;
    fs::create_dir_all(target_path)?;

    let gitignore = render_gitignore(gitignore_template, gitignore_entries)?;
    fs::write(target_path.join(".gitignore"), gitignore)?;
    if !target_path.join("README.md").exists() {
        fs::write(target_path.join("README.md"), format!("# {repo_name}\n"))?;
    }

    run_command("git", &["init"], target_path)?;
    run_command("git", &["add", ".gitignore", "README.md"], target_path)?;
    run_command(
        "git",
        &[
            "-c",
            "user.name=Grove",
            "-c",
            "user.email=grove@local",
            "commit",
            "-m",
            "Initial commit",
        ],
        target_path,
    )?;
    run_command("git", &["branch", "-M", "main"], target_path)?;
    create_provider_repo(
        &provider,
        repo_name,
        owner,
        visibility,
        target_path,
        remote_name,
    )?;

    ensure_local_directory_exists(target_path)
}

fn fork_repo_project(
    provider: &str,
    source_path: &Path,
    target_path: &Path,
    repo_name: &str,
    owner: Option<&str>,
    visibility: &str,
    remote_name: &str,
) -> GroveResult<PathBuf> {
    ensure_git_repository(source_path)?;
    let provider = validate_repo_provider_create_supported(provider)?;
    copy_directory_recursive(source_path, target_path, true)?;
    prepare_remote_slot(target_path, remote_name)?;
    create_provider_repo(
        &provider,
        repo_name,
        owner,
        visibility,
        target_path,
        remote_name,
    )?;
    ensure_local_directory_exists(target_path)
}

pub fn get_workspace(
    project_root: &Path,
) -> GroveResult<crate::db::repositories::workspaces_repo::WorkspaceRow> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let workspace_id = workspace::ensure_workspace(&conn)?;
    crate::db::repositories::workspaces_repo::get(&conn, &workspace_id)
}

pub fn update_workspace_name(project_root: &Path, name: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let workspace_id = workspace::ensure_workspace(&conn)?;
    crate::db::repositories::workspaces_repo::update_name(&conn, &workspace_id, name)
}

pub fn archive_workspace(project_root: &Path, id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::workspaces_repo::set_state(&conn, id, "archived")
}

pub fn delete_workspace(project_root: &Path, id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::workspaces_repo::delete(&conn, id)
}

pub fn get_project(
    project_root: &Path,
) -> GroveResult<crate::db::repositories::projects_repo::ProjectRow> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let workspace_id = workspace::ensure_workspace(&conn)?;
    let project_id = workspace::ensure_project(&conn, project_root, &workspace_id)?;
    crate::db::repositories::projects_repo::get(&conn, &project_id)
}

pub fn list_projects(
    project_root: &Path,
) -> GroveResult<Vec<crate::db::repositories::projects_repo::ProjectRow>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let workspace_id = workspace::ensure_workspace(&conn)?;
    crate::db::repositories::projects_repo::list_for_workspace(&conn, &workspace_id, 100)
}

pub fn create_project_from_source(
    project_root: &Path,
    request: ProjectCreateRequest,
) -> GroveResult<crate::db::repositories::projects_repo::ProjectRow> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let workspace_id = workspace::ensure_workspace(&conn)?;

    match request {
        ProjectCreateRequest::OpenFolder { root_path, name } => {
            let canonical = ensure_local_directory_exists(Path::new(&root_path))?;
            register_project_row(
                &conn,
                &workspace_id,
                canonical.to_string_lossy().as_ref(),
                name.as_deref(),
                "local",
                None,
            )
        }
        ProjectCreateRequest::CloneGitRepo {
            repo_url,
            target_path,
            name,
        } => {
            let target = resolve_target_path(&target_path)?;
            ensure_directory_available(&target)?;
            let target_str = target.to_string_lossy().to_string();
            let clone_cwd = target
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."));
            run_command("git", &["clone", &repo_url, &target_str], &clone_cwd)?;
            let canonical = ensure_local_directory_exists(&target)?;
            register_project_row(
                &conn,
                &workspace_id,
                canonical.to_string_lossy().as_ref(),
                name.as_deref(),
                "git_clone",
                Some(
                    crate::db::repositories::projects_repo::ProjectSourceDetails {
                        repo_provider: infer_repo_provider_from_url(&repo_url),
                        repo_url: Some(repo_url),
                        ..Default::default()
                    },
                ),
            )
        }
        ProjectCreateRequest::CreateRepo {
            provider,
            repo_name,
            target_path,
            owner,
            visibility,
            gitignore_template,
            gitignore_entries,
            name,
        } => {
            let canonical = create_repo_project(
                &provider,
                &repo_name,
                &resolve_target_path(&target_path)?,
                owner.as_deref(),
                &visibility,
                gitignore_template.as_deref(),
                &gitignore_entries,
                "origin",
            )?;
            let repo_url = run_command("git", &["remote", "get-url", "origin"], &canonical).ok();
            let provider = normalize_repo_provider(&provider)?;
            register_project_row(
                &conn,
                &workspace_id,
                canonical.to_string_lossy().as_ref(),
                name.as_deref(),
                "repo_create",
                Some(
                    crate::db::repositories::projects_repo::ProjectSourceDetails {
                        repo_provider: Some(provider),
                        repo_url,
                        repo_visibility: Some(visibility),
                        remote_name: Some("origin".to_string()),
                        gitignore_template,
                        gitignore_entries,
                        ..Default::default()
                    },
                ),
            )
        }
        ProjectCreateRequest::ForkRepoToRemote {
            provider,
            source_path,
            target_path,
            repo_name,
            owner,
            visibility,
            remote_name,
            name,
        } => {
            let source = resolve_local_source_path(&source_path)?;
            let target = resolve_target_path(&target_path)?;
            let remote_name = remote_name
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("origin")
                .to_string();
            let canonical = fork_repo_project(
                &provider,
                &source,
                &target,
                &repo_name,
                owner.as_deref(),
                &visibility,
                &remote_name,
            )?;
            let repo_url =
                run_command("git", &["remote", "get-url", &remote_name], &canonical).ok();
            let provider = normalize_repo_provider(&provider)?;
            register_project_row(
                &conn,
                &workspace_id,
                canonical.to_string_lossy().as_ref(),
                name.as_deref(),
                "repo_fork",
                Some(
                    crate::db::repositories::projects_repo::ProjectSourceDetails {
                        repo_provider: Some(provider),
                        repo_url,
                        repo_visibility: Some(visibility),
                        remote_name: Some(remote_name),
                        source_path: Some(source.to_string_lossy().to_string()),
                        preserve_git: Some(true),
                        ..Default::default()
                    },
                ),
            )
        }
        ProjectCreateRequest::ForkFolderToFolder {
            source_path,
            target_path,
            preserve_git,
            name,
        } => {
            let source = resolve_local_source_path(&source_path)?;
            let target = resolve_target_path(&target_path)?;
            copy_directory_recursive(&source, &target, preserve_git)?;
            let canonical = ensure_local_directory_exists(&target)?;
            register_project_row(
                &conn,
                &workspace_id,
                canonical.to_string_lossy().as_ref(),
                name.as_deref(),
                "folder_fork",
                Some(
                    crate::db::repositories::projects_repo::ProjectSourceDetails {
                        source_path: Some(source.to_string_lossy().to_string()),
                        preserve_git: Some(preserve_git),
                        ..Default::default()
                    },
                ),
            )
        }
        ProjectCreateRequest::Ssh {
            host,
            remote_path,
            user,
            port,
            name,
        } => {
            if host.trim().is_empty() {
                return Err(GroveError::Runtime("SSH host is required".to_string()));
            }
            if remote_path.trim().is_empty() {
                return Err(GroveError::Runtime(
                    "SSH remote path is required".to_string(),
                ));
            }
            let authority = match (&user, port) {
                (Some(user), Some(port)) if !user.trim().is_empty() => {
                    format!("{}@{}:{port}", user.trim(), host.trim())
                }
                (Some(user), None) if !user.trim().is_empty() => {
                    format!("{}@{}", user.trim(), host.trim())
                }
                (None, Some(port)) => format!("{}:{port}", host.trim()),
                _ => host.trim().to_string(),
            };
            let synthetic_root = format!("ssh://{authority}{}", remote_path.trim());
            register_project_row(
                &conn,
                &workspace_id,
                &synthetic_root,
                name.as_deref(),
                "ssh",
                Some(
                    crate::db::repositories::projects_repo::ProjectSourceDetails {
                        ssh_host: Some(host.trim().to_string()),
                        ssh_user: user
                            .map(|value| value.trim().to_string())
                            .filter(|value| !value.is_empty()),
                        ssh_port: port,
                        ssh_remote_path: Some(remote_path.trim().to_string()),
                        ..Default::default()
                    },
                ),
            )
        }
    }
}

pub fn create_project(
    project_root: &Path,
    new_root_path: &str,
    name: Option<&str>,
) -> GroveResult<crate::db::repositories::projects_repo::ProjectRow> {
    create_project_from_source(
        project_root,
        ProjectCreateRequest::OpenFolder {
            root_path: new_root_path.to_string(),
            name: name.map(str::to_string),
        },
    )
}

pub fn project_supports_local_runs(
    project: &crate::db::repositories::projects_repo::ProjectRow,
) -> bool {
    project.source_kind != "ssh"
}

pub fn update_project_name(project_root: &Path, id: &str, name: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::projects_repo::update_name(&conn, id, name)
}

pub fn archive_project(project_root: &Path, id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::projects_repo::set_state(&conn, id, "archived")
}

pub fn delete_project(project_root: &Path, id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::projects_repo::delete(&conn, id)
}

pub fn get_project_settings(
    project_root: &Path,
    project_id: &str,
) -> GroveResult<crate::db::repositories::projects_repo::ProjectSettings> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::projects_repo::get_settings(&conn, project_id)
}

pub fn update_project_settings(
    project_root: &Path,
    project_id: &str,
    settings: &crate::db::repositories::projects_repo::ProjectSettings,
) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::projects_repo::update_settings(&conn, project_id, settings)
}

// ── Conversation API (CLI-facing) ─────────────────────────────────────────────

pub fn list_conversations(
    project_root: &Path,
    limit: i64,
) -> GroveResult<Vec<crate::db::repositories::conversations_repo::ConversationRow>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let project_id = conversation::derive_project_id(project_root);
    crate::db::repositories::conversations_repo::list_for_project(&conn, &project_id, limit)
}

pub fn get_conversation(
    project_root: &Path,
    id: &str,
) -> GroveResult<crate::db::repositories::conversations_repo::ConversationRow> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::conversations_repo::get(&conn, id)
}

pub fn archive_conversation(project_root: &Path, id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::conversations_repo::set_state(&conn, id, "archived")?;
    // Remove the conversation's persistent worktree. The conv branch and run
    // snapshot branches are preserved for history.
    if let Err(e) = worktree::conversation::remove_conversation_worktree(project_root, id) {
        tracing::warn!(conv_id = %id, error = %e, "failed to remove conversation worktree on archive");
    }
    Ok(())
}

pub fn delete_conversation(project_root: &Path, id: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::conversations_repo::delete(&conn, id)?;

    // Best-effort: delete the conversation branch if it exists.
    // This is cheap (branch is just a ref pointer) and keeps the repo clean.
    if worktree::git_ops::is_git_repo(project_root) {
        let branch = worktree::paths::conv_branch_name(id);
        if let Err(e) = worktree::git_ops::git_delete_branch(project_root, &branch) {
            tracing::debug!(branch = %branch, error = %e, "conv branch deletion skipped");
        }
    }
    Ok(())
}

fn latest_run_id_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<Option<String>> {
    conn.query_row(
        "SELECT id FROM runs WHERE conversation_id=?1 ORDER BY created_at DESC LIMIT 1",
        [conversation_id],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn run_process_with_timeout(cmd: &mut Command, label: &str) -> GroveResult<Output> {
    cmd.env("PATH", crate::capability::shell_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| GroveError::Runtime(format!("{label} failed to start: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(MERGE_PUBLISH_COMMAND_TIMEOUT_SECS);

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|e| GroveError::Runtime(format!("{label} output read failed: {e}")));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GroveError::Runtime(format!(
                        "{label} timed out after {}s",
                        MERGE_PUBLISH_COMMAND_TIMEOUT_SECS
                    )));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GroveError::Runtime(format!("{label} wait failed: {e}")));
            }
        }
    }
}

/// Merge `origin/{default_branch}` INTO the conversation branch currently
/// checked out in `conv_worktree_path`. The merge happens directly in the
/// conversation worktree — no temp worktree needed (unlike rebase).
pub(crate) fn merge_main_into_conv_branch(
    conv_worktree_path: &Path,
    default_branch: &str,
) -> GroveResult<worktree::git_ops::MergeUpstreamOutcome> {
    let upstream = format!("origin/{}", default_branch);
    let msg = format!("grove: merge {} into conversation branch", default_branch);
    worktree::git_ops::git_merge_upstream_into(conv_worktree_path, &upstream, &msg)
}

pub(crate) fn rebase_conv_branch(
    project_root: &Path,
    conv_branch: &str,
    upstream: &str,
) -> GroveResult<worktree::git_ops::RebaseOutcome> {
    let ahead =
        worktree::git_ops::git_log_oneline(project_root, &format!("{upstream}..{conv_branch}"))?;
    if ahead.is_empty() {
        let upstream_sha = worktree::git_ops::git_rev_parse(project_root, upstream)?;
        worktree::git_ops::git_update_ref(
            project_root,
            &format!("refs/heads/{conv_branch}"),
            &upstream_sha,
        )?;
        return Ok(worktree::git_ops::RebaseOutcome::Success);
    }

    let temp_id = format!("rebase_{}", uuid::Uuid::new_v4().simple());
    let worktrees_base = crate::config::grove_dir(project_root).join("worktrees");
    std::fs::create_dir_all(&worktrees_base)?;
    let temp_path = worktrees_base.join(&temp_id);

    worktree::git_ops::git_worktree_add_detached_at(project_root, &temp_path, conv_branch)?;
    let rebase_result = worktree::git_ops::git_rebase(&temp_path, upstream);
    let rebased_head = if matches!(rebase_result, Ok(worktree::git_ops::RebaseOutcome::Success)) {
        Some(worktree::git_ops::git_rev_parse_head(&temp_path)?)
    } else {
        None
    };

    let _ = worktree::git_ops::git_worktree_remove(project_root, &temp_path);
    if temp_path.exists() {
        let _ = std::fs::remove_dir_all(&temp_path);
    }
    let _ = worktree::git_ops::git_worktree_prune(project_root);

    if let Some(rebased_head) = rebased_head {
        worktree::git_ops::git_update_ref(
            project_root,
            &format!("refs/heads/{conv_branch}"),
            &rebased_head,
        )?;
    }

    rebase_result
}

/// Rebase a conversation's branch onto the project's default branch (e.g. `main`).
///
/// The rebase is performed inside a temporary linked worktree so the project
/// root is never checked out to the conversation branch. If any commit in the
/// rebase conflicts, the rebase is aborted and the branch is left unchanged.
///
/// Returns a human-readable success message, or `Err` on conflict or git error.
pub fn rebase_conversation(project_root: &Path, conversation_id: &str) -> GroveResult<String> {
    let cfg = GroveConfig::load_or_create(project_root)?;

    if !worktree::git_ops::is_git_repo(project_root)
        || !worktree::git_ops::has_commits(project_root)
    {
        return Err(GroveError::Runtime(
            "rebase requires a git repository with at least one commit".into(),
        ));
    }

    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;

    // Ensure the conversation exists.
    let _conv = crate::db::repositories::conversations_repo::get(&conn, conversation_id)?;

    // Reject if a run is currently active for this conversation.
    let active: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM runs r
         JOIN conversations c ON r.conversation_id = c.id
         WHERE c.id = ?1
           AND r.state IN ('executing', 'waiting_for_gate', 'planning', 'verifying', 'publishing', 'merging')",
            [conversation_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if active > 0 {
        return Err(GroveError::Runtime(format!(
            "cannot rebase conversation {conversation_id}: a run is currently active"
        )));
    }

    let conv_branch =
        worktree::paths::conv_branch_name_p(&cfg.worktree.branch_prefix, conversation_id);
    let upstream = &cfg.project.default_branch;

    // Quick up-to-date check — avoid creating a worktree unnecessarily.
    if worktree::git_ops::git_detect_stale_base(project_root, &conv_branch, upstream).is_none() {
        return Ok(format!(
            "Conversation branch {conv_branch} is already up-to-date with {upstream}."
        ));
    }

    match rebase_conv_branch(project_root, &conv_branch, upstream)? {
        worktree::git_ops::RebaseOutcome::Success => {
            if let Some(event_run_id) = latest_run_id_for_conversation(&conn, conversation_id)? {
                let _ = events::emit(
                    &conn,
                    &event_run_id,
                    None,
                    crate::events::event_types::CONV_REBASED,
                    json!({
                        "conversation_id": conversation_id,
                        "source_branch": conv_branch,
                        "target_branch": upstream,
                    }),
                );
            }
            Ok(format!(
                "Conversation branch {conv_branch} successfully rebased onto {upstream}."
            ))
        }
        worktree::git_ops::RebaseOutcome::Conflict { conflicting_files } => {
            let file_count = conflicting_files.len();
            let files = conflicting_files.join(", ");
            Err(GroveError::MergeConflict { files, file_count })
        }
    }
}

/// Result of merging a conversation branch into the project's target branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConversationResult {
    pub conversation_id: String,
    pub source_branch: String,
    pub target_branch: String,
    pub strategy: String,
    pub outcome: String,
    pub pr_url: Option<String>,
    pub conflicting_files: Vec<String>,
}

/// Merge a conversation's branch into the project's default branch.
///
/// **Direct** (default): performs `git merge --no-ff` locally.
/// **Github**: pushes the conversation branch and opens a PR via `gh`.
///
/// Returns `Err` if the conversation has no branch, has an active run,
/// or the merge encounters an unrecoverable error.
pub fn merge_conversation(
    project_root: &Path,
    conversation_id: &str,
) -> GroveResult<MergeConversationResult> {
    let cfg = GroveConfig::load_or_create(project_root)?;
    let handle = DbHandle::new(project_root);
    let mut conn = handle.connect()?;

    // 1. Load conversation — verify it exists and is active.
    let conv = crate::db::repositories::conversations_repo::get(&conn, conversation_id)?;
    if conv.state != "active" {
        return Err(GroveError::Runtime(format!(
            "conversation {conversation_id} is not active (state={})",
            conv.state
        )));
    }

    let source_branch = conv.branch_name.ok_or_else(|| {
        GroveError::Runtime(format!(
            "conversation {conversation_id} has no branch — run at least once first"
        ))
    })?;

    // 2. Verify no active run on this conversation.
    let active_run: Option<String> = conn
        .query_row(
            "SELECT id FROM runs WHERE conversation_id=?1 AND state NOT IN ('completed','failed','aborted','cancelled')",
            [conversation_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(run_id) = active_run {
        return Err(GroveError::Runtime(format!(
            "conversation {conversation_id} has active run {run_id} — wait for it to complete"
        )));
    }

    // 3. Check project is a git repo with commits.
    if !worktree::git_ops::is_git_repo(project_root)
        || !worktree::git_ops::has_commits(project_root)
    {
        return Err(GroveError::Runtime(
            "project is not a git repo or has no commits".to_string(),
        ));
    }

    let target_branch = worktree::git_ops::detect_default_branch(project_root)?;

    // 4. Check if the branch has diverged (any commits ahead of target).
    let ahead = worktree::git_ops::git_log_oneline(
        project_root,
        &format!("{target_branch}..{source_branch}"),
    )?;
    if ahead.is_empty() {
        return Ok(MergeConversationResult {
            conversation_id: conversation_id.to_string(),
            source_branch,
            target_branch,
            strategy: format!("{:?}", cfg.merge.target).to_lowercase(),
            outcome: "up_to_date".to_string(),
            pr_url: None,
            conflicting_files: vec![],
        });
    }

    let strategy_name = format!("{:?}", cfg.merge.target).to_lowercase();

    // 5. Record in merge_queue for audit trail.
    let queue_id = crate::merge::queue::enqueue(
        &mut conn,
        conversation_id,
        &source_branch,
        &target_branch,
        &strategy_name,
    )?;

    // 6. Execute based on strategy.
    match cfg.merge.target {
        crate::config::MergeTarget::Direct => {
            let outcome = crate::merge::executor::execute(
                project_root,
                &source_branch,
                &target_branch,
                conversation_id,
                conversation_id,
                &cfg.hooks,
            )?;

            match outcome {
                crate::merge::executor::MergeOutcome::Success { .. } => {
                    crate::merge::queue::mark_done(&conn, queue_id)?;
                    if let Some(event_run_id) =
                        latest_run_id_for_conversation(&conn, conversation_id)?
                    {
                        let _ = events::emit(
                            &conn,
                            &event_run_id,
                            None,
                            crate::events::event_types::CONV_MERGED,
                            json!({
                                "conversation_id": conversation_id,
                                "source_branch": &source_branch,
                                "target_branch": &target_branch,
                                "strategy": &strategy_name,
                            }),
                        );
                    }
                    Ok(MergeConversationResult {
                        conversation_id: conversation_id.to_string(),
                        source_branch,
                        target_branch,
                        strategy: strategy_name,
                        outcome: "merged".to_string(),
                        pr_url: None,
                        conflicting_files: vec![],
                    })
                }
                crate::merge::executor::MergeOutcome::Conflict { files } => {
                    crate::merge::queue::mark_conflict(&conn, queue_id, &files)?;
                    Ok(MergeConversationResult {
                        conversation_id: conversation_id.to_string(),
                        source_branch,
                        target_branch,
                        strategy: strategy_name,
                        outcome: "conflict".to_string(),
                        pr_url: None,
                        conflicting_files: files,
                    })
                }
            }
        }
        crate::config::MergeTarget::Github => {
            // Push the conversation branch and open a PR.
            let push_out = run_process_with_timeout(
                std::process::Command::new("git")
                    .args(["push", "-u", "origin", &source_branch])
                    .current_dir(project_root),
                "git push",
            )?;
            if !push_out.status.success() {
                let stderr = String::from_utf8_lossy(&push_out.stderr).to_string();
                crate::merge::queue::mark_failed(&conn, queue_id, &stderr)?;
                return Err(GroveError::Runtime(format!("git push failed: {stderr}")));
            }

            let pr_out = run_process_with_timeout(
                std::process::Command::new("gh")
                    .args([
                        "pr", "create",
                        "--base", &target_branch,
                        "--head", &source_branch,
                        "--title", &format!("grove: conversation {conversation_id}"),
                        "--body", &format!("Automated PR from Grove conversation `{conversation_id}`.\n\n{} commit(s) ahead of `{target_branch}`.", ahead.len()),
                    ])
                    .current_dir(project_root),
                "gh pr create",
            )?;

            if pr_out.status.success() {
                let pr_url = String::from_utf8_lossy(&pr_out.stdout).trim().to_string();
                crate::merge::queue::set_pr_url(&conn, queue_id, &pr_url)?;
                crate::merge::queue::mark_done(&conn, queue_id)?;
                if let Some(event_run_id) = latest_run_id_for_conversation(&conn, conversation_id)?
                {
                    let _ = events::emit(
                        &conn,
                        &event_run_id,
                        None,
                        crate::events::event_types::CONV_MERGED,
                        json!({
                            "conversation_id": conversation_id,
                            "source_branch": &source_branch,
                            "target_branch": &target_branch,
                            "strategy": &strategy_name,
                            "pr_url": &pr_url,
                        }),
                    );
                }
                Ok(MergeConversationResult {
                    conversation_id: conversation_id.to_string(),
                    source_branch,
                    target_branch,
                    strategy: strategy_name,
                    outcome: "pr_opened".to_string(),
                    pr_url: Some(pr_url),
                    conflicting_files: vec![],
                })
            } else {
                let stderr = String::from_utf8_lossy(&pr_out.stderr).to_string();
                // PR may already exist — treat as success if the branch was pushed.
                if stderr.contains("already exists") {
                    crate::merge::queue::mark_done(&conn, queue_id)?;
                    if let Some(event_run_id) =
                        latest_run_id_for_conversation(&conn, conversation_id)?
                    {
                        let _ = events::emit(
                            &conn,
                            &event_run_id,
                            None,
                            crate::events::event_types::CONV_MERGED,
                            json!({
                                "conversation_id": conversation_id,
                                "source_branch": &source_branch,
                                "target_branch": &target_branch,
                                "strategy": &strategy_name,
                            }),
                        );
                    }
                    Ok(MergeConversationResult {
                        conversation_id: conversation_id.to_string(),
                        source_branch,
                        target_branch,
                        strategy: strategy_name,
                        outcome: "pr_exists".to_string(),
                        pr_url: None,
                        conflicting_files: vec![],
                    })
                } else {
                    crate::merge::queue::mark_failed(&conn, queue_id, &stderr)?;
                    Err(GroveError::Runtime(format!(
                        "gh pr create failed: {stderr}"
                    )))
                }
            }
        }
    }
}

pub fn update_conversation_title(project_root: &Path, id: &str, title: &str) -> GroveResult<()> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    crate::db::repositories::conversations_repo::update_title(&conn, id, title)
}

pub fn credit_balance(project_root: &Path) -> GroveResult<f64> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let workspace_id = workspace::ensure_workspace(&conn)?;
    crate::db::repositories::workspaces_repo::credit_balance(&conn, &workspace_id)
}

pub fn add_credits(project_root: &Path, amount_usd: f64) -> GroveResult<f64> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let workspace_id = workspace::ensure_workspace(&conn)?;
    crate::db::repositories::workspaces_repo::add_credits(&conn, &workspace_id, amount_usd)
}

pub fn list_conversation_messages(
    project_root: &Path,
    id: &str,
    limit: i64,
) -> GroveResult<Vec<crate::db::repositories::messages_repo::MessageRow>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    // Verify conversation exists
    crate::db::repositories::conversations_repo::get(&conn, id)?;
    crate::db::repositories::messages_repo::list_for_conversation(&conn, id, limit)
}

pub fn list_run_messages(
    project_root: &Path,
    run_id: &str,
) -> GroveResult<Vec<crate::db::repositories::messages_repo::MessageRow>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    require_run_exists(&conn, run_id)?;
    crate::db::repositories::messages_repo::list_for_run(&conn, run_id)
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Return `Err` if `run_id` is not present in the `runs` table.
fn require_run_exists(conn: &Connection, run_id: &str) -> GroveResult<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM runs WHERE id=?1", [run_id], |r| {
        r.get(0)
    })?;
    if count == 0 {
        return Err(GroveError::Runtime(format!("run '{run_id}' not found")));
    }
    Ok(())
}

/// Atomically check concurrency constraints and insert a new run, all inside
/// Acquire a run slot, waiting up to `lock_wait_timeout_secs` for one to open.
///
/// Uses a `BEGIN IMMEDIATE` transaction to prevent TOCTOU races.
///
/// Enforces two invariants:
/// 1. At most 1 active run per conversation.
/// 2. At most `max_concurrent_runs` conversations with active runs globally.
///
/// When `lock_wait_timeout_secs > 0` and the slot is busy, polls with
/// exponential backoff (1 s → 2 s → 4 s … capped at 30 s) and logs progress
/// messages until the slot is acquired or the timeout is reached.
#[allow(clippy::too_many_arguments)]
fn acquire_run_slot(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    budget_usd: f64,
    conversation_id: &str,
    max_concurrent_runs: u16,
    lock_wait_timeout_secs: u64,
    disable_phase_gates: bool,
    provider: Option<&str>,
    model: Option<&str>,
) -> GroveResult<()> {
    let deadline = if lock_wait_timeout_secs > 0 {
        Some(std::time::Instant::now() + std::time::Duration::from_secs(lock_wait_timeout_secs))
    } else {
        None
    };
    let mut backoff_secs: u64 = 1;
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;
        let result = try_acquire_run_slot(
            conn,
            run_id,
            objective,
            budget_usd,
            conversation_id,
            max_concurrent_runs,
            disable_phase_gates,
            provider,
            model,
        );

        match result {
            Ok(()) => return Ok(()),
            Err(GroveError::Runtime(ref msg)) if is_slot_busy_error(msg) => {
                // Slot is currently held by another run.
                let Some(ref deadline) = deadline else {
                    // Timeout disabled — fail immediately.
                    return result;
                };
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                if remaining.is_zero() {
                    return Err(GroveError::Runtime(format!(
                        "timed out after {lock_wait_timeout_secs}s waiting for a run slot. \
                         {msg}"
                    )));
                }
                let sleep_secs = backoff_secs.min(30).min(remaining.as_secs().max(1));
                let elapsed = lock_wait_timeout_secs.saturating_sub(remaining.as_secs());
                tracing::info!(
                    attempt = attempt,
                    elapsed_secs = elapsed,
                    retry_in_secs = sleep_secs,
                    "Waiting for run slot… ({elapsed}s elapsed, retrying in {sleep_secs}s)"
                );
                std::thread::sleep(std::time::Duration::from_secs(sleep_secs));
                backoff_secs = (backoff_secs * 2).min(30);
            }
            Err(e) => return Err(e),
        }
    }
}

/// Returns `true` if the error string indicates a slot-busy condition (as
/// opposed to a DB error, which should not be retried).
fn is_slot_busy_error(msg: &str) -> bool {
    msg.contains("already in progress on this conversation")
        || msg.contains("concurrency limit reached")
}

/// Single attempt to acquire a run slot — no polling.
#[allow(clippy::too_many_arguments)]
fn try_acquire_run_slot(
    conn: &mut Connection,
    run_id: &str,
    objective: &str,
    budget_usd: f64,
    conversation_id: &str,
    max_concurrent_runs: u16,
    disable_phase_gates: bool,
    provider: Option<&str>,
    model: Option<&str>,
) -> GroveResult<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // Check 1: no other active run on this conversation.
    let active: Option<String> = tx
        .query_row(
            "SELECT id FROM runs WHERE conversation_id = ?1 \
         AND state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging') LIMIT 1",
            params![conversation_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(active_id) = active {
        tx.rollback().ok();
        return Err(GroveError::Runtime(format!(
            "run '{active_id}' is already in progress on this conversation. \
             Wait for it to complete, abort it, or start a new conversation for parallel execution."
        )));
    }

    // Check 2: global concurrency cap across all conversations.
    let active_count: i64 = tx.query_row(
        "SELECT COUNT(DISTINCT conversation_id) FROM runs \
         WHERE state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging') \
         AND conversation_id IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    if active_count >= max_concurrent_runs as i64 {
        tx.rollback().ok();
        return Err(GroveError::Runtime(format!(
            "concurrency limit reached: {active_count} conversation(s) already have active runs \
             (max_concurrent_runs = {max_concurrent_runs}). \
             Wait for a run to finish or increase runtime.max_concurrent_runs in grove.yml."
        )));
    }

    // All checks passed — insert the run with conversation_id already set.
    let now = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, publish_status, \
         conversation_id, disable_phase_gates, provider, model, created_at, updated_at)
         VALUES (?1, ?2, 'created', ?3, 0, 'pending_retry', ?4, ?5, ?6, ?7, ?8, ?8)",
        params![
            run_id,
            objective,
            budget_usd,
            conversation_id,
            disable_phase_gates,
            provider,
            model,
            now
        ],
    )?;

    tx.commit()?;
    Ok(())
}

fn new_run_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// Recover runs stuck in non-terminal state from a previous crash.
///
/// A run is considered crashed if it's in an active state (executing,
/// waiting_for_gate, planning, verifying, publishing, merging) but was last
/// updated more than 5 minutes ago (indicating
/// the process that owned it is no longer running). These runs are marked as
/// `failed` so their resources can be cleaned up by the subsequent sweep.
///
/// Returns the number of runs recovered.
fn recover_crashed_runs(conn: &mut rusqlite::Connection) -> usize {
    let now = Utc::now().to_rfc3339();
    // Mark runs that have been in an active state for > 5 minutes as failed.
    // The 5-minute threshold avoids false positives for slow-starting runs.
    let result = conn.execute(
        "UPDATE runs SET state = 'failed', updated_at = ?1
         WHERE state IN ('executing', 'waiting_for_gate', 'planning', 'verifying', 'publishing', 'merging')
           AND updated_at < datetime('now', '-5 minutes')",
        [&now],
    );
    match result {
        Ok(n) => {
            // Also mark any active sessions belonging to those failed runs.
            if n > 0 {
                let _ = conn.execute(
                    "UPDATE sessions SET state = 'failed', updated_at = ?1
                     WHERE state NOT IN ('completed', 'failed', 'aborted')
                       AND run_id IN (
                           SELECT id FROM runs WHERE state = 'failed' AND updated_at = ?1
                       )",
                    [&now],
                );
            }
            n
        }
        Err(e) => {
            tracing::warn!(error = %e, "crash recovery query failed");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::conversations_repo::ConversationRow;
    use crate::db::repositories::projects_repo::ProjectRow;
    use std::process::Command;

    fn git_ok(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn git_stdout(cwd: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn create_project_from_open_folder_registers_local_metadata() {
        let workspace = tempfile::TempDir::new().unwrap();
        crate::db::initialize(workspace.path()).unwrap();

        let project_dir = tempfile::TempDir::new().unwrap();
        let row = create_project_from_source(
            workspace.path(),
            ProjectCreateRequest::OpenFolder {
                root_path: project_dir.path().to_string_lossy().to_string(),
                name: Some("Sample App".to_string()),
            },
        )
        .unwrap();

        assert_eq!(row.name.as_deref(), Some("Sample App"));
        assert_eq!(row.source_kind, "local");
        assert!(row.source_details.is_none());
    }

    #[test]
    fn create_project_from_ssh_registers_remote_metadata() {
        let workspace = tempfile::TempDir::new().unwrap();
        crate::db::initialize(workspace.path()).unwrap();

        let row = create_project_from_source(
            workspace.path(),
            ProjectCreateRequest::Ssh {
                host: "devbox.example.com".to_string(),
                remote_path: "/srv/api".to_string(),
                user: Some("farooq".to_string()),
                port: Some(2222),
                name: Some("API Box".to_string()),
            },
        )
        .unwrap();

        assert_eq!(row.name.as_deref(), Some("API Box"));
        assert_eq!(row.source_kind, "ssh");
        assert!(!project_supports_local_runs(&row));
        let details = row.source_details.expect("source details");
        assert_eq!(details.ssh_host.as_deref(), Some("devbox.example.com"));
        assert_eq!(details.ssh_user.as_deref(), Some("farooq"));
        assert_eq!(details.ssh_port, Some(2222));
        assert_eq!(details.ssh_remote_path.as_deref(), Some("/srv/api"));
    }

    #[test]
    fn create_project_from_folder_fork_copies_files_and_metadata() {
        let workspace = tempfile::TempDir::new().unwrap();
        crate::db::initialize(workspace.path()).unwrap();

        let source_dir = tempfile::TempDir::new().unwrap();
        fs::write(source_dir.path().join("README.md"), "# Sample\n").unwrap();
        fs::create_dir_all(source_dir.path().join(".grove")).unwrap();
        fs::write(source_dir.path().join(".grove").join("cache"), "skip").unwrap();

        let target_path = workspace.path().join("forked-folder");
        let row = create_project_from_source(
            workspace.path(),
            ProjectCreateRequest::ForkFolderToFolder {
                source_path: source_dir.path().to_string_lossy().to_string(),
                target_path: target_path.to_string_lossy().to_string(),
                preserve_git: false,
                name: Some("Forked Folder".to_string()),
            },
        )
        .unwrap();

        assert_eq!(row.name.as_deref(), Some("Forked Folder"));
        assert_eq!(row.source_kind, "folder_fork");
        assert!(target_path.join("README.md").exists());
        assert!(!target_path.join(".grove").exists());
        let details = row.source_details.expect("source details");
        let expected_source = source_dir.path().canonicalize().unwrap();
        assert_eq!(
            details.source_path.as_deref(),
            Some(expected_source.to_string_lossy().as_ref())
        );
        assert_eq!(details.preserve_git, Some(false));
    }

    #[test]
    fn derive_run_publish_statuses_marks_remote_branch_ancestor_as_published() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init", "-b", "main"]);
        git_ok(repo.path(), &["config", "user.email", "test@grove.local"]);
        git_ok(repo.path(), &["config", "user.name", "Grove Test"]);
        std::fs::write(repo.path().join("README.md"), "base\n").unwrap();
        git_ok(repo.path(), &["add", "README.md"]);
        git_ok(repo.path(), &["commit", "-m", "base"]);

        let remote = tempfile::TempDir::new().unwrap();
        git_ok(remote.path(), &["init", "--bare"]);
        git_ok(
            repo.path(),
            &["remote", "add", "origin", remote.path().to_str().unwrap()],
        );
        git_ok(repo.path(), &["checkout", "-b", "grove/s_conv-publish"]);
        std::fs::write(repo.path().join("feature.txt"), "published\n").unwrap();
        git_ok(repo.path(), &["add", "feature.txt"]);
        git_ok(
            repo.path(),
            &[
                "commit",
                "-m",
                "grove: builder [run: run_publish_1234, session: sess_test_1234]",
            ],
        );
        let final_commit_sha = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
        git_ok(repo.path(), &["push", "-u", "origin", "HEAD"]);

        crate::db::initialize(repo.path()).unwrap();
        let mut conn = crate::db::DbHandle::new(repo.path()).connect().unwrap();
        let workspace_id = workspace::ensure_workspace(&conn).unwrap();
        crate::db::repositories::projects_repo::insert(
            &mut conn,
            &ProjectRow {
                id: "proj_publish".to_string(),
                workspace_id,
                name: Some("Publish Test".to_string()),
                root_path: repo.path().to_string_lossy().to_string(),
                state: "active".to_string(),
                created_at: "2024-01-01T00:00:00Z".to_string(),
                updated_at: "2024-01-01T00:00:00Z".to_string(),
                base_ref: None,
                source_kind: "local".to_string(),
                source_details: None,
            },
        )
        .unwrap();
        crate::db::repositories::conversations_repo::insert(
            &mut conn,
            &ConversationRow {
                id: "conv_publish".to_string(),
                project_id: "proj_publish".to_string(),
                title: Some("Publish Conversation".to_string()),
                state: "active".to_string(),
                conversation_kind: crate::orchestrator::conversation::RUN_CONVERSATION_KIND
                    .to_string(),
                cli_provider: None,
                cli_model: None,
                branch_name: Some("grove/s_conv-publish".to_string()),
                remote_branch_name: Some("origin/grove/s_conv-publish".to_string()),
                remote_registration_state: "registered".to_string(),
                remote_registration_error: None,
                remote_registered_at: Some("2024-01-01T00:00:00Z".to_string()),
                worktree_path: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
                updated_at: "2024-01-01T00:00:00Z".to_string(),
                workspace_id: None,
                user_id: None,
            },
        )
        .unwrap();

        let mut runs = vec![RunRecord {
            id: "run_publish_1234".to_string(),
            objective: "test".to_string(),
            state: "completed".to_string(),
            budget_usd: 1.0,
            cost_used_usd: 0.0,
            publish_status: "pending_retry".to_string(),
            publish_error: None,
            final_commit_sha: Some(final_commit_sha),
            pr_url: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            conversation_id: Some("conv_publish".to_string()),
            pipeline: None,
            current_agent: None,
        }];

        derive_run_publish_statuses(repo.path(), &conn, &mut runs).unwrap();

        assert_eq!(runs[0].publish_status, "published");
    }

    #[test]
    fn task_terminal_state_maps_failed_and_paused_runs_correctly() {
        assert_eq!(task_terminal_state("failed"), "failed");
        assert_eq!(task_terminal_state("paused"), "cancelled");
        assert_eq!(task_terminal_state("completed"), "completed");
    }

    #[test]
    fn parse_permission_mode_accepts_known_values() {
        assert_eq!(
            parse_permission_mode(Some("skip_all")),
            Some(PermissionMode::SkipAll)
        );
        assert_eq!(
            parse_permission_mode(Some("human_gate")),
            Some(PermissionMode::HumanGate)
        );
        assert_eq!(
            parse_permission_mode(Some("autonomous_gate")),
            Some(PermissionMode::AutonomousGate)
        );
        assert_eq!(parse_permission_mode(Some("unknown")), None);
        assert_eq!(parse_permission_mode(None), None);
    }

    #[test]
    fn effective_pause_after_merges_pipeline_gates_by_default() {
        let pause_after = effective_pause_after(
            &[crate::agents::AgentType::Reviewer],
            &[
                crate::agents::AgentType::BuildPrd,
                crate::agents::AgentType::Reviewer,
            ],
            false,
        );
        assert_eq!(
            pause_after,
            vec![
                crate::agents::AgentType::Reviewer,
                crate::agents::AgentType::BuildPrd,
            ]
        );
    }

    #[test]
    fn effective_pause_after_skips_pipeline_gates_when_disabled() {
        let pause_after = effective_pause_after(
            &[crate::agents::AgentType::Reviewer],
            &[crate::agents::AgentType::BuildPrd],
            true,
        );
        assert_eq!(pause_after, vec![crate::agents::AgentType::Reviewer]);
    }
}
