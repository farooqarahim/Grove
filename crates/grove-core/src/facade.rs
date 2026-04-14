//! Transport-neutral facade.
//!
//! Each function here mirrors one method of `grove_cli::transport::Transport`.
//! Both the in-process `DirectTransport` and the daemon RPC dispatcher call into
//! this module so that the two paths share a single, tested implementation.
//!
//! Conventions:
//! - `project_root` is the git project root (where `.grove/grove.yaml` lives).
//! - `workspace_root` is the centralized data directory
//!   (`~/.grove/workspaces/<id>/` — same as `paths::project_db_dir(project_root)`).
//! - Callers that only have a project root may pass it for both arguments;
//!   the path helpers centralize idempotently.

use std::path::Path;

use serde_json::{Value, json};

use crate::config::loader::load_config;
use crate::db::DbHandle;
use crate::db::repositories::{
    conversations_repo::ConversationRow, issues_repo, projects_repo::ProjectRow,
    workspaces_repo::WorkspaceRow,
};
use crate::llm::{AuthInfo, AuthStore, LlmProviderKind, LlmRouter};
use crate::orchestrator::{self, RunRecord, TaskRecord};
use crate::tracker;
use crate::worktree;
use crate::{GroveError, GroveResult};

// ── Shared DTOs ──────────────────────────────────────────────────────────────

/// Input for `start_run` — a superset of the `queue_task` defaults.
#[derive(Debug, Clone)]
pub struct StartRunInput {
    pub objective: String,
    pub pipeline: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub conversation_id: Option<String>,
    /// If true, resolve (or create) the latest active conversation for the
    /// project and, if the conversation has a completed session with a
    /// recorded `provider_session_id`, roll it forward so the provider
    /// resumes the same multi-turn context. Mirrors multica's `chat_session`
    /// pattern.
    pub continue_last: bool,
}

/// Output of `start_run`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StartRunOutput {
    pub run_id: String,
    pub task_id: String,
    pub state: String,
    pub objective: String,
}

const DEFAULT_REPORT_RUN_LIMIT: i64 = 50;

fn badarg(msg: impl Into<String>) -> GroveError {
    GroveError::Config(msg.into())
}

fn runtime(msg: impl Into<String>) -> GroveError {
    GroveError::Runtime(msg.into())
}

fn not_found(what: impl Into<String>) -> GroveError {
    GroveError::NotFound(what.into())
}

fn to_value<T: serde::Serialize>(v: &T) -> GroveResult<Value> {
    serde_json::to_value(v).map_err(GroveError::SerdeJson)
}

// ── Runs ─────────────────────────────────────────────────────────────────────

pub fn list_runs(workspace_root: &Path, limit: i64) -> GroveResult<Vec<RunRecord>> {
    orchestrator::list_runs(workspace_root, limit)
}

pub fn get_run(workspace_root: &Path, run_id: &str) -> GroveResult<Option<RunRecord>> {
    let runs = orchestrator::list_runs(workspace_root, 1000)?;
    Ok(runs.into_iter().find(|r| r.id == run_id))
}

pub fn abort_run(workspace_root: &Path, run_id: &str) -> GroveResult<()> {
    orchestrator::abort_run(workspace_root, run_id)
}

pub fn resume_run(workspace_root: &Path, run_id: &str) -> GroveResult<()> {
    orchestrator::resume_run(workspace_root, run_id).map(|_| ())
}

pub fn retry_publish_run(workspace_root: &Path, run_id: &str) -> GroveResult<()> {
    orchestrator::retry_publish_run(workspace_root, run_id).map(|_| ())
}

pub fn start_run(workspace_root: &Path, req: StartRunInput) -> GroveResult<StartRunOutput> {
    // Resolve continue_last at queue time so the queued TaskRecord carries the
    // conversation_id + resume_provider_session_id. The drain loop then passes
    // both through to `execute_objective` without re-resolving.
    let (conversation_id, resume_provider_session_id) = if req.continue_last {
        let mut conn = DbHandle::new(workspace_root).connect()?;
        let conv_id = orchestrator::conversation::resolve_conversation(
            &mut conn,
            workspace_root,
            req.conversation_id.as_deref(),
            true,
            None,
            None,
            orchestrator::conversation::RUN_CONVERSATION_KIND,
        )?;
        let resume = crate::db::repositories::sessions_repo::latest_resumable_for_conversation(
            &conn, &conv_id,
        )?;
        (Some(conv_id), resume)
    } else {
        (req.conversation_id.clone(), None)
    };

    let task = orchestrator::queue_task(
        workspace_root,
        &req.objective,
        None,
        0,
        req.model.as_deref(),
        None,
        conversation_id.as_deref(),
        resume_provider_session_id.as_deref(),
        req.pipeline.as_deref(),
        req.permission_mode.as_deref(),
        false,
    )?;
    let task_id = task.id;
    Ok(StartRunOutput {
        run_id: task.run_id.unwrap_or_else(|| task_id.clone()),
        task_id,
        state: task.state,
        objective: task.objective,
    })
}

pub fn drain_queue(_workspace_root: &Path) -> GroveResult<()> {
    Err(runtime("drain_queue not available in direct mode"))
}

// ── Tasks ────────────────────────────────────────────────────────────────────

pub fn list_tasks(workspace_root: &Path) -> GroveResult<Vec<TaskRecord>> {
    orchestrator::list_tasks(workspace_root)
}

#[allow(clippy::too_many_arguments)]
pub fn queue_task(
    workspace_root: &Path,
    objective: &str,
    priority: i64,
    model: Option<&str>,
    conversation_id: Option<&str>,
    pipeline: Option<&str>,
    permission_mode: Option<&str>,
) -> GroveResult<TaskRecord> {
    orchestrator::queue_task(
        workspace_root,
        objective,
        None,
        priority,
        model,
        None,
        conversation_id,
        None,
        pipeline,
        permission_mode,
        false,
    )
}

pub fn cancel_task(workspace_root: &Path, task_id: &str) -> GroveResult<()> {
    orchestrator::cancel_task(workspace_root, task_id)
}

// ── Workspace ────────────────────────────────────────────────────────────────

pub fn get_workspace(workspace_root: &Path) -> GroveResult<Option<WorkspaceRow>> {
    match orchestrator::get_workspace(workspace_root) {
        Ok(row) => Ok(Some(row)),
        Err(GroveError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn set_workspace_name(workspace_root: &Path, name: &str) -> GroveResult<()> {
    orchestrator::update_workspace_name(workspace_root, name)
}

pub fn archive_workspace(workspace_root: &Path, id: &str) -> GroveResult<()> {
    orchestrator::archive_workspace(workspace_root, id)
}

pub fn delete_workspace(workspace_root: &Path, id: &str) -> GroveResult<()> {
    orchestrator::delete_workspace(workspace_root, id)
}

// ── Projects ─────────────────────────────────────────────────────────────────

pub fn list_projects(workspace_root: &Path) -> GroveResult<Vec<ProjectRow>> {
    orchestrator::list_projects(workspace_root)
}

pub fn get_project(workspace_root: &Path) -> GroveResult<Option<ProjectRow>> {
    match orchestrator::get_project(workspace_root) {
        Ok(row) => Ok(Some(row)),
        Err(GroveError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn set_project_name(workspace_root: &Path, name: &str) -> GroveResult<()> {
    let project = orchestrator::get_project(workspace_root)?;
    orchestrator::update_project_name(workspace_root, &project.id, name)
}

pub fn set_project_settings(
    workspace_root: &Path,
    provider: Option<&str>,
    parallel: Option<i64>,
    pipeline: Option<&str>,
    permission_mode: Option<&str>,
) -> GroveResult<()> {
    let project = orchestrator::get_project(workspace_root)?;
    let mut settings = orchestrator::get_project_settings(workspace_root, &project.id)?;
    if let Some(p) = provider {
        settings.default_provider = Some(p.to_string());
    }
    if let Some(n) = parallel {
        settings.max_parallel_agents = Some(n);
    }
    if let Some(pl) = pipeline {
        settings.default_pipeline = Some(pl.to_string());
    }
    if let Some(pm) = permission_mode {
        settings.default_permission_mode = Some(pm.to_string());
    }
    orchestrator::update_project_settings(workspace_root, &project.id, &settings)
}

pub fn archive_project(workspace_root: &Path, id: Option<&str>) -> GroveResult<()> {
    let project_id = match id {
        Some(i) => i.to_string(),
        None => orchestrator::get_project(workspace_root)?.id,
    };
    orchestrator::archive_project(workspace_root, &project_id)
}

pub fn delete_project(workspace_root: &Path, id: Option<&str>) -> GroveResult<()> {
    let project_id = match id {
        Some(i) => i.to_string(),
        None => orchestrator::get_project(workspace_root)?.id,
    };
    orchestrator::delete_project(workspace_root, &project_id)
}

// ── Conversations ────────────────────────────────────────────────────────────

pub fn list_conversations(workspace_root: &Path, limit: i64) -> GroveResult<Vec<ConversationRow>> {
    orchestrator::list_conversations(workspace_root, limit)
}

pub fn get_conversation(workspace_root: &Path, id: &str) -> GroveResult<Option<ConversationRow>> {
    match orchestrator::get_conversation(workspace_root, id) {
        Ok(row) => Ok(Some(row)),
        Err(GroveError::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn archive_conversation(workspace_root: &Path, id: &str) -> GroveResult<()> {
    orchestrator::archive_conversation(workspace_root, id)
}

pub fn delete_conversation(workspace_root: &Path, id: &str) -> GroveResult<()> {
    orchestrator::delete_conversation(workspace_root, id)
}

pub fn rebase_conversation(workspace_root: &Path, id: &str) -> GroveResult<()> {
    orchestrator::rebase_conversation(workspace_root, id).map(|_| ())
}

pub fn merge_conversation(workspace_root: &Path, id: &str) -> GroveResult<()> {
    orchestrator::merge_conversation(workspace_root, id).map(|_| ())
}

// ── Issues ───────────────────────────────────────────────────────────────────

pub fn list_issues(workspace_root: &Path) -> GroveResult<Vec<Value>> {
    let project = orchestrator::get_project(workspace_root)?;
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let issues = issues_repo::list(&conn, &project.id, &issues_repo::IssueFilter::new())?;
    issues.iter().map(to_value).collect()
}

pub fn get_issue(workspace_root: &Path, id: &str) -> GroveResult<Value> {
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let issue = issues_repo::get(&conn, id)?.ok_or_else(|| not_found(format!("issue {id}")))?;
    to_value(&issue)
}

pub fn create_issue(
    workspace_root: &Path,
    title: &str,
    body: Option<&str>,
    labels: Vec<String>,
    priority: Option<i64>,
) -> GroveResult<Value> {
    let project = orchestrator::get_project(workspace_root)?;
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    let priority_str = priority.map(|p| p.to_string());
    let issue = issues_repo::create_native(
        &mut conn,
        &project.id,
        title,
        body,
        priority_str.as_deref(),
        &labels,
    )?;
    to_value(&issue)
}

pub fn close_issue(workspace_root: &Path, id: &str) -> GroveResult<()> {
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    issues_repo::update_status(
        &mut conn,
        id,
        "closed",
        tracker::status::CanonicalStatus::Done,
    )
}

pub fn search_issues(
    workspace_root: &Path,
    query: &str,
    limit: i64,
    provider: Option<&str>,
) -> GroveResult<Vec<Value>> {
    let project = orchestrator::get_project(workspace_root)?;
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let mut filter = issues_repo::IssueFilter::new();
    filter.limit = if limit > 0 { limit as usize } else { 100 };
    if let Some(p) = provider {
        filter.provider = Some(p.to_string());
    }
    let issues = issues_repo::list(&conn, &project.id, &filter)?;
    let q = query.to_ascii_lowercase();
    let filtered: Vec<_> = if q.is_empty() {
        issues
    } else {
        issues
            .into_iter()
            .filter(|i| {
                i.title.to_ascii_lowercase().contains(&q)
                    || i.body
                        .as_deref()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains(&q)
            })
            .collect()
    };
    filtered.iter().map(to_value).collect()
}

pub fn sync_issues(
    project_root: &Path,
    workspace_root: &Path,
    provider: Option<&str>,
    full: bool,
) -> GroveResult<Value> {
    let project = orchestrator::get_project(workspace_root)?;
    let cfg = load_config(project_root)?;
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    let incremental = !full;
    let result = if let Some(p) = provider {
        let backend: Box<dyn tracker::TrackerBackend> = match p {
            "github" => Box::new(tracker::github::GitHubTracker::new(
                project_root,
                &cfg.tracker.github,
            )),
            "jira" => Box::new(tracker::jira::JiraTracker::new(&cfg.tracker.jira)),
            "linear" => Box::new(tracker::linear::LinearTracker::new(&cfg.tracker.linear)),
            other => return Err(badarg(format!("unknown provider: {other}"))),
        };
        let r =
            tracker::sync::sync_provider(&mut conn, backend.as_ref(), &project.id, incremental, 0);
        tracker::sync::MultiSyncResult {
            total_new: r.new_count,
            total_updated: r.updated_count,
            total_errors: r.errors.len(),
            results: vec![r],
        }
    } else {
        tracker::sync::sync_all(&mut conn, &cfg, project_root, &project.id, incremental)
    };
    to_value(&result)
}

pub fn update_issue(
    workspace_root: &Path,
    id: &str,
    title: Option<&str>,
    status: Option<&str>,
    label: Option<&str>,
    assignee: Option<&str>,
    priority: Option<&str>,
) -> GroveResult<Value> {
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    let update = tracker::IssueUpdate {
        title: title.map(|s| s.to_string()),
        body: None,
        status: status.map(|s| s.to_string()),
        labels: label.map(|l| vec![l.to_string()]),
        assignee: assignee.map(|s| s.to_string()),
        priority: priority.map(|s| s.to_string()),
    };
    issues_repo::update_fields(&mut conn, id, &update)?;
    let issue = issues_repo::get(&conn, id)?.ok_or_else(|| not_found(format!("issue {id}")))?;
    to_value(&issue)
}

pub fn comment_issue(workspace_root: &Path, id: &str, body: &str) -> GroveResult<Value> {
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    let comment_id = issues_repo::add_comment(&mut conn, id, body, "user", false)?;
    Ok(json!({ "id": comment_id, "issue_id": id, "body": body, "author": "user" }))
}

pub fn assign_issue(workspace_root: &Path, id: &str, assignee: &str) -> GroveResult<()> {
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    let update = tracker::IssueUpdate {
        assignee: Some(assignee.to_string()),
        ..Default::default()
    };
    issues_repo::update_fields(&mut conn, id, &update)
}

pub fn move_issue(workspace_root: &Path, id: &str, status: &str) -> GroveResult<()> {
    let canonical = tracker::status::normalize("grove", status);
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    issues_repo::update_status(&mut conn, id, status, canonical)
}

pub fn reopen_issue(workspace_root: &Path, id: &str) -> GroveResult<()> {
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    issues_repo::update_status(
        &mut conn,
        id,
        "open",
        tracker::status::CanonicalStatus::Open,
    )
}

pub fn activity_issue(workspace_root: &Path, id: &str) -> GroveResult<Vec<Value>> {
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let events = issues_repo::list_events(&conn, id)?;
    let comments = issues_repo::list_comments(&conn, id)?;
    let mut activity: Vec<Value> = events
        .into_iter()
        .map(|e| {
            let mut v = serde_json::to_value(&e).unwrap_or(Value::Null);
            if let Value::Object(ref mut m) = v {
                m.insert("kind".to_string(), json!("event"));
            }
            v
        })
        .collect();
    let mut comment_values: Vec<Value> = comments
        .into_iter()
        .map(|c| {
            let mut v = serde_json::to_value(&c).unwrap_or(Value::Null);
            if let Value::Object(ref mut m) = v {
                m.insert("kind".to_string(), json!("comment"));
            }
            v
        })
        .collect();
    activity.append(&mut comment_values);
    activity.sort_by(|a, b| {
        let ta = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        let tb = b.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        ta.cmp(tb)
    });
    Ok(activity)
}

pub fn push_issue(workspace_root: &Path, id: &str, _provider: &str) -> GroveResult<Value> {
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let issue = issues_repo::get(&conn, id)?.ok_or_else(|| not_found(format!("issue {id}")))?;
    to_value(&issue)
}

pub fn issue_ready(workspace_root: &Path, id: &str) -> GroveResult<Value> {
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    let update = tracker::IssueUpdate {
        status: Some("ready".to_string()),
        ..Default::default()
    };
    issues_repo::update_fields(&mut conn, id, &update)?;
    let issue = issues_repo::get(&conn, id)?.ok_or_else(|| not_found(format!("issue {id}")))?;
    to_value(&issue)
}

// ── Logs / reports / plans / sessions ────────────────────────────────────────

pub fn get_logs(workspace_root: &Path, run_id: &str, all: bool) -> GroveResult<Vec<Value>> {
    let events = if all {
        orchestrator::run_events_all(workspace_root, run_id)?
    } else {
        orchestrator::run_events(workspace_root, run_id)?
    };
    events.iter().map(to_value).collect()
}

pub fn get_report(workspace_root: &Path) -> GroveResult<Value> {
    let report = orchestrator::cost_report(workspace_root, DEFAULT_REPORT_RUN_LIMIT)?;
    to_value(&report)
}

pub fn get_plan(workspace_root: &Path, run_id: Option<&str>) -> GroveResult<Value> {
    let rid = run_id.ok_or_else(|| runtime("run_id is required for plan"))?;
    let steps = orchestrator::list_plan_steps(workspace_root, rid)?;
    to_value(&steps)
}

pub fn get_subtasks(workspace_root: &Path, run_id: Option<&str>) -> GroveResult<Vec<Value>> {
    let rid = run_id.ok_or_else(|| runtime("run_id is required for subtasks"))?;
    let steps = orchestrator::list_plan_steps(workspace_root, rid)?;
    steps.iter().map(to_value).collect()
}

pub fn get_sessions(workspace_root: &Path, run_id: &str) -> GroveResult<Vec<Value>> {
    let sessions = orchestrator::list_sessions(workspace_root, run_id)?;
    sessions.iter().map(to_value).collect()
}

// ── Auth / LLM ───────────────────────────────────────────────────────────────

pub fn list_providers() -> GroveResult<Vec<Value>> {
    let statuses = LlmRouter::providers();
    let values = statuses
        .into_iter()
        .map(|s| {
            let key_hint = if s.authenticated {
                AuthStore::get(s.kind.id())
                    .map(|info| match info {
                        AuthInfo::Api { key } => {
                            let prefix: String = key.chars().take(4).collect();
                            format!("{prefix}...")
                        }
                        AuthInfo::WorkspaceCredits => "workspace-credits".to_string(),
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            json!({
                "provider": s.kind.id(),
                "name": s.name,
                "authenticated": s.authenticated,
                "key_hint": key_hint,
                "model_count": s.model_count,
                "default_model": s.default_model,
            })
        })
        .collect();
    Ok(values)
}

pub fn set_api_key(provider: &str, key: &str) -> GroveResult<()> {
    let kind = LlmProviderKind::from_str(provider)
        .ok_or_else(|| badarg(format!("unknown provider: {provider}")))?;
    LlmRouter::set_api_key(kind, key).map_err(|e| runtime(e.to_string()))
}

pub fn remove_api_key(provider: &str) -> GroveResult<()> {
    let kind = LlmProviderKind::from_str(provider)
        .ok_or_else(|| badarg(format!("unknown provider: {provider}")))?;
    LlmRouter::remove_api_key(kind).map_err(|e| runtime(e.to_string()))
}

pub fn list_models(provider: &str) -> GroveResult<Vec<Value>> {
    let kind = LlmProviderKind::from_str(provider)
        .ok_or_else(|| badarg(format!("unknown provider: {provider}")))?;
    let models = LlmRouter::models(kind);
    let values = models
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "context_window": m.context_window,
                "max_output_tokens": m.max_output_tokens,
                "cost_input_per_m": m.cost_input_per_m,
                "cost_output_per_m": m.cost_output_per_m,
                "vision": m.capabilities.vision,
                "tools": m.capabilities.tools,
                "reasoning": m.capabilities.reasoning,
            })
        })
        .collect();
    Ok(values)
}

pub fn select_llm(workspace_root: &Path, provider: &str, model: Option<&str>) -> GroveResult<()> {
    LlmProviderKind::from_str(provider)
        .ok_or_else(|| badarg(format!("unknown provider: {provider}")))?;
    let project = orchestrator::get_project(workspace_root)?;
    let mut settings = orchestrator::get_project_settings(workspace_root, &project.id)?;
    settings.default_llm_provider = Some(provider.to_string());
    if let Some(m) = model {
        settings.default_llm_model = Some(m.to_string());
    }
    orchestrator::update_project_settings(workspace_root, &project.id, &settings)
}

// ── Connect (tracker credentials) ────────────────────────────────────────────

pub fn connect_status() -> GroveResult<Vec<Value>> {
    let statuses: Vec<tracker::credentials::ConnectionStatus> = ["github", "jira", "linear"]
        .iter()
        .map(|p| {
            let connected = tracker::credentials::CredentialStore::has(p, "token");
            if connected {
                tracker::credentials::ConnectionStatus::ok(p, "configured")
            } else {
                tracker::credentials::ConnectionStatus::disconnected(p)
            }
        })
        .collect();
    statuses.iter().map(to_value).collect()
}

pub fn connect_provider(
    provider: &str,
    token: Option<&str>,
    site: Option<&str>,
    email: Option<&str>,
) -> GroveResult<()> {
    if let Some(t) = token {
        tracker::credentials::CredentialStore::store(provider, "token", t)?;
    }
    if let Some(s) = site {
        tracker::credentials::CredentialStore::store(provider, "site_url", s)?;
    }
    if let Some(e) = email {
        tracker::credentials::CredentialStore::store(provider, "email", e)?;
    }
    Ok(())
}

pub fn disconnect_provider(provider: &str) -> GroveResult<()> {
    tracker::credentials::CredentialStore::delete_provider(provider)
}

// ── Quality: lint / CI ───────────────────────────────────────────────────────

pub fn run_lint(project_root: &Path, fix: bool, _model: Option<&str>) -> GroveResult<Value> {
    let cfg = load_config(project_root)?;
    if cfg.linter.commands.is_empty() {
        return Ok(json!({"issues": [], "count": 0, "fix_mode": fix}));
    }
    let mut all_issues: Vec<Value> = Vec::new();
    for cmd_config in &cfg.linter.commands {
        let result = tracker::linter::run_linter(cmd_config, project_root)?;
        for issue in result.issues {
            if let Ok(v) = serde_json::to_value(&issue) {
                all_issues.push(v);
            }
        }
    }
    let count = all_issues.len();
    Ok(json!({"issues": all_issues, "count": count, "fix_mode": fix}))
}

pub fn run_ci(
    project_root: &Path,
    branch: Option<&str>,
    wait: bool,
    timeout: Option<u64>,
    _fix: bool,
    _model: Option<&str>,
) -> GroveResult<Value> {
    let branch_name = match branch {
        Some(b) => b.to_string(),
        None => crate::git::branch_info(project_root)
            .map(|b| b.branch)
            .unwrap_or_else(|_| "HEAD".to_string()),
    };
    let status = if wait {
        tracker::ci::wait_for_ci(project_root, &branch_name, timeout.unwrap_or(300))?
    } else {
        tracker::ci::get_ci_status(project_root, &branch_name)?
    };
    to_value(&status)
}

// ── Signals ──────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn send_signal(
    workspace_root: &Path,
    run_id: &str,
    from: &str,
    to: &str,
    signal_type: &str,
    payload: Option<&str>,
    priority: Option<i64>,
) -> GroveResult<()> {
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let sig_type = crate::signals::SignalType::parse(signal_type)
        .ok_or_else(|| badarg(format!("unknown signal type: {signal_type}")))?;
    let sig_priority = priority
        .map(|p| match p {
            i64::MIN..=-1 => crate::signals::SignalPriority::Low,
            0 => crate::signals::SignalPriority::Normal,
            1 => crate::signals::SignalPriority::High,
            _ => crate::signals::SignalPriority::Urgent,
        })
        .unwrap_or_default();
    let payload_val: Value = payload
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    crate::signals::send_signal(&conn, run_id, from, to, sig_type, sig_priority, payload_val)
        .map(|_| ())
}

pub fn check_signals(workspace_root: &Path, run_id: &str, agent: &str) -> GroveResult<Vec<Value>> {
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let signals = crate::signals::check_signals(&conn, run_id, agent)?;
    signals.iter().map(to_value).collect()
}

pub fn list_signals(workspace_root: &Path, run_id: &str) -> GroveResult<Vec<Value>> {
    let db = DbHandle::new(workspace_root);
    let conn = db.connect()?;
    let signals = crate::signals::list_for_run(&conn, run_id)?;
    signals.iter().map(to_value).collect()
}

// ── Hooks ────────────────────────────────────────────────────────────────────

pub fn run_hook(
    project_root: &Path,
    event: &str,
    agent_type: Option<&str>,
    run_id: Option<&str>,
    session_id: Option<&str>,
    _tool: Option<&str>,
    _file_path: Option<&str>,
) -> GroveResult<()> {
    let cfg = load_config(project_root)?;
    let hook_event = match event {
        "session_start" => crate::config::HookEvent::SessionStart,
        "user_prompt_submit" => crate::config::HookEvent::UserPromptSubmit,
        "pre_tool_use" => crate::config::HookEvent::PreToolUse,
        "post_tool_use" => crate::config::HookEvent::PostToolUse,
        "stop" => crate::config::HookEvent::Stop,
        "pre_compact" => crate::config::HookEvent::PreCompact,
        "post_run" => crate::config::HookEvent::PostRun,
        "pre_merge" => crate::config::HookEvent::PreMerge,
        other => return Err(badarg(format!("unknown hook event: {other}"))),
    };
    let ctx = crate::hooks::HookContext {
        run_id: run_id.unwrap_or("").to_string(),
        session_id: session_id.map(|s| s.to_string()),
        agent_type: agent_type.map(|s| s.to_string()),
        worktree_path: None,
        event: hook_event,
    };
    crate::hooks::run_hooks(&cfg.hooks, hook_event, &ctx, project_root)
}

// ── Worktrees ────────────────────────────────────────────────────────────────

pub fn list_worktrees(project_root: &Path) -> GroveResult<Vec<Value>> {
    let entries = worktree::list_worktrees(project_root, true)?;
    let values = entries
        .into_iter()
        .map(|e| {
            json!({
                "session_id": e.session_id,
                "path": e.path.to_string_lossy(),
                "size_bytes": e.size_bytes,
                "size": e.size_display(),
                "run_id": e.run_id,
                "agent_type": e.agent_type,
                "state": e.state,
                "created_at": e.created_at,
                "ended_at": e.ended_at,
                "conversation_id": e.conversation_id,
                "project_id": e.project_id,
                "active": e.is_active(),
            })
        })
        .collect();
    Ok(values)
}

pub fn clean_worktrees(project_root: &Path) -> GroveResult<Value> {
    let (count, bytes) = worktree::delete_finished_worktrees(project_root)?;
    Ok(json!({"deleted": count, "bytes_freed": bytes}))
}

pub fn delete_worktree(project_root: &Path, id: &str) -> GroveResult<()> {
    worktree::delete_worktree(project_root, id).map(|_| ())
}

pub fn delete_all_worktrees(project_root: &Path) -> GroveResult<Value> {
    let (count, bytes) = worktree::delete_all_worktrees(project_root)?;
    Ok(json!({"deleted": count, "bytes_freed": bytes}))
}

// ── Maintenance ──────────────────────────────────────────────────────────────

pub fn run_cleanup(
    project_root: &Path,
    _project: bool,
    _conversation: bool,
    _dry_run: bool,
    _yes: bool,
    _force: bool,
) -> GroveResult<Value> {
    let (deleted, bytes_freed) = worktree::delete_finished_worktrees(project_root)?;
    Ok(json!({
        "deleted_worktrees": deleted,
        "bytes_freed": bytes_freed,
    }))
}

pub fn run_gc(project_root: &Path, workspace_root: &Path, _dry_run: bool) -> GroveResult<Value> {
    let db = DbHandle::new(workspace_root);
    let mut conn = db.connect()?;
    let report = worktree::sweep_orphaned_resources(project_root, &mut conn)?;
    Ok(json!({
        "git_gc_ran": report.git_gc_ran,
        "orphaned_branches_deleted": report.orphaned_branches_deleted,
        "orphaned_dirs_removed": report.orphaned_dirs_removed,
        "ghost_sessions_recovered": report.ghost_sessions_recovered,
    }))
}

// ── Locks / merge queue ──────────────────────────────────────────────────────

pub fn list_ownership_locks(
    workspace_root: &Path,
    run_id: Option<&str>,
) -> GroveResult<Vec<Value>> {
    let locks = orchestrator::list_ownership_locks(workspace_root, run_id)?;
    locks.iter().map(to_value).collect()
}

pub fn list_merge_queue(workspace_root: &Path, conversation_id: &str) -> GroveResult<Vec<Value>> {
    let entries = orchestrator::list_merge_queue(workspace_root, conversation_id)?;
    entries.iter().map(to_value).collect()
}

#[cfg(test)]
mod tests {
    //! Tests for [`start_run`] — specifically the `continue_last` rollforward
    //! path that resolves the latest conversation + resumable session id at
    //! queue time (the B3 feature).

    use super::*;

    fn fresh_workspace() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::db::initialize(tmp.path()).expect("db init");
        tmp
    }

    #[test]
    fn start_run_without_continue_last_does_not_populate_resume_id() {
        let tmp = fresh_workspace();
        let out = start_run(
            tmp.path(),
            StartRunInput {
                objective: "test-obj".into(),
                pipeline: None,
                model: None,
                permission_mode: None,
                conversation_id: None,
                continue_last: false,
            },
        )
        .expect("start_run");

        // Inspect the queued task: resume_provider_session_id must remain None.
        let conn = crate::db::DbHandle::new(tmp.path())
            .connect()
            .expect("connect");
        let resume: Option<String> = conn
            .query_row(
                "SELECT resume_provider_session_id FROM tasks WHERE id=?1",
                [&out.task_id],
                |r| r.get(0),
            )
            .expect("query task");
        assert_eq!(
            resume, None,
            "continue_last=false must not populate resume id"
        );
    }

    #[test]
    fn start_run_with_continue_last_on_empty_project_creates_conversation_without_resume_id() {
        let tmp = fresh_workspace();
        let out = start_run(
            tmp.path(),
            StartRunInput {
                objective: "first-turn".into(),
                pipeline: None,
                model: None,
                permission_mode: None,
                conversation_id: None,
                continue_last: true,
            },
        )
        .expect("start_run");

        let conn = crate::db::DbHandle::new(tmp.path())
            .connect()
            .expect("connect");
        // Conversation should have been created and attached to the task.
        let (conv_id, resume): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT conversation_id, resume_provider_session_id FROM tasks WHERE id=?1",
                [&out.task_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("query task");
        assert!(
            conv_id.is_some(),
            "continue_last must resolve a conversation_id even on first run"
        );
        assert_eq!(
            resume, None,
            "first run has no prior session; resume id must stay None"
        );
    }

    #[test]
    fn start_run_with_continue_last_rolls_forward_provider_session_id() {
        let tmp = fresh_workspace();

        // Turn 1: queue a run so a conversation gets created, then hand-seed a
        // completed session carrying a provider_session_id (normally written by
        // the provider layer after the Claude Code subprocess finishes).
        let turn1 = start_run(
            tmp.path(),
            StartRunInput {
                objective: "turn-1".into(),
                pipeline: None,
                model: None,
                permission_mode: None,
                conversation_id: None,
                continue_last: true,
            },
        )
        .expect("start_run turn 1");

        let mut conn = crate::db::DbHandle::new(tmp.path())
            .connect()
            .expect("connect");
        let conv_id: String = conn
            .query_row(
                "SELECT conversation_id FROM tasks WHERE id=?1",
                [&turn1.task_id],
                |r| r.get(0),
            )
            .expect("query conv");

        // Insert a run under that conversation + a completed session with a
        // recorded provider_session_id.
        conn.execute(
            "INSERT INTO runs (id, conversation_id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES ('run-seed', ?1, 'turn-1', 'completed', 0.0, 0.0, '2024-01-01', '2024-01-01')",
            [&conv_id],
        ).expect("insert run");
        let sess = crate::db::repositories::sessions_repo::SessionRow {
            id: "sess-seed".into(),
            run_id: "run-seed".into(),
            agent_type: "coder".into(),
            state: "completed".into(),
            worktree_path: "/tmp/seed".into(),
            started_at: Some("2024-01-01".into()),
            ended_at: Some("2024-01-02".into()),
            created_at: "2024-01-01".into(),
            updated_at: "2024-01-02".into(),
            provider_session_id: Some("prov-seed-123".into()),
            last_heartbeat: None,
            stalled_since: None,
            checkpoint_sha: None,
            parent_checkpoint_sha: None,
            branch: None,
            pid: None,
        };
        crate::db::repositories::sessions_repo::insert(&mut conn, &sess).expect("insert session");

        // Turn 2: another continue_last run — should pick up `prov-seed-123`.
        let turn2 = start_run(
            tmp.path(),
            StartRunInput {
                objective: "turn-2".into(),
                pipeline: None,
                model: None,
                permission_mode: None,
                conversation_id: None,
                continue_last: true,
            },
        )
        .expect("start_run turn 2");

        let resume: Option<String> = conn
            .query_row(
                "SELECT resume_provider_session_id FROM tasks WHERE id=?1",
                [&turn2.task_id],
                |r| r.get(0),
            )
            .expect("query task 2");
        assert_eq!(
            resume,
            Some("prov-seed-123".into()),
            "continue_last must roll the prior session's provider id onto the new task"
        );
    }
}
