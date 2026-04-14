pub mod envelope;
pub mod health;

pub mod auth;
pub mod connect;
pub mod conversations;
pub mod hooks;
pub mod issues;
pub mod locks;
pub mod logs;
pub mod maintenance;
pub mod projects;
pub mod quality;
pub mod runs;
pub mod signals;
pub mod tasks;
pub mod workspace;
pub mod worktrees;

use crate::config::DaemonConfig;
use crate::queue_drain::DrainSignal;
use envelope::{RpcError, RpcRequest, RpcResponse};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct DispatchCtx {
    pub cfg: Arc<DaemonConfig>,
    pub started_at: Instant,
    pub drain_signal: DrainSignal,
}

impl DispatchCtx {
    pub fn new(cfg: DaemonConfig, drain_signal: DrainSignal) -> Self {
        Self {
            cfg: Arc::new(cfg),
            started_at: Instant::now(),
            drain_signal,
        }
    }
}

pub(crate) fn invalid_params<E: std::fmt::Display>(e: E) -> RpcError {
    RpcError::invalid_params(e.to_string())
}

pub(crate) fn internal<E: std::fmt::Display>(e: E) -> RpcError {
    RpcError::internal(e.to_string())
}

pub(crate) fn join_err<E: std::fmt::Display>(e: E) -> RpcError {
    RpcError::internal(format!("join: {e}"))
}

pub(crate) fn to_value<T: serde::Serialize>(v: &T) -> Result<serde_json::Value, RpcError> {
    serde_json::to_value(v).map_err(internal)
}

pub async fn dispatch(ctx: &DispatchCtx, req: RpcRequest) -> RpcResponse {
    if req.jsonrpc != "2.0" {
        return RpcResponse::err(req.id, RpcError::invalid_request("jsonrpc must be \"2.0\""));
    }
    let id = req.id.clone();
    let result = match req.method.as_str() {
        "grove.health" => health::handle(ctx, req.params).await,

        // Runs
        "grove.list_runs" => runs::list_runs(ctx, req.params).await,
        "grove.get_run" => runs::get_run(ctx, req.params).await,
        "grove.abort_run" => runs::abort_run(ctx, req.params).await,
        "grove.resume_run" => runs::resume_run(ctx, req.params).await,
        "grove.start_run" => runs::start_run(ctx, req.params).await,
        "grove.drain_queue" => runs::drain_queue(ctx, req.params).await,
        "grove.retry_publish_run" => runs::retry_publish_run(ctx, req.params).await,

        // Tasks
        "grove.list_tasks" => tasks::list_tasks(ctx, req.params).await,
        "grove.queue_task" => tasks::queue_task(ctx, req.params).await,
        "grove.cancel_task" => tasks::cancel_task(ctx, req.params).await,

        // Workspace
        "grove.get_workspace" => workspace::get_workspace(ctx, req.params).await,
        "grove.set_workspace_name" => workspace::set_workspace_name(ctx, req.params).await,
        "grove.archive_workspace" => workspace::archive_workspace(ctx, req.params).await,
        "grove.delete_workspace" => workspace::delete_workspace(ctx, req.params).await,

        // Projects
        "grove.list_projects" => projects::list_projects(ctx, req.params).await,
        "grove.get_project" => projects::get_project(ctx, req.params).await,
        "grove.set_project_name" => projects::set_project_name(ctx, req.params).await,
        "grove.set_project_settings" => projects::set_project_settings(ctx, req.params).await,
        "grove.archive_project" => projects::archive_project(ctx, req.params).await,
        "grove.delete_project" => projects::delete_project(ctx, req.params).await,

        // Conversations
        "grove.list_conversations" => conversations::list_conversations(ctx, req.params).await,
        "grove.get_conversation" => conversations::get_conversation(ctx, req.params).await,
        "grove.archive_conversation" => conversations::archive_conversation(ctx, req.params).await,
        "grove.delete_conversation" => conversations::delete_conversation(ctx, req.params).await,
        "grove.rebase_conversation" => conversations::rebase_conversation(ctx, req.params).await,
        "grove.merge_conversation" => conversations::merge_conversation(ctx, req.params).await,

        // Issues
        "grove.list_issues" => issues::list_issues(ctx, req.params).await,
        "grove.get_issue" => issues::get_issue(ctx, req.params).await,
        "grove.create_issue" => issues::create_issue(ctx, req.params).await,
        "grove.close_issue" => issues::close_issue(ctx, req.params).await,
        "grove.search_issues" => issues::search_issues(ctx, req.params).await,
        "grove.sync_issues" => issues::sync_issues(ctx, req.params).await,
        "grove.update_issue" => issues::update_issue(ctx, req.params).await,
        "grove.comment_issue" => issues::comment_issue(ctx, req.params).await,
        "grove.assign_issue" => issues::assign_issue(ctx, req.params).await,
        "grove.move_issue" => issues::move_issue(ctx, req.params).await,
        "grove.reopen_issue" => issues::reopen_issue(ctx, req.params).await,
        "grove.activity_issue" => issues::activity_issue(ctx, req.params).await,
        "grove.push_issue" => issues::push_issue(ctx, req.params).await,
        "grove.issue_ready" => issues::issue_ready(ctx, req.params).await,

        // Logs / reports
        "grove.get_logs" => logs::get_logs(ctx, req.params).await,
        "grove.get_report" => logs::get_report(ctx, req.params).await,
        "grove.get_plan" => logs::get_plan(ctx, req.params).await,
        "grove.get_subtasks" => logs::get_subtasks(ctx, req.params).await,
        "grove.get_sessions" => logs::get_sessions(ctx, req.params).await,

        // Auth / LLM
        "grove.list_providers" => auth::list_providers(ctx, req.params).await,
        "grove.set_api_key" => auth::set_api_key(ctx, req.params).await,
        "grove.remove_api_key" => auth::remove_api_key(ctx, req.params).await,
        "grove.list_models" => auth::list_models(ctx, req.params).await,
        "grove.select_llm" => auth::select_llm(ctx, req.params).await,

        // Connect (tracker credentials)
        "grove.connect_status" => connect::connect_status(ctx, req.params).await,
        "grove.connect_provider" => connect::connect_provider(ctx, req.params).await,
        "grove.disconnect_provider" => connect::disconnect_provider(ctx, req.params).await,

        // Quality
        "grove.run_lint" => quality::run_lint(ctx, req.params).await,
        "grove.run_ci" => quality::run_ci(ctx, req.params).await,

        // Signals
        "grove.send_signal" => signals::send_signal(ctx, req.params).await,
        "grove.check_signals" => signals::check_signals(ctx, req.params).await,
        "grove.list_signals" => signals::list_signals(ctx, req.params).await,

        // Hooks
        "grove.run_hook" => hooks::run_hook(ctx, req.params).await,

        // Worktrees
        "grove.list_worktrees" => worktrees::list_worktrees(ctx, req.params).await,
        "grove.clean_worktrees" => worktrees::clean_worktrees(ctx, req.params).await,
        "grove.delete_worktree" => worktrees::delete_worktree(ctx, req.params).await,
        "grove.delete_all_worktrees" => worktrees::delete_all_worktrees(ctx, req.params).await,

        // Maintenance
        "grove.run_cleanup" => maintenance::run_cleanup(ctx, req.params).await,
        "grove.run_gc" => maintenance::run_gc(ctx, req.params).await,

        // Locks / merge queue
        "grove.list_ownership_locks" => locks::list_ownership_locks(ctx, req.params).await,
        "grove.list_merge_queue" => locks::list_merge_queue(ctx, req.params).await,

        other => Err(RpcError::method_not_found(other)),
    };
    match result {
        Ok(v) => RpcResponse::ok(id, v),
        Err(e) => RpcResponse::err(id, e),
    }
}
