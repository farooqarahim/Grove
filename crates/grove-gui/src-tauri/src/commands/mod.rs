pub mod automation;
pub mod config;
pub mod conversation;
pub mod git;
pub mod graph;
pub mod issue;
pub mod project;
pub mod run;
pub mod streaming;

pub use automation::*;
pub use config::*;
pub use conversation::*;
pub use git::*;
pub use graph::*;
pub use issue::*;
pub use project::*;
pub use run::*;
pub use streaming::*;

// ── Shared imports used by multiple domain modules ───────────────────────────

use tauri::{Emitter as _, Manager};

use grove_core::db::repositories::projects_repo::ProjectRow;

use crate::state::AppState;

// ── Event emission ────────────────────────────────────────────────────────────

/// Emit a lightweight "data changed" event to the frontend so TanStack Query
/// can invalidate and refetch immediately — no polling lag.
///
/// Errors are intentionally swallowed (log only): a missed push degrades to
/// the polling fallback already in place on the frontend.
pub(crate) fn emit(handle: &tauri::AppHandle, event: &str, payload: serde_json::Value) {
    if let Err(e) = handle.emit(event, payload) {
        tracing::warn!(event, error = %e, "failed to emit Tauri event");
    }
}

// ── Queue drain helpers ───────────────────────────────────────────────────────

/// Resolve the project root for a queued task by looking up the task's
/// conversation → project. Falls back to `workspace_root` if unresolvable.
pub(crate) use grove_core::orchestrator::resolve_project_root_for_task;

pub(crate) fn ensure_project_supports_local_runs(project: &ProjectRow) -> Result<(), String> {
    if grove_core::orchestrator::project_supports_local_runs(project) {
        return Ok(());
    }
    Err("SSH projects currently support shell access only. Agent runs still require a local checkout.".to_string())
}

pub(crate) fn is_internal_workspace_project(project: &ProjectRow) -> bool {
    project.root_path.contains("/.grove/workspaces/")
}

pub(crate) fn ensure_project_is_valid_run_target(project: &ProjectRow) -> Result<(), String> {
    if is_internal_workspace_project(project) {
        return Err(
            "Select a real project before starting a run. The internal Grove workspace cannot be used as a run target."
                .to_string(),
        );
    }
    ensure_project_supports_local_runs(project)
}

pub(crate) fn project_root_for_id(
    state: &AppState,
    project_id: &str,
) -> Result<ProjectRow, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::projects_repo::get(&conn, project_id).map_err(|e| e.to_string())
}

/// Delegate to the new pty::launch module for CLI command resolution.
pub(crate) fn resolve_cli_launch_command(
    project_root: &std::path::Path,
    provider_id: &str,
    model: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    crate::pty::launch::resolve_cli_command(project_root, provider_id, model)
}

/// Look up the issue linked to a run and apply project-level workflow transitions.
/// Called after run completion or failure in a background thread.
pub(crate) fn trigger_workflow_writeback(
    workspace_root: &std::path::Path,
    cfg: &grove_core::config::GroveConfig,
    run_id: &str,
    conversation_id: Option<&str>,
    succeeded: bool,
    error_msg: Option<&str>,
) {
    use grove_core::db::DbHandle;
    use grove_core::tracker::write_back::{
        WriteBackContext, on_run_completed_project, on_run_failed_project,
    };

    // Resolve project_id from conversation
    let project_id: Option<String> = conversation_id.and_then(|conv_id| {
        let handle = DbHandle::new(workspace_root);
        let conn = handle.connect().ok()?;
        conn.query_row(
            "SELECT project_id FROM conversations WHERE id = ?1",
            [conv_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
    });
    let project_id = match project_id {
        Some(p) => p,
        None => return,
    };

    let handle = DbHandle::new(workspace_root);
    let mut conn = match handle.connect() {
        Ok(c) => c,
        Err(_) => return,
    };

    // Find the issue linked to this run
    let issue_row: Option<(String, String, String)> = conn
        .query_row(
            "SELECT id, external_id, provider FROM issues WHERE run_id = ?1 LIMIT 1",
            [run_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();

    let (issue_db_id, _external_id, _provider) = match issue_row {
        Some(row) => row,
        None => return, // run wasn't linked to an issue
    };

    let settings =
        match grove_core::db::repositories::projects_repo::get_settings(&conn, &project_id) {
            Ok(s) => s,
            Err(_) => return,
        };

    let ctx = WriteBackContext {
        run_id: run_id.to_string(),
        issue_id: issue_db_id,
        pr_url: None,
        cost_usd: 0.0,
        duration_secs: 0,
        agent_count: 0,
        error: error_msg.map(|s| s.to_string()),
    };

    if succeeded {
        let _ = on_run_completed_project(&mut conn, workspace_root, cfg, &settings, &ctx);
    } else {
        let _ = on_run_failed_project(&mut conn, workspace_root, cfg, &settings, &ctx);
    }
}

/// Drain all eligible queued tasks, respecting per-conversation and global
/// concurrency limits.
///
/// Called from `start_run` (via queue), `queue_task`, and after runs complete.
/// Multiple concurrent drains are safe: `dequeue_next_task` uses `BEGIN IMMEDIATE`
/// to atomically claim each task so no task is executed twice.
///
/// When `app_handle` is provided, events are emitted so the frontend can update
/// immediately without waiting for the next poll cycle.
pub(crate) fn drain_task_queue(
    workspace_root: std::path::PathBuf,
    initial_project_root: std::path::PathBuf,
    app_handle: Option<tauri::AppHandle>,
) {
    loop {
        let task = match grove_core::orchestrator::dequeue_next_task(&workspace_root) {
            Ok(Some(t)) => t,
            Ok(None) => break,
            Err(e) => {
                tracing::error!("drain_task_queue: dequeue_next_task failed: {e}");
                break;
            }
        };

        let project_root = resolve_project_root_for_task(&workspace_root, &task);
        let project_root = if project_root == workspace_root {
            initial_project_root.clone()
        } else {
            project_root
        };

        let cfg = match grove_core::config::GroveConfig::load_or_create(&project_root) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(task_id = %task.id, "drain_task_queue: failed to load config: {e}");
                let _ = grove_core::orchestrator::finish_task(
                    &workspace_root,
                    &task.id,
                    "failed",
                    None,
                );
                if let Some(ref h) = app_handle {
                    emit(
                        h,
                        "grove://tasks-changed",
                        serde_json::json!({
                            "conversation_id": task.conversation_id,
                        }),
                    );
                }
                continue;
            }
        };
        let task_permission_mode =
            grove_core::orchestrator::parse_permission_mode(task.permission_mode.as_deref());
        let provider = match grove_core::orchestrator::build_provider(
            &cfg,
            &project_root,
            task.provider.as_deref(),
            task_permission_mode.clone(),
            None,
        ) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(task_id = %task.id, error = %e, "drain_task_queue: failed to build provider; failing task");
                let _ = grove_core::orchestrator::finish_task(
                    &workspace_root,
                    &task.id,
                    "failed",
                    None,
                );
                if let Some(ref h) = app_handle {
                    emit(
                        h,
                        "grove://tasks-changed",
                        serde_json::json!({
                            "conversation_id": task.conversation_id,
                        }),
                    );
                }
                continue;
            }
        };
        let db_path = grove_core::config::db_path(&workspace_root);
        let abort_handle = grove_core::orchestrator::abort_handle::AbortHandle::new();

        // Register the abort handle so abort_run can kill it.
        if let Some(ref h) = app_handle {
            if let Some(ref conv_id) = task.conversation_id {
                let app_state = h.state::<crate::state::AppState>();
                app_state.set_abort(conv_id.clone(), abort_handle.clone());
            }
        }

        // Create a TauriStreamSink for real-time event streaming (when GUI is present).
        let stream_sink: Option<streaming::TauriStreamSink> = app_handle.as_ref().map(|h| {
            let pool = h.state::<crate::state::AppState>().pool().clone();
            streaming::TauriStreamSink::new(h.clone(), pool)
        });
        let sink_run_id_handle = stream_sink.as_ref().map(|s| s.run_id_handle());

        // Build the on_run_created callback so the frontend sees the run immediately.
        let on_run_created: Option<Box<dyn Fn(String) + Send + 'static>> =
            app_handle.as_ref().map(|h| {
                let notify_handle = h.clone();
                let conv_id = task.conversation_id.clone();
                let rid_handle = sink_run_id_handle.clone();
                Box::new(move |run_id: String| {
                    // Set the run_id on the TauriStreamSink so subsequent events carry it.
                    if let Some(ref handle) = rid_handle {
                        *handle.lock() = run_id.clone();
                    }
                    emit(
                        &notify_handle,
                        "grove://run-changed",
                        serde_json::json!({
                            "conversation_id": conv_id,
                            "run_id": run_id,
                        }),
                    );
                }) as Box<dyn Fn(String) + Send + 'static>
            });

        // Notify frontend that a task is now running.
        if let Some(ref h) = app_handle {
            emit(
                h,
                "grove://tasks-changed",
                serde_json::json!({
                    "conversation_id": task.conversation_id,
                }),
            );
        }

        // Build the input_handle_callback so providers register stdin handles
        // for live Q&A write-back. The run_id is captured via the same
        // Arc<Mutex<String>> that on_run_created populates.
        let input_handle_callback: Option<
            std::sync::Arc<
                dyn Fn(grove_core::providers::agent_input::AgentInputHandle) + Send + Sync,
            >,
        > = app_handle.as_ref().and_then(|h| {
            let rid_handle = sink_run_id_handle.clone()?;
            let agent_inputs =
                std::sync::Arc::clone(&h.state::<crate::state::AppState>().agent_inputs);
            Some(std::sync::Arc::new(
                move |handle: grove_core::providers::agent_input::AgentInputHandle| {
                    let run_id = rid_handle.lock().clone();
                    if !run_id.is_empty() {
                        let mut map = agent_inputs.lock();
                        map.insert(run_id, handle);
                    }
                },
            )
                as std::sync::Arc<
                    dyn Fn(grove_core::providers::agent_input::AgentInputHandle) + Send + Sync,
                >)
        });

        let task_pipeline = task
            .pipeline
            .as_deref()
            .and_then(grove_core::orchestrator::pipeline::PipelineKind::from_str);
        let run_control_callback: Option<
            std::sync::Arc<
                dyn Fn(
                        String,
                        grove_core::providers::claude_code_persistent::PersistentRunControlHandle,
                    ) + Send
                    + Sync,
            >,
        > = app_handle.as_ref().map(|h| {
            let run_controls = std::sync::Arc::clone(&h.state::<crate::state::AppState>().run_controls);
            std::sync::Arc::new(
                move |run_id: String,
                      handle: grove_core::providers::claude_code_persistent::PersistentRunControlHandle| {
                    let mut map = run_controls.lock();
                    map.insert(run_id, handle);
                },
            ) as std::sync::Arc<
                dyn Fn(
                        String,
                        grove_core::providers::claude_code_persistent::PersistentRunControlHandle,
                    ) + Send
                    + Sync,
            >
        });

        let options = grove_core::orchestrator::RunOptions {
            budget_usd: task.budget_usd,
            max_agents: None,
            model: task.model.clone(),
            provider: task.provider.clone(),
            interactive: false,
            pause_after: vec![],
            disable_phase_gates: task.disable_phase_gates,
            permission_mode: task_permission_mode,
            pipeline: task_pipeline,
            conversation_id: task.conversation_id.clone(),
            continue_last: false,
            db_path: Some(db_path),
            abort_handle: Some(abort_handle),
            issue_id: None,
            issue: None,
            resume_provider_session_id: task.resume_provider_session_id.clone(),
            on_run_created,
            input_handle_callback,
            run_control_callback,
            session_host_registry: None,
        };

        let sink_ref: Option<&dyn grove_core::providers::StreamSink> = stream_sink
            .as_ref()
            .map(|s| s as &dyn grove_core::providers::StreamSink);

        match grove_core::orchestrator::execute_objective_with_sink(
            &project_root,
            &cfg,
            &task.objective,
            options,
            provider,
            sink_ref,
        ) {
            Ok(r) => {
                let task_state = grove_core::orchestrator::task_terminal_state(&r.state);
                tracing::info!(task_id = %task.id, run_id = %r.run_id, state = task_state, "queued task finished");
                let _ = grove_core::orchestrator::finish_task(
                    &workspace_root,
                    &task.id,
                    task_state,
                    Some(&r.run_id),
                );
                // Auto-delete completed/cancelled tasks from the queue.
                if task_state == "completed" || task_state == "cancelled" {
                    let _ =
                        grove_core::orchestrator::delete_completed_task(&workspace_root, &task.id);
                }
                // Project-level workflow write-back: transition issue on success.
                if task_state == "completed" {
                    let wb_workspace = workspace_root.clone();
                    let wb_cfg = cfg.clone();
                    let run_id_wb = r.run_id.clone();
                    let conv_id_wb = task.conversation_id.clone();
                    let _ = std::thread::spawn(move || {
                        trigger_workflow_writeback(
                            &wb_workspace,
                            &wb_cfg,
                            &run_id_wb,
                            conv_id_wb.as_deref(),
                            true,
                            None,
                        );
                    });
                }
                if let Some(ref h) = app_handle {
                    emit(
                        h,
                        "grove://run-changed",
                        serde_json::json!({
                            "conversation_id": task.conversation_id,
                            "run_id": r.run_id,
                        }),
                    );
                    emit(
                        h,
                        "grove://tasks-changed",
                        serde_json::json!({
                            "conversation_id": task.conversation_id,
                        }),
                    );
                }
            }
            Err(e) => {
                tracing::error!(task_id = %task.id, "queued task failed: {e}");
                let _ = grove_core::orchestrator::finish_task(
                    &workspace_root,
                    &task.id,
                    "failed",
                    None,
                );
                // Project-level workflow write-back: move issue back on failure + comment.
                {
                    let wb_workspace = workspace_root.clone();
                    let wb_cfg = cfg.clone();
                    let err_msg = e.to_string();
                    let conv_id_wb = task.conversation_id.clone();
                    let _ = std::thread::spawn(move || {
                        if let Some(ref conv_id) = conv_id_wb {
                            let handle = grove_core::db::DbHandle::new(&wb_workspace);
                            if let Ok(conn) = handle.connect() {
                                let run_id: Option<String> = conn.query_row(
                                    "SELECT id FROM runs WHERE conversation_id = ?1 ORDER BY created_at DESC LIMIT 1",
                                    [conv_id],
                                    |r| r.get(0),
                                ).ok();
                                if let Some(ref rid) = run_id {
                                    trigger_workflow_writeback(
                                        &wb_workspace,
                                        &wb_cfg,
                                        rid,
                                        Some(conv_id),
                                        false,
                                        Some(&err_msg),
                                    );
                                }
                            }
                        }
                    });
                }
                if let Some(ref h) = app_handle {
                    emit(
                        h,
                        "grove://run-changed",
                        serde_json::json!({
                            "conversation_id": task.conversation_id,
                        }),
                    );
                    emit(
                        h,
                        "grove://tasks-changed",
                        serde_json::json!({
                            "conversation_id": task.conversation_id,
                        }),
                    );
                }
            }
        }

        // Clean up the agent stdin handle now that the run has finished.
        if let Some(ref h) = app_handle {
            if let Some(ref rid_handle) = sink_run_id_handle {
                let run_id = rid_handle.lock().clone();
                if !run_id.is_empty() {
                    let app_state = h.state::<crate::state::AppState>();
                    let mut map = app_state.agent_inputs.lock();
                    map.remove(&run_id);
                    let mut controls = app_state.run_controls.lock();
                    controls.remove(&run_id);
                }
            }
        }
    }
}

// ── Shared DTO structs (owned fields for serde) ──────────────────────────────

#[derive(serde::Serialize)]
pub struct ProviderStatusDto {
    pub kind: String,
    pub name: String,
    pub authenticated: bool,
    pub model_count: usize,
    pub default_model: String,
}

#[derive(serde::Serialize)]
pub struct ModelDefDto {
    pub id: String,
    pub name: String,
    pub context_window: u32,
    pub max_output_tokens: u32,
    pub cost_input_per_m: f64,
    pub cost_output_per_m: f64,
    pub vision: bool,
    pub tools: bool,
    pub reasoning: bool,
}

#[derive(serde::Serialize)]
pub struct LlmSelectionDto {
    pub provider: String,
    pub model: Option<String>,
    pub auth_mode: String,
}

#[derive(serde::Serialize)]
pub struct EditorIntegrationStatusDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    pub detected: bool,
    pub path: Option<String>,
}

#[derive(serde::Serialize)]
pub struct WorktreeEntryDto {
    pub session_id: String,
    pub path: String,
    pub size_bytes: u64,
    pub size_display: String,
    pub run_id: Option<String>,
    pub agent_type: Option<String>,
    pub state: Option<String>,
    pub created_at: Option<String>,
    pub ended_at: Option<String>,
    pub is_active: bool,
    pub conversation_id: Option<String>,
    pub project_id: Option<String>,
}

#[derive(serde::Serialize)]
pub struct DoctorResultDto {
    pub ok: bool,
    pub git: bool,
    pub sqlite: bool,
    pub config: bool,
    pub db: bool,
}

#[derive(serde::Serialize)]
pub struct WorktreeCleanResultDto {
    pub deleted_count: usize,
    pub freed_bytes: u64,
}

#[derive(serde::Serialize)]
pub struct FileDiffEntry {
    pub status: String,
    pub path: String,
    /// true = already committed (not in working tree); false = uncommitted/staged
    pub committed: bool,
    /// Diff area: "staged", "unstaged", "untracked", or "committed"
    pub area: String,
}

#[derive(serde::Serialize)]
pub struct ConnectionStatusDto {
    pub provider: String,
    pub connected: bool,
    pub user_display: Option<String>,
    pub error: Option<String>,
}

#[derive(serde::Serialize)]
pub struct HookConfigDto {
    pub hooks: serde_json::Value,
    pub guards: serde_json::Value,
}

#[derive(serde::Serialize)]
pub struct CapabilityCheckDto {
    pub name: String,
    pub available: bool,
    pub message: String,
}

#[derive(serde::Serialize)]
pub struct CapabilityReportDto {
    pub level: String,
    pub checks: Vec<CapabilityCheckDto>,
}

/// Serializable model entry for the frontend agent+model selector.
#[derive(serde::Serialize, Clone)]
pub struct ModelEntryDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_default: bool,
}

/// Includes `enabled` (from grove.yaml) and `detected` (CLI found on PATH).
#[derive(serde::Serialize, Clone)]
pub struct AgentCatalogEntryDto {
    pub id: String,
    pub name: String,
    pub cli: String,
    pub model_flag: Option<String>,
    pub models: Vec<ModelEntryDto>,
    pub enabled: bool,
    /// `true` when the agent's CLI binary is found on PATH.
    pub detected: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PipelineDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub agents: Vec<String>,
    pub gates: Vec<String>,
    pub is_default: bool,
}

#[derive(serde::Serialize)]
pub struct LastSessionInfo {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub provider_session_id: Option<String>,
}

#[derive(serde::Serialize, Clone)]
pub struct PhaseCheckpointDto {
    pub id: i64,
    pub run_id: String,
    pub agent: String,
    pub status: String,
    pub decision: Option<String>,
    pub decided_at: Option<String>,
    pub artifact_path: Option<String>,
    pub created_at: String,
}

/// DTO for agent config (sent to frontend as JSON).
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct AgentConfigDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub can_write: bool,
    pub can_run_commands: bool,
    pub artifact: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub skills: Vec<String>,
    pub upstream_artifacts: Vec<UpstreamArtifactDto>,
    pub prompt: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct UpstreamArtifactDto {
    pub label: String,
    pub filename: String,
}

/// DTO for pipeline config.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct PipelineConfigDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub agents: Vec<String>,
    pub gates: Vec<String>,
    pub default: bool,
    pub aliases: Vec<String>,
    pub content: String,
}

/// DTO for skill config.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct SkillConfigDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub applies_to: Vec<String>,
    pub content: String,
}

#[derive(serde::Serialize)]
pub struct TokenSavingsDto {
    pub raw_bytes: i64,
    pub filtered_bytes: i64,
    pub savings_pct: f64,
    pub by_filter_type: Vec<grove_core::token_filter::metrics::FilterTypeStat>,
}

// ── Connection status cache ──────────────────────────────────────────────────

/// In-process cache for connection status results.
/// Avoids hitting GitHub/Jira/Linear APIs on every 30s poll from multiple components.
#[allow(clippy::type_complexity)]
pub(crate) static CONNECTION_STATUS_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<Option<(Vec<CachedConnectionStatus>, std::time::Instant)>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(None));

/// TTL for cached connection status (30 seconds).
pub(crate) const CONNECTION_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// Clone-able snapshot of connection status for caching.
#[derive(Clone)]
pub(crate) struct CachedConnectionStatus {
    pub provider: String,
    pub connected: bool,
    pub user_display: Option<String>,
    pub error: Option<String>,
}

/// Returns the user's full shell PATH, cached after the first call.
///
/// macOS GUI apps launch with a minimal system PATH. We recover the full PATH
/// by spawning the user's shell in interactive-login mode (sources ~/.zshrc,
/// ~/.zprofile, etc.) and then appending a list of well-known tool directories
/// so that CLIs installed via npm/pipx/cargo/homebrew are always found even
/// when shell configs vary.
pub(crate) fn shell_path() -> &'static str {
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let user_shell = std::env::var("SHELL").unwrap_or_default();

        // Attempt order: interactive-login first (sources .zshrc), then login-only.
        // Interactive mode with stdin=/dev/null avoids TTY prompts but still runs
        // .zshrc so paths added there (nvm, ~/.local/bin, etc.) are captured.
        let shell_attempts: &[(&str, &[&str])] = &[
            (&user_shell, &["-ilc", "echo $PATH"]),
            (&user_shell, &["-lc", "echo $PATH"]),
            ("/bin/zsh", &["-ilc", "echo $PATH"]),
            ("/bin/zsh", &["-lc", "echo $PATH"]),
            ("/bin/bash", &["-lc", "echo $PATH"]),
        ];

        let mut shell_derived = String::new();
        'outer: for (shell, args) in shell_attempts {
            if shell.is_empty() {
                continue;
            }
            let mut child = match std::process::Command::new(shell)
                .args(*args)
                .stdin(std::process::Stdio::null()) // prevents interactive prompts
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => continue,
            };
            // Wait up to 3 seconds
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() >= deadline => {
                        let _ = child.kill();
                        continue 'outer;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                    Err(_) => continue 'outer,
                }
            }
            if let Ok(out) = child.wait_with_output() {
                if out.status.success() {
                    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !path.is_empty() {
                        shell_derived = path;
                        break 'outer;
                    }
                }
            }
        }

        if shell_derived.is_empty() {
            shell_derived = std::env::var("PATH").unwrap_or_default();
        }

        // Always prepend well-known tool directories so CLIs are found regardless
        // of whether the shell config was sourced. These are checked for existence
        // first to avoid polluting the PATH with non-existent entries.
        let well_known: &[String] = &[
            format!("{}/.local/bin", home),
            format!("{}/.cargo/bin", home),
            format!("{}/.bun/bin", home),
            format!("{}/.npm-global/bin", home),
            format!("{}/.claude/local/node_modules/.bin", home),
            "/opt/homebrew/bin".to_string(),
            "/opt/homebrew/sbin".to_string(),
            "/usr/local/bin".to_string(),
            "/usr/local/sbin".to_string(),
        ];

        let existing_parts: Vec<&str> = shell_derived.split(':').collect();
        let mut extra: Vec<&str> = well_known
            .iter()
            .filter(|p| {
                !p.is_empty()
                    && std::path::Path::new(p.as_str()).is_dir()
                    && !existing_parts.contains(&p.as_str())
            })
            .map(|p| p.as_str())
            .collect();
        extra.extend(existing_parts);
        extra.join(":")
    })
}

pub(crate) fn resolve_project_root_from_state(
    state: &AppState,
) -> Result<std::path::PathBuf, String> {
    Ok(state.workspace_root().to_path_buf())
}
