use tauri::State;

use grove_core::agents::session_record::SessionRecord;
use grove_core::db::repositories::{
    checkpoints_repo::CheckpointRow, conversations_repo::ConversationRow,
    merge_queue_repo::MergeQueueRow, messages_repo::MessageRow, ownership_repo::OwnershipLockRow,
};
use grove_core::events::EventRecord;
use grove_core::orchestrator::{
    PlanStep, RunExecutionResult, RunRecord, SubtaskRecord, TaskRecord,
};
use grove_core::publish::PublishResult;
use grove_core::reporting::{RunReport, render_markdown};
use grove_core::signals::Signal;
use grove_core::tracker::TrackerBackend;

use super::git::{
    BranchStatus, ProjectPanelData, RightPanelData, detect_default_branch, git_branch_status_sync,
    git_get_log_sync, git_project_files_sync, list_run_files_sync, load_run_workspace_meta,
    resolve_project_root, resolve_run_branch_name, resolve_run_cwd,
};
use super::{
    AgentCatalogEntryDto, CONNECTION_CACHE_TTL, CONNECTION_STATUS_CACHE, CachedConnectionStatus,
    ConnectionStatusDto, FileDiffEntry, ModelEntryDto, TokenSavingsDto, drain_task_queue, emit,
    ensure_project_is_valid_run_target, shell_path,
};
use crate::state::AppState;

#[derive(serde::Serialize)]
pub struct BootstrapData {
    pub workspace: Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>,
    pub projects: Vec<grove_core::db::repositories::projects_repo::ProjectRow>,
    pub conversations: Vec<ConversationRow>,
    pub recent_runs: Vec<RunRecord>,
    pub open_issue_count: usize,
    pub default_provider: String,
    pub agent_catalog: Vec<AgentCatalogEntryDto>,
    pub connections: Vec<ConnectionStatusDto>,
}

/// Return all data the app needs on startup in a single IPC call.
///
/// This replaces 6–8 individual queries that the frontend previously fired in
/// parallel on mount, reducing IPC round-trips and DB connection overhead.
#[tauri::command]
pub async fn get_bootstrap_data(state: State<'_, AppState>) -> Result<BootstrapData, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();

    tauri::async_runtime::spawn_blocking(move || {
        // ── 1. DB queries (single connection) ────────────────────────────
        let conn = pool.get().map_err(|e| e.to_string())?;

        // Workspace
        let workspace_id = grove_core::orchestrator::workspace::ensure_workspace(&conn)
            .map_err(|e| e.to_string())?;
        let workspace =
            grove_core::db::repositories::workspaces_repo::get(&conn, &workspace_id).ok();

        // Projects
        let projects = grove_core::db::repositories::projects_repo::list_for_workspace(
            &conn,
            &workspace_id,
            100,
        )
        .unwrap_or_default();

        // Conversations — collect across all active projects
        let mut conversations = Vec::new();
        for p in &projects {
            if let Ok(mut convs) =
                grove_core::db::repositories::conversations_repo::list_for_project(&conn, &p.id, 50)
            {
                conversations.append(&mut convs);
            }
        }

        // Recent runs (global, capped)
        let recent_runs =
            grove_core::orchestrator::list_runs(&workspace_root, 50).unwrap_or_default();

        // Open issue count — sum across all projects
        let open_issue_count: usize = projects
            .iter()
            .filter_map(|p| {
                grove_core::db::repositories::issues_repo::count_open(&conn, &p.id).ok()
            })
            .sum();

        // ── 2. Config-based data ─────────────────────────────────────────
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;

        let default_provider = cfg.providers.default.clone();

        // Agent catalog
        let search_path = shell_path();
        let agent_catalog: Vec<AgentCatalogEntryDto> = grove_core::providers::catalog::all_agents()
            .iter()
            .map(|entry| {
                let enabled = if entry.id == "claude_code" {
                    cfg.providers.claude_code.enabled
                } else {
                    cfg.providers
                        .coding_agents
                        .get(entry.id)
                        .map(|c| c.enabled)
                        .unwrap_or(true)
                };
                let detected = which::which_in(entry.cli, Some(&search_path), ".").is_ok();
                AgentCatalogEntryDto {
                    id: entry.id.to_string(),
                    name: entry.name.to_string(),
                    cli: entry.cli.to_string(),
                    model_flag: entry.model_flag.map(|s| s.to_string()),
                    models: entry
                        .models
                        .iter()
                        .map(|m| ModelEntryDto {
                            id: m.id.to_string(),
                            name: m.name.to_string(),
                            description: m.description.to_string(),
                            is_default: m.is_default,
                        })
                        .collect(),
                    enabled,
                    detected,
                }
            })
            .collect();

        // ── 3. Connection status (use cache if warm) ─────────────────────
        let connections = {
            let cache = CONNECTION_STATUS_CACHE.lock();
            if let Some((ref cached, fetched_at)) = *cache {
                if fetched_at.elapsed() < CONNECTION_CACHE_TTL {
                    Some(
                        cached
                            .iter()
                            .map(|c| ConnectionStatusDto {
                                provider: c.provider.clone(),
                                connected: c.connected,
                                user_display: c.user_display.clone(),
                                error: c.error.clone(),
                            })
                            .collect::<Vec<_>>(),
                    )
                } else {
                    None
                }
            } else {
                None
            }
        };
        let connections = match connections {
            Some(c) => c,
            None => {
                // Cold path — actually check all providers
                let mut statuses = Vec::new();

                let gh = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                let gh_s = gh.check_connection();
                statuses.push(ConnectionStatusDto {
                    provider: gh_s.provider,
                    connected: gh_s.connected,
                    user_display: gh_s.user_display,
                    error: gh_s.error,
                });

                let jira = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
                let jira_s = jira.check_connection();
                statuses.push(ConnectionStatusDto {
                    provider: jira_s.provider,
                    connected: jira_s.connected,
                    user_display: jira_s.user_display,
                    error: jira_s.error,
                });

                let linear = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                let linear_s = linear.check_connection();
                statuses.push(ConnectionStatusDto {
                    provider: linear_s.provider,
                    connected: linear_s.connected,
                    user_display: linear_s.user_display,
                    error: linear_s.error,
                });

                // Populate cache
                let cached: Vec<CachedConnectionStatus> = statuses
                    .iter()
                    .map(|s| CachedConnectionStatus {
                        provider: s.provider.clone(),
                        connected: s.connected,
                        user_display: s.user_display.clone(),
                        error: s.error.clone(),
                    })
                    .collect();
                *CONNECTION_STATUS_CACHE.lock() = Some((cached, std::time::Instant::now()));

                statuses
            }
        };

        Ok(BootstrapData {
            workspace,
            projects,
            conversations,
            recent_runs,
            open_issue_count,
            default_provider,
            agent_catalog,
            connections,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
pub struct StartRunResult {
    pub conversation_id: String,
}

#[derive(serde::Serialize)]
pub struct CreateConversationResult {
    pub conversation_id: String,
}

// ── Runs ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_runs(state: State<'_, AppState>, limit: i64) -> Result<Vec<RunRecord>, String> {
    grove_core::orchestrator::list_runs(state.workspace_root(), limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_runs_for_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<RunRecord>, String> {
    grove_core::orchestrator::list_runs_for_conversation(state.workspace_root(), &conversation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_run(state: State<'_, AppState>, id: String) -> Result<Option<RunRecord>, String> {
    let runs = grove_core::orchestrator::list_runs(state.workspace_root(), 1000)
        .map_err(|e| e.to_string())?;
    Ok(runs.into_iter().find(|r| r.id == id))
}

#[tauri::command]
pub fn list_sessions(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<SessionRecord>, String> {
    grove_core::orchestrator::list_sessions(state.workspace_root(), &run_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_plan_steps(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<PlanStep>, String> {
    grove_core::orchestrator::list_plan_steps(state.workspace_root(), &run_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn run_events(state: State<'_, AppState>, run_id: String) -> Result<Vec<EventRecord>, String> {
    grove_core::orchestrator::run_events(state.workspace_root(), &run_id).map_err(|e| e.to_string())
}

// ── Tasks ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_tasks(state: State<'_, AppState>) -> Result<Vec<TaskRecord>, String> {
    grove_core::orchestrator::list_tasks(state.workspace_root()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_tasks_for_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Vec<TaskRecord>, String> {
    grove_core::orchestrator::list_tasks_for_conversation(state.workspace_root(), &conversation_id)
        .map_err(|e| e.to_string())
}

/// Reconcile stale 'running' tasks (from crashes, orphans, etc.) and kick the
/// drain loop so the next queued task starts. Called from the "Refresh Queue" button.
#[tauri::command]
pub fn refresh_queue(state: State<'_, AppState>) -> Result<usize, String> {
    let workspace_root = state.workspace_root().to_path_buf();

    let reconciled = grove_core::orchestrator::reconcile_stale_tasks(&workspace_root)
        .map_err(|e| e.to_string())?;

    // Emit tasks-changed so the frontend refreshes immediately.
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({ "reconciled": reconciled }),
    );

    // Spawn a drain thread to process any newly eligible queued tasks.
    let bg_workspace = workspace_root.clone();
    let bg_app_handle = state.app_handle.clone();
    std::thread::spawn(move || {
        drain_task_queue(bg_workspace.clone(), bg_workspace, Some(bg_app_handle));
    });

    Ok(reconciled)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn queue_task(
    state: State<'_, AppState>,
    objective: String,
    budget_usd: Option<f64>,
    conversation_id: Option<String>,
    priority: Option<i64>,
    model: Option<String>,
    provider: Option<String>,
    // Optional: used to resolve project_root for the drain if no active run exists.
    project_id: Option<String>,
    resume_provider_session_id: Option<String>,
    disable_phase_gates: Option<bool>,
) -> Result<TaskRecord, String> {
    let workspace_root = state.workspace_root().to_path_buf();

    let resolved_project_id = conversation_id
        .as_ref()
        .and_then(|conv_id| {
            let conn = state.pool().get().ok()?;
            conn.query_row(
                "SELECT project_id FROM conversations WHERE id = ?1",
                [conv_id],
                |r| r.get::<_, String>(0),
            )
            .ok()
        })
        .or(project_id.clone());

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let resolved_project = match resolved_project_id.as_ref() {
        Some(pid) => Some(
            grove_core::db::repositories::projects_repo::get(&conn, pid)
                .map_err(|e| e.to_string())?,
        ),
        None => None,
    };

    if conversation_id.is_none() && resolved_project.is_none() {
        return Err("Select a project before starting a run.".to_string());
    }
    if let Some(ref project) = resolved_project {
        ensure_project_is_valid_run_target(project)?;
    }

    let task = grove_core::orchestrator::queue_task(
        &workspace_root,
        &objective,
        budget_usd,
        priority.unwrap_or(0),
        model.as_deref(),
        provider.as_deref(),
        conversation_id.as_deref(),
        resume_provider_session_id.as_deref(),
        None, // no pipeline override from queue_task command
        None, // no permission_mode override from queue_task command
        disable_phase_gates.unwrap_or(false),
    )
    .map_err(|e| e.to_string())?;

    // Resolve the initial project root for the drain thread.
    let initial_project_root = {
        if let Ok(conn) = state.pool().get() {
            if let Some(ref pid) = resolved_project_id {
                grove_core::db::repositories::projects_repo::get(&conn, pid)
                    .map(|p| std::path::PathBuf::from(&p.root_path))
                    .unwrap_or_else(|_| workspace_root.clone())
            } else if let Some(ref cid) = conversation_id {
                grove_core::db::repositories::conversations_repo::get(&conn, cid)
                    .ok()
                    .and_then(|conv| {
                        grove_core::db::repositories::projects_repo::get(&conn, &conv.project_id)
                            .map(|p| std::path::PathBuf::from(&p.root_path))
                            .ok()
                    })
                    .unwrap_or_else(|| workspace_root.clone())
            } else {
                workspace_root.clone()
            }
        } else {
            workspace_root.clone()
        }
    };

    // Spawn a drain thread unconditionally. dequeue_next_task uses BEGIN IMMEDIATE
    // to prevent double-execution when multiple drain threads race.
    let bg_workspace = workspace_root.clone();
    let bg_app_handle = state.app_handle.clone();
    std::thread::spawn(move || {
        drain_task_queue(bg_workspace, initial_project_root, Some(bg_app_handle));
    });

    // Notify the frontend: task list changed, so any run/conversation that owns
    // this task should also refresh.
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({
            "conversation_id": conversation_id,
        }),
    );

    Ok(task)
}

/// Start a run (fire-and-forget). The objective is always inserted into the task
/// queue first, then a drain thread picks it up for execution. This ensures
/// that multiple runs on the same conversation are properly serialized instead
/// of blocking/timing out.
///
/// When `project_id` is provided, the run uses that project's `root_path` as the
/// git working directory while the workspace DB is used for all data storage.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn start_run(
    state: State<'_, AppState>,
    objective: String,
    budget_usd: Option<f64>,
    model: Option<String>,
    provider: Option<String>,
    conversation_id: Option<String>,
    continue_last: Option<bool>,
    project_id: Option<String>,
    pipeline: Option<String>,
    max_agents: Option<u16>,
    permission_mode: Option<String>,
    disable_phase_gates: Option<bool>,
    interactive: Option<bool>,
    resume_provider_session_id: Option<String>,
    session_name: Option<String>,
) -> Result<StartRunResult, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let resolved_project_id = conversation_id
        .as_ref()
        .and_then(|conv_id| {
            let conn = state.pool().get().ok()?;
            conn.query_row(
                "SELECT project_id FROM conversations WHERE id = ?1",
                [conv_id],
                |r| r.get::<_, String>(0),
            )
            .ok()
        })
        .or(project_id.clone());

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let resolved_project = match resolved_project_id.as_ref() {
        Some(pid) => Some(
            grove_core::db::repositories::projects_repo::get(&conn, pid)
                .map_err(|e| e.to_string())?,
        ),
        None => None,
    };
    drop(conn);

    if conversation_id.is_none() && resolved_project.is_none() {
        return Err("Select a project before starting a new session.".to_string());
    }
    let project = resolved_project.as_ref().ok_or_else(|| {
        "Could not resolve the project for this run. Select a project and try again.".to_string()
    })?;
    ensure_project_is_valid_run_target(project)?;
    let project_root = std::path::PathBuf::from(&project.root_path);

    // 1. Resolve conversation synchronously so we can return the ID immediately.
    //    Pass the real project_root so ensure_project registers the actual git
    //    repo path, not the virtual workspace root.
    let conv_id = {
        let mut conn = state.pool().get().map_err(|e| e.to_string())?;
        grove_core::orchestrator::conversation::resolve_conversation(
            &mut conn,
            &project_root,
            conversation_id.as_deref(),
            continue_last.unwrap_or(false),
            Some("grove"),
            session_name.as_deref(),
            grove_core::orchestrator::conversation::RUN_CONVERSATION_KIND,
        )
        .map_err(|e| e.to_string())?
    };

    // 2. Queue the task in the DB. The drain thread will pick it up for execution.
    {
        let conn = state.pool().get().map_err(|e| e.to_string())?;
        let effective_model = model.as_deref().filter(|s| !s.is_empty());
        // Require an explicit provider — no silent config fallback.
        // The UI always sends the selected agent; an empty/missing value is an error.
        let explicit_provider = provider
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "no provider selected — choose a coding agent before starting a run".to_string()
            })?;
        let effective_provider = Some(explicit_provider);
        let effective_resume = resume_provider_session_id
            .as_deref()
            .filter(|s| !s.is_empty());
        let effective_pipeline = pipeline.as_deref().filter(|s| !s.is_empty());
        let effective_permission = permission_mode.as_deref().filter(|s| !s.is_empty());
        let effective_disable_phase_gates = disable_phase_gates.unwrap_or(false);
        let _task = grove_core::orchestrator::insert_queued_task(
            &conn,
            &objective,
            budget_usd,
            0, // default priority
            effective_model,
            effective_provider,
            Some(&conv_id),
            effective_resume,
            effective_pipeline,
            effective_permission,
            effective_disable_phase_gates,
        )
        .map_err(|e| e.to_string())?;
    }

    // Notify the frontend that a task was queued.
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({
            "conversation_id": &conv_id,
        }),
    );

    // 3. Spawn a drain thread. The drain dequeues eligible tasks (respecting
    //    per-conversation and global concurrency limits) and executes them.
    //    Multiple drain threads are safe — dequeue_next_task uses BEGIN IMMEDIATE.
    let bg_workspace = workspace_root.clone();
    let bg_project = project_root.clone();
    let bg_app_handle = state.app_handle.clone();
    let _max_agents = max_agents;
    let _interactive = interactive.unwrap_or(false);

    std::thread::spawn(move || {
        drain_task_queue(bg_workspace, bg_project, Some(bg_app_handle));
    });

    // 4. Return immediately with the conversation ID.
    Ok(StartRunResult {
        conversation_id: conv_id,
    })
}

#[tauri::command]
pub fn cancel_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    grove_core::orchestrator::cancel_task(state.workspace_root(), &id)
        .map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({ "task_id": id }),
    );
    Ok(())
}

#[tauri::command]
pub fn delete_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    grove_core::orchestrator::delete_task(state.workspace_root(), &id)
        .map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({ "task_id": id }),
    );
    Ok(())
}

#[tauri::command]
pub fn clear_queue(state: State<'_, AppState>) -> Result<usize, String> {
    let cleared = grove_core::orchestrator::clear_terminal_tasks(state.workspace_root())
        .map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({ "cleared": cleared }),
    );
    Ok(cleared)
}

#[tauri::command]
pub fn retry_task(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let _new_task =
        grove_core::orchestrator::retry_task(&workspace_root, &id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({ "retried_task_id": id }),
    );
    // Spawn drain thread to pick up the newly queued task.
    let bg_workspace = workspace_root.clone();
    let bg_app_handle = state.app_handle.clone();
    std::thread::spawn(move || {
        drain_task_queue(bg_workspace.clone(), bg_workspace, Some(bg_app_handle));
    });
    Ok(())
}

// ── Abort ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn abort_run(state: State<'_, AppState>, id: String) -> Result<(), String> {
    // Kill subprocess(es) immediately before transitioning DB state.
    // Try direct key lookup first, then look up the run's conversation_id.
    let conv_id: Option<String> = {
        state.pool().get().ok().and_then(|conn| {
            conn.query_row("SELECT conversation_id FROM runs WHERE id=?1", [&id], |r| {
                r.get(0)
            })
            .ok()
        })
    };

    let abort_handle = state
        .take_abort(&id)
        .or_else(|| conv_id.as_deref().and_then(|cid| state.take_abort(cid)));
    if let Some(h) = abort_handle {
        h.abort();
    }
    grove_core::orchestrator::abort_run(state.workspace_root(), &id).map_err(|e| e.to_string())?;

    // Cancel the running task associated with this conversation so the queue
    // immediately reflects the abort (the drain thread's finish_task is
    // conditional on state='running' so it won't overwrite this).
    if let Some(ref cid) = conv_id {
        let _ = grove_core::orchestrator::cancel_running_tasks_for_conversation(
            state.workspace_root(),
            cid,
        );
    }

    emit(
        &state.app_handle,
        "grove://run-changed",
        serde_json::json!({
            "conversation_id": conv_id,
            "run_id": id,
        }),
    );
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({
            "conversation_id": conv_id,
        }),
    );

    // Spawn a drain thread so the next queued task starts immediately instead
    // of waiting for the old drain thread to finish processing the abort.
    let bg_workspace = state.workspace_root().to_path_buf();
    let bg_pool = state.pool().clone();
    let bg_app_handle = state.app_handle.clone();
    let bg_conv_id = conv_id.clone();
    std::thread::spawn(move || {
        // Resolve the project root from the conversation (same logic as start_run).
        let project_root = {
            if let (Ok(conn), Some(cid)) = (bg_pool.get(), bg_conv_id.as_deref()) {
                grove_core::db::repositories::conversations_repo::get(&conn, cid)
                    .ok()
                    .and_then(|conv| {
                        grove_core::db::repositories::projects_repo::get(&conn, &conv.project_id)
                            .map(|p| std::path::PathBuf::from(&p.root_path))
                            .ok()
                    })
                    .unwrap_or_else(|| bg_workspace.clone())
            } else {
                bg_workspace.clone()
            }
        };
        drain_task_queue(bg_workspace, project_root, Some(bg_app_handle));
    });

    Ok(())
}

// ── Messages ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_messages(
    state: State<'_, AppState>,
    conversation_id: String,
    limit: i64,
) -> Result<Vec<MessageRow>, String> {
    grove_core::orchestrator::list_conversation_messages(
        state.workspace_root(),
        &conversation_id,
        limit,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_run_messages(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<MessageRow>, String> {
    grove_core::orchestrator::list_run_messages(state.workspace_root(), &run_id)
        .map_err(|e| e.to_string())
}

// ── Session Logs ─────────────────────────────────────────────────────────────

#[tauri::command]
pub fn read_session_log(
    state: State<'_, AppState>,
    run_id: String,
    session_id: String,
) -> Result<Vec<grove_core::session_log::LogEntry>, String> {
    let project_root = resolve_project_root(&state, &run_id);
    grove_core::session_log::read_session_log(&project_root, &run_id, &session_id)
        .map_err(|e| e.to_string())
}

// ── Resume ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn resume_run(state: State<'_, AppState>, id: String) -> Result<RunExecutionResult, String> {
    grove_core::orchestrator::resume_run(state.workspace_root(), &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn retry_publish_run(state: State<'_, AppState>, id: String) -> Result<PublishResult, String> {
    let result = grove_core::orchestrator::retry_publish_run(state.workspace_root(), &id)
        .map_err(|e| e.to_string())?;
    let conv_id: Option<String> = {
        state.pool().get().ok().and_then(|conn| {
            conn.query_row(
                "SELECT conversation_id FROM runs WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .ok()
        })
    };
    emit(
        &state.app_handle,
        "grove://run-changed",
        serde_json::json!({
            "conversation_id": conv_id,
            "run_id": id,
        }),
    );
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({
            "conversation_id": conv_id,
        }),
    );
    Ok(result)
}

// ── Subtasks ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_subtasks(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<SubtaskRecord>, String> {
    grove_core::orchestrator::list_subtasks(state.workspace_root(), &run_id)
        .map_err(|e| e.to_string())
}

// ── Ownership Locks ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_ownership_locks(
    state: State<'_, AppState>,
    run_id: Option<String>,
) -> Result<Vec<OwnershipLockRow>, String> {
    grove_core::orchestrator::list_ownership_locks(state.workspace_root(), run_id.as_deref())
        .map_err(|e| e.to_string())
}

// ── Merge Queue ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_merge_queue(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<MergeQueueRow>, String> {
    grove_core::orchestrator::list_merge_queue(state.workspace_root(), &run_id)
        .map_err(|e| e.to_string())
}

// ── Reports ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_run_report(state: State<'_, AppState>, run_id: String) -> Result<RunReport, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::reporting::build_report(&conn, &run_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_run_report_markdown(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<String, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let report = grove_core::reporting::build_report(&conn, &run_id).map_err(|e| e.to_string())?;
    Ok(render_markdown(&report))
}

#[tauri::command]
pub fn list_run_files(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<FileDiffEntry>, String> {
    let cwd = resolve_run_cwd(&state, &run_id);
    let project_root = resolve_project_root(&state, &run_id);
    Ok(list_run_files_sync(&cwd, &project_root, &run_id))
}

/// Single batch command: returns files + branch status + latest commit + diffs
/// for the right panel. Runs git operations in parallel to minimise latency.
///
/// Shares the `worktree_status` scan between file listing and diff computation,
/// eliminating the redundant second scan that happened when `get_all_file_diffs`
/// was called separately.
#[tauri::command]
pub async fn get_right_panel_data(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<RightPanelData, String> {
    let workspace_meta = load_run_workspace_meta(&state, &run_id);
    let cwd = resolve_run_cwd(&state, &run_id);
    let project_root = workspace_meta.project_root.clone();
    let conversation_id = workspace_meta.conversation_id.clone();
    let cwd_display = cwd.to_string_lossy().to_string();

    tauri::async_runtime::spawn_blocking(move || {
        let cwd1 = cwd.clone();
        let cwd2 = cwd.clone();
        let cwd3 = cwd.clone();
        let project_root1 = project_root.clone();
        let project_root2 = project_root.clone();
        let run_id1 = run_id.clone();
        let conversation_id1 = conversation_id.clone();

        let (files_and_diffs, branch_result, log_result) = std::thread::scope(|s| {
            // Thread 1: files + diffs (shared worktree_status via run_files_and_diffs).
            let h1 = s.spawn(move || {
                grove_core::git::run_files_and_diffs(&cwd1, &project_root1, &run_id1)
                    .map(|(files, diffs)| {
                        let entries: Vec<FileDiffEntry> = files
                            .into_iter()
                            .map(|c| FileDiffEntry {
                                status: c.status,
                                path: c.path,
                                committed: c.committed,
                                area: c.area,
                            })
                            .collect();
                        (entries, diffs)
                    })
                    .unwrap_or_default()
            });
            let h2 = s.spawn(move || {
                git_branch_status_sync(&cwd2, &project_root2, conversation_id1.as_deref())
            });
            let h3 = s.spawn(move || git_get_log_sync(&cwd3, Some(1)));
            (
                h1.join().unwrap_or_default(),
                h2.join()
                    .unwrap_or(Err("branch status thread panicked".to_string())),
                h3.join()
                    .unwrap_or(Err("git log thread panicked".to_string())),
            )
        });

        let (files, diffs) = files_and_diffs;
        Ok(RightPanelData {
            files,
            branch: branch_result.ok(),
            latest_commit: log_result.ok().and_then(|v| v.into_iter().next()),
            cwd: cwd_display,
            diffs,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Batch command for project mode (no run context): returns files + branch status + diffs in parallel.
#[tauri::command]
pub async fn get_project_panel_data(project_root: String) -> Result<ProjectPanelData, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root1 = project_root.clone();
        let root2 = project_root.clone();
        let root3 = project_root.clone();

        let (files_result, branch_result, log_result) = std::thread::scope(|s| {
            let h1 = s.spawn(move || git_project_files_sync(&root1));
            let h2 = s.spawn(move || {
                let cwd = std::path::Path::new(&root2);
                grove_core::worktree::git_ops::git_branch_status_full(cwd)
                    .map(|info| BranchStatus {
                        branch: info.branch,
                        default_branch: info.default_branch,
                        ahead: info.ahead,
                        behind: info.behind,
                        has_upstream: info.has_upstream,
                        remote_branch_exists: info.remote_branch_exists,
                        comparison_mode: info.comparison_mode,
                        remote_registration_state: "local_only".to_string(),
                        remote_error: None,
                    })
                    .map_err(|e| e.to_string())
            });
            let h3 = s.spawn(move || {
                let cwd = std::path::Path::new(&root3);
                git_get_log_sync(cwd, Some(1))
            });
            (
                h1.join().unwrap(),
                h2.join().unwrap(),
                h3.join()
                    .unwrap_or(Err("git log thread panicked".to_string())),
            )
        });

        let files: Vec<FileDiffEntry> = files_result.unwrap_or_default();

        let root_path = std::path::Path::new(&project_root);

        // Fetch uncommitted diffs (staged/unstaged/untracked) in parallel.
        let mut diffs: std::collections::HashMap<String, String> = std::thread::scope(|s| {
            let handles: Vec<_> = files
                .iter()
                .filter(|f| f.area != "committed")
                .map(|file| {
                    let file_path = file.path.clone();
                    let area = file.area.clone();
                    s.spawn(move || {
                        let diff = grove_core::git::file_diff(root_path, &file_path, &area)
                            .unwrap_or_default();
                        (file_path, diff)
                    })
                })
                .collect();
            handles
                .into_iter()
                .filter_map(|h| h.join().ok())
                .filter(|(_, diff)| !diff.is_empty())
                .collect()
        });

        // Fetch committed-but-not-pushed diffs as a single batched range diff.
        if files.iter().any(|f| f.area == "committed") {
            let default_branch = detect_default_branch(root_path);
            let base_candidates = [
                "@{u}".to_string(),
                format!("refs/remotes/origin/{default_branch}"),
                format!("refs/heads/{default_branch}"),
                "refs/remotes/origin/main".to_string(),
                "refs/remotes/origin/master".to_string(),
                "refs/heads/main".to_string(),
                "refs/heads/master".to_string(),
            ];
            for base in &base_candidates {
                if let Ok(committed_diffs) =
                    grove_core::git::committed_range_diffs(root_path, base, "HEAD")
                {
                    for (path, diff) in committed_diffs {
                        diffs.entry(path).or_insert(diff);
                    }
                    break;
                }
            }
        }

        Ok(ProjectPanelData {
            files,
            branch: branch_result.ok(),
            latest_commit: log_result.ok().and_then(|v| v.into_iter().next()),
            cwd: project_root.clone(),
            diffs,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Batch diff command — returns all file diffs for a run using in-process gix.
/// Uncommitted files (staged/unstaged/untracked) now return real diffs instead of empty strings.
#[tauri::command]
pub async fn get_all_file_diffs(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<std::collections::HashMap<String, String>, String> {
    let cwd = resolve_run_cwd(&state, &run_id);
    let project_root = resolve_project_root(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        grove_core::git::all_diffs_for_run(&cwd, &project_root, &run_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_file_diff(
    state: State<'_, AppState>,
    run_id: String,
    file_path: String,
    // "staged", "unstaged", "untracked", or "committed" (default when area is absent)
    area: Option<String>,
) -> Result<String, String> {
    let cwd = resolve_run_cwd(&state, &run_id);
    let project_root = resolve_project_root(&state, &run_id);
    let branch_name = resolve_run_branch_name(&state, &run_id);

    tauri::async_runtime::spawn_blocking(move || {
        let area_str = area.as_deref().unwrap_or("committed");

        match area_str {
            "staged" | "unstaged" | "untracked" => {
                // Uncommitted file — compute diff against working tree or index via gix.
                grove_core::git::file_diff(&cwd, &file_path, area_str).map_err(|e| e.to_string())
            }
            _ => {
                // Committed file — diff conversation/current branch vs default branch.
                let default_branch =
                    grove_core::worktree::git_ops::detect_default_branch(&project_root)
                        .unwrap_or_else(|_| "main".to_string());
                let head_ref = branch_name
                    .as_ref()
                    .map(|branch| format!("refs/heads/{branch}"))
                    .unwrap_or_else(|| "HEAD".to_string());
                let base_candidates = [
                    format!("refs/remotes/origin/{default_branch}"),
                    format!("refs/heads/{default_branch}"),
                    "refs/remotes/origin/main".to_string(),
                    "refs/remotes/origin/master".to_string(),
                    "refs/heads/main".to_string(),
                    "refs/heads/master".to_string(),
                ];
                for base in &base_candidates {
                    if let Ok(diffs) =
                        grove_core::git::committed_range_diffs(&project_root, base, &head_ref)
                    {
                        if let Some(diff) = diffs.get(&file_path) {
                            return Ok(diff.clone());
                        }
                    }
                }
                Ok(String::new())
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Signals ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_signals(state: State<'_, AppState>, run_id: String) -> Result<Vec<Signal>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::signals::list_for_run(&conn, &run_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn mark_signal_read(state: State<'_, AppState>, signal_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::signals::mark_read(&conn, &signal_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://signals-changed",
        serde_json::json!({ "signal_id": signal_id }),
    );
    Ok(())
}

// ── Checkpoints ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_checkpoints(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<CheckpointRow>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::checkpoints_repo::list_for_run(&conn, &run_id)
        .map_err(|e| e.to_string())
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn start_run_from_issue(
    state: State<'_, AppState>,
    issue_id: String,
    additional_prompt: Option<String>,
    budget_usd: Option<f64>,
    model: Option<String>,
    project_id: Option<String>,
    provider: Option<String>,
    conversation_id: Option<String>,
    disable_phase_gates: Option<bool>,
) -> Result<StartRunResult, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let resolved_project_id = conversation_id
        .as_ref()
        .and_then(|conv_id| {
            let conn = state.pool().get().ok()?;
            conn.query_row(
                "SELECT project_id FROM conversations WHERE id = ?1",
                [conv_id],
                |r| r.get::<_, String>(0),
            )
            .ok()
        })
        .or(project_id.clone());
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let resolved_project = match resolved_project_id.as_ref() {
        Some(pid) => Some(
            grove_core::db::repositories::projects_repo::get(&conn, pid)
                .map_err(|e| e.to_string())?,
        ),
        None => None,
    };
    drop(conn);

    if conversation_id.is_none() && resolved_project.is_none() {
        return Err("Select a project before starting a run from an issue.".to_string());
    }
    let project = resolved_project.as_ref().ok_or_else(|| {
        "Could not resolve the project for this issue run. Select a project and try again."
            .to_string()
    })?;
    ensure_project_is_valid_run_target(project)?;
    let project_root = std::path::PathBuf::from(&project.root_path);

    // Fetch the issue from the tracker.
    // Resolution order:
    //  1. Local DB cache (covers grove-native issues and previously synced external issues).
    //  2. Project-level configured provider (e.g. GitHub with a specific repo key).
    //  3. Workspace-level TrackerRegistry (legacy / multi-provider fallback).
    let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
        .map_err(|e| e.to_string())?;

    // Step 1: check the local DB cache, scoped to the project.
    let db_issue: Option<grove_core::tracker::Issue> =
        resolved_project_id.as_deref().and_then(|pid| {
            let conn = state.pool().get().ok()?;
            grove_core::tracker::get_cached(&conn, &issue_id, pid)
                .ok()
                .flatten()
        });

    // Step 2: try every provider that has a board/repo key configured for this
    // project — regardless of what default_provider is set to.  This means the
    // user doesn't have to set a default provider just to start a run from an
    // issue they picked from the GitHub connector dropdown.
    let project_issue: Option<grove_core::tracker::Issue> = if db_issue.is_none() {
        resolved_project_id.as_deref().and_then(|pid| {
            let conn = state.pool().get().ok()?;
            let settings =
                grove_core::db::repositories::projects_repo::get_settings(&conn, pid).ok()?;

            // GitHub — try if a repo key is configured
            if let Some(repo_key) = settings.project_key_for("github") {
                let tracker = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                if let Ok(issue) = tracker.show_for_repo(&issue_id, Some(repo_key)) {
                    return Some(issue);
                }
            }

            // Jira — try if a project key is configured or jira is the default
            if settings.jira_project_key.is_some()
                || settings.default_provider.as_deref() == Some("jira")
            {
                let tracker = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
                if let Ok(issue) = tracker.show(&issue_id) {
                    return Some(issue);
                }
            }

            // Linear — try if a team key is configured or linear is the default
            if settings.linear_project_key.is_some()
                || settings.default_provider.as_deref() == Some("linear")
            {
                let tracker = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                if let Ok(issue) = tracker.show(&issue_id) {
                    return Some(issue);
                }
            }

            None
        })
    } else {
        None
    };

    // Step 3: workspace-level registry fallback.
    let registry = grove_core::tracker::registry::TrackerRegistry::from_config(&cfg, &project_root);
    let issue = match db_issue.or(project_issue) {
        Some(i) => i,
        None => registry
            .find_issue(&issue_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("issue '{issue_id}' not found in any connected tracker"))?,
    };

    // Build enriched objective
    let user_prompt = additional_prompt.as_deref().unwrap_or("");
    let objective = grove_core::orchestrator::issue_context::enrich_objective(&issue, user_prompt);

    // Resolve conversation — reuse the caller's conversation when provided,
    // otherwise create a new one scoped to the project.
    let conv_id = {
        let mut conn = state.pool().get().map_err(|e| e.to_string())?;
        grove_core::orchestrator::conversation::resolve_conversation(
            &mut conn,
            &project_root,
            conversation_id.as_deref(),
            false,
            Some("grove"),
            None,
            grove_core::orchestrator::conversation::RUN_CONVERSATION_KIND,
        )
        .map_err(|e| e.to_string())?
    };

    // Trigger project-level workflow: move the issue to "In Progress".
    if let Some(ref pid) = resolved_project_id {
        let wb_pool = state.pool().clone();
        let wb_cfg = cfg.clone();
        let wb_issue = issue.clone();
        let pid_clone = pid.clone();
        let wb_workspace = workspace_root.clone();
        let _ = std::thread::spawn(move || {
            if let Ok(mut conn) = wb_pool.get() {
                if let Ok(settings) =
                    grove_core::db::repositories::projects_repo::get_settings(&conn, &pid_clone)
                {
                    let _ = grove_core::tracker::write_back::on_run_started_project(
                        &mut conn,
                        &wb_workspace,
                        &wb_cfg,
                        &settings,
                        &wb_issue,
                    );
                }
            }
        });
    }

    // Queue the task in the DB. The drain thread picks it up for execution.
    {
        let conn = state.pool().get().map_err(|e| e.to_string())?;
        let effective_model = model.as_deref().filter(|s| !s.is_empty());
        // Require an explicit provider — no silent config fallback.
        let explicit_provider = provider
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                "no provider selected — choose a coding agent before starting a run".to_string()
            })?;
        let effective_provider = Some(explicit_provider);
        let _task = grove_core::orchestrator::insert_queued_task(
            &conn,
            &objective,
            budget_usd,
            0, // default priority
            effective_model,
            effective_provider,
            Some(&conv_id),
            None, // issue-linked runs don't resume provider sessions
            None, // no pipeline override for issue-linked runs
            None, // no permission_mode override for issue-linked runs
            disable_phase_gates.unwrap_or(false),
        )
        .map_err(|e| e.to_string())?;
    }

    // Notify the frontend that a task was queued.
    emit(
        &state.app_handle,
        "grove://tasks-changed",
        serde_json::json!({
            "conversation_id": &conv_id,
        }),
    );

    // Spawn a drain thread.
    let bg_workspace = workspace_root;
    let bg_project = project_root;
    let bg_app_handle = state.app_handle.clone();
    std::thread::spawn(move || {
        drain_task_queue(bg_workspace, bg_project, Some(bg_app_handle));
    });

    Ok(StartRunResult {
        conversation_id: conv_id,
    })
}

#[tauri::command]
pub fn get_run_token_savings(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<TokenSavingsDto, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let savings = grove_core::token_filter::metrics::query_run_savings(&conn, &run_id)
        .map_err(|e| e.to_string())?;
    let by_type = grove_core::token_filter::metrics::query_run_savings_by_type(&conn, &run_id)
        .map_err(|e| e.to_string())?;
    Ok(TokenSavingsDto {
        raw_bytes: savings.raw_bytes,
        filtered_bytes: savings.filtered_bytes,
        savings_pct: savings.savings_pct,
        by_filter_type: by_type,
    })
}

// ── App version ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_app_version(app: tauri::AppHandle) -> String {
    app.config()
        .version
        .clone()
        .unwrap_or_else(|| "0.0.0".to_string())
}

#[cfg(debug_assertions)]
#[tauri::command]
pub fn detect_debug() -> String {
    let path = shell_path();
    let agents = ["claude", "codex", "gemini", "aider", "goose", "amp"];
    let found: Vec<String> = agents
        .iter()
        .map(|a| {
            let p = which::which_in(a, Some(path), ".")
                .ok()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "NOT FOUND".to_string());
            format!("{a}: {p}")
        })
        .collect();
    let result = format!(
        "SHELL={}\nSYSTEM_PATH={}\nSHELL_PATH={}\nAGENTS:\n{}",
        std::env::var("SHELL").unwrap_or_else(|_| "(not set)".into()),
        std::env::var("PATH").unwrap_or_else(|_| "(not set)".into()),
        path,
        found.join("\n"),
    );
    tracing::info!("detect_debug:\n{}", result);
    result
}
