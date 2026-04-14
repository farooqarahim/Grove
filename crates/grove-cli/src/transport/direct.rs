use std::path::{Path, PathBuf};

use super::{RunResult, StartRunRequest, Transport};
use crate::error::{CliError, CliResult};
use grove_core::facade;

pub struct DirectTransport {
    /// The actual git project root — used for git, config, worktree, and CI operations.
    project: PathBuf,
    /// The centralized Grove workspace root (~/.grove/workspaces/<id>/) — used for all DB operations.
    workspace_root: PathBuf,
}

impl DirectTransport {
    #[allow(dead_code)] // called from GroveTransport::detect (Task 6)
    pub fn new(project: &Path, workspace_root: &Path) -> Self {
        Self {
            project: project.to_owned(),
            workspace_root: workspace_root.to_owned(),
        }
    }
}

impl Transport for DirectTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        facade::list_runs(&self.workspace_root, limit).map_err(CliError::Core)
    }

    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        facade::list_tasks(&self.workspace_root).map_err(CliError::Core)
    }

    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        facade::get_workspace(&self.workspace_root).map_err(CliError::Core)
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        facade::list_projects(&self.workspace_root).map_err(CliError::Core)
    }

    fn list_conversations(
        &self,
        limit: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        facade::list_conversations(&self.workspace_root, limit).map_err(CliError::Core)
    }

    fn list_issues(&self, _cached: bool) -> CliResult<Vec<serde_json::Value>> {
        facade::list_issues(&self.workspace_root).map_err(CliError::Core)
    }

    fn get_issue(&self, id: &str) -> CliResult<serde_json::Value> {
        facade::get_issue(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn create_issue(
        &self,
        title: &str,
        body: Option<&str>,
        labels: Vec<String>,
        priority: Option<i64>,
    ) -> CliResult<serde_json::Value> {
        facade::create_issue(&self.workspace_root, title, body, labels, priority)
            .map_err(CliError::Core)
    }

    fn close_issue(&self, id: &str) -> CliResult<()> {
        facade::close_issue(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn search_issues(
        &self,
        query: &str,
        limit: i64,
        provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>> {
        facade::search_issues(&self.workspace_root, query, limit, provider)
            .map_err(CliError::Core)
    }

    fn sync_issues(&self, provider: Option<&str>, full: bool) -> CliResult<serde_json::Value> {
        facade::sync_issues(&self.project, &self.workspace_root, provider, full)
            .map_err(CliError::Core)
    }

    fn queue_task(
        &self,
        objective: &str,
        priority: i64,
        model: Option<&str>,
        conversation_id: Option<&str>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<grove_core::orchestrator::TaskRecord> {
        facade::queue_task(
            &self.workspace_root,
            objective,
            priority,
            model,
            conversation_id,
            pipeline,
            permission_mode,
        )
        .map_err(CliError::Core)
    }

    fn cancel_task(&self, task_id: &str) -> CliResult<()> {
        facade::cancel_task(&self.workspace_root, task_id).map_err(CliError::Core)
    }

    fn drain_queue(&self, _project: &std::path::Path) -> CliResult<()> {
        facade::drain_queue(&self.workspace_root).map_err(CliError::Core)
    }

    fn get_logs(&self, run_id: &str, all: bool) -> CliResult<Vec<serde_json::Value>> {
        facade::get_logs(&self.workspace_root, run_id, all).map_err(CliError::Core)
    }

    fn get_report(&self, _run_id: &str) -> CliResult<serde_json::Value> {
        facade::get_report(&self.workspace_root).map_err(CliError::Core)
    }

    fn get_plan(&self, run_id: Option<&str>) -> CliResult<serde_json::Value> {
        facade::get_plan(&self.workspace_root, run_id).map_err(CliError::Core)
    }

    fn get_subtasks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        facade::get_subtasks(&self.workspace_root, run_id).map_err(CliError::Core)
    }

    fn get_sessions(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        facade::get_sessions(&self.workspace_root, run_id).map_err(CliError::Core)
    }

    fn abort_run(&self, run_id: &str) -> CliResult<()> {
        facade::abort_run(&self.workspace_root, run_id).map_err(CliError::Core)
    }

    fn resume_run(&self, run_id: &str) -> CliResult<()> {
        facade::resume_run(&self.workspace_root, run_id).map_err(CliError::Core)
    }

    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>> {
        facade::list_providers().map_err(CliError::Core)
    }

    fn set_api_key(&self, provider: &str, key: &str) -> CliResult<()> {
        facade::set_api_key(provider, key).map_err(CliError::Core)
    }

    fn remove_api_key(&self, provider: &str) -> CliResult<()> {
        facade::remove_api_key(provider).map_err(CliError::Core)
    }

    fn list_models(&self, provider: &str) -> CliResult<Vec<serde_json::Value>> {
        facade::list_models(provider).map_err(CliError::Core)
    }

    fn select_llm(&self, provider: &str, model: Option<&str>) -> CliResult<()> {
        facade::select_llm(&self.workspace_root, provider, model).map_err(CliError::Core)
    }

    fn update_issue(
        &self,
        id: &str,
        title: Option<&str>,
        status: Option<&str>,
        label: Option<&str>,
        assignee: Option<&str>,
        priority: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        facade::update_issue(
            &self.workspace_root,
            id,
            title,
            status,
            label,
            assignee,
            priority,
        )
        .map_err(CliError::Core)
    }

    fn comment_issue(&self, id: &str, body: &str) -> CliResult<serde_json::Value> {
        facade::comment_issue(&self.workspace_root, id, body).map_err(CliError::Core)
    }

    fn assign_issue(&self, id: &str, assignee: &str) -> CliResult<()> {
        facade::assign_issue(&self.workspace_root, id, assignee).map_err(CliError::Core)
    }

    fn move_issue(&self, id: &str, status: &str) -> CliResult<()> {
        facade::move_issue(&self.workspace_root, id, status).map_err(CliError::Core)
    }

    fn reopen_issue(&self, id: &str) -> CliResult<()> {
        facade::reopen_issue(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn activity_issue(&self, id: &str) -> CliResult<Vec<serde_json::Value>> {
        facade::activity_issue(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn push_issue(&self, id: &str, provider: &str) -> CliResult<serde_json::Value> {
        facade::push_issue(&self.workspace_root, id, provider).map_err(CliError::Core)
    }

    fn issue_ready(&self, id: &str) -> CliResult<serde_json::Value> {
        facade::issue_ready(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>> {
        facade::connect_status().map_err(CliError::Core)
    }

    fn connect_provider(
        &self,
        provider: &str,
        token: Option<&str>,
        site: Option<&str>,
        email: Option<&str>,
    ) -> CliResult<()> {
        facade::connect_provider(provider, token, site, email).map_err(CliError::Core)
    }

    fn disconnect_provider(&self, provider: &str) -> CliResult<()> {
        facade::disconnect_provider(provider).map_err(CliError::Core)
    }

    fn run_lint(&self, fix: bool, model: Option<&str>) -> CliResult<serde_json::Value> {
        facade::run_lint(&self.project, fix, model).map_err(CliError::Core)
    }

    fn run_ci(
        &self,
        branch: Option<&str>,
        wait: bool,
        timeout: Option<u64>,
        fix: bool,
        model: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        facade::run_ci(&self.project, branch, wait, timeout, fix, model).map_err(CliError::Core)
    }

    fn set_workspace_name(&self, name: &str) -> CliResult<()> {
        facade::set_workspace_name(&self.workspace_root, name).map_err(CliError::Core)
    }

    fn archive_workspace(&self, id: &str) -> CliResult<()> {
        facade::archive_workspace(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn delete_workspace(&self, id: &str) -> CliResult<()> {
        facade::delete_workspace(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>> {
        facade::get_project(&self.workspace_root).map_err(CliError::Core)
    }

    fn set_project_name(&self, name: &str) -> CliResult<()> {
        facade::set_project_name(&self.workspace_root, name).map_err(CliError::Core)
    }

    fn set_project_settings(
        &self,
        provider: Option<&str>,
        parallel: Option<i64>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<()> {
        facade::set_project_settings(
            &self.workspace_root,
            provider,
            parallel,
            pipeline,
            permission_mode,
        )
        .map_err(CliError::Core)
    }

    fn archive_project(&self, id: Option<&str>) -> CliResult<()> {
        facade::archive_project(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn delete_project(&self, id: Option<&str>) -> CliResult<()> {
        facade::delete_project(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn get_conversation(
        &self,
        id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        facade::get_conversation(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn archive_conversation(&self, id: &str) -> CliResult<()> {
        facade::archive_conversation(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn delete_conversation(&self, id: &str) -> CliResult<()> {
        facade::delete_conversation(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn rebase_conversation(&self, id: &str) -> CliResult<()> {
        facade::rebase_conversation(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn merge_conversation(&self, id: &str) -> CliResult<()> {
        facade::merge_conversation(&self.workspace_root, id).map_err(CliError::Core)
    }

    fn send_signal(
        &self,
        run_id: &str,
        from: &str,
        to: &str,
        signal_type: &str,
        payload: Option<&str>,
        priority: Option<i64>,
    ) -> CliResult<()> {
        facade::send_signal(
            &self.workspace_root,
            run_id,
            from,
            to,
            signal_type,
            payload,
            priority,
        )
        .map_err(CliError::Core)
    }

    fn check_signals(&self, run_id: &str, agent: &str) -> CliResult<Vec<serde_json::Value>> {
        facade::check_signals(&self.workspace_root, run_id, agent).map_err(CliError::Core)
    }

    fn list_signals(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        facade::list_signals(&self.workspace_root, run_id).map_err(CliError::Core)
    }

    fn run_hook(
        &self,
        event: &str,
        agent_type: Option<&str>,
        run_id: Option<&str>,
        session_id: Option<&str>,
        tool: Option<&str>,
        file_path: Option<&str>,
    ) -> CliResult<()> {
        facade::run_hook(
            &self.project,
            event,
            agent_type,
            run_id,
            session_id,
            tool,
            file_path,
        )
        .map_err(CliError::Core)
    }

    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>> {
        facade::list_worktrees(&self.project).map_err(CliError::Core)
    }

    fn clean_worktrees(&self) -> CliResult<serde_json::Value> {
        facade::clean_worktrees(&self.project).map_err(CliError::Core)
    }

    fn delete_worktree(&self, id: &str) -> CliResult<()> {
        facade::delete_worktree(&self.project, id).map_err(CliError::Core)
    }

    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value> {
        facade::delete_all_worktrees(&self.project).map_err(CliError::Core)
    }

    fn run_cleanup(
        &self,
        project: bool,
        conversation: bool,
        dry_run: bool,
        yes: bool,
        force: bool,
    ) -> CliResult<serde_json::Value> {
        facade::run_cleanup(&self.project, project, conversation, dry_run, yes, force)
            .map_err(CliError::Core)
    }

    fn run_gc(&self, dry_run: bool) -> CliResult<serde_json::Value> {
        facade::run_gc(&self.project, &self.workspace_root, dry_run).map_err(CliError::Core)
    }

    fn get_run(&self, run_id: &str) -> CliResult<Option<grove_core::orchestrator::RunRecord>> {
        facade::get_run(&self.project, run_id).map_err(CliError::Core)
    }

    fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult> {
        let input = facade::StartRunInput {
            objective: req.objective,
            pipeline: req.pipeline,
            model: req.model,
            permission_mode: req.permission_mode,
            conversation_id: req.conversation_id,
        };
        let out = facade::start_run(&self.workspace_root, input).map_err(CliError::Core)?;
        Ok(RunResult {
            run_id: out.run_id,
            task_id: out.task_id,
            state: out.state,
            objective: out.objective,
        })
    }

    fn list_ownership_locks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        facade::list_ownership_locks(&self.workspace_root, run_id).map_err(CliError::Core)
    }

    fn list_merge_queue(&self, conversation_id: &str) -> CliResult<Vec<serde_json::Value>> {
        facade::list_merge_queue(&self.workspace_root, conversation_id).map_err(CliError::Core)
    }

    fn retry_publish_run(&self, run_id: &str) -> CliResult<()> {
        facade::retry_publish_run(&self.workspace_root, run_id).map_err(CliError::Core)
    }
}
