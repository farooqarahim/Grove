use std::path::PathBuf;

use super::{RunResult, StartRunRequest, Transport};
use crate::error::{CliError, CliResult};

pub struct SocketTransport {
    sock_path: PathBuf,
}

impl SocketTransport {
    #[allow(dead_code)] // called from GroveTransport::detect (Task 6)
    pub fn new(sock_path: PathBuf) -> Self {
        Self { sock_path }
    }

    /// Stub: send a JSON-RPC request over the Unix socket. Implemented in Task 15.
    #[allow(dead_code)]
    fn call(&self, method: &str, params: serde_json::Value) -> CliResult<serde_json::Value> {
        let _ = (method, params, &self.sock_path);
        Err(CliError::Transport(
            "socket transport not yet implemented".into(),
        ))
    }
}

impl Transport for SocketTransport {
    fn list_runs(&self, _limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_conversations(
        &self,
        _: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_issues(&self, _cached: bool) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_issue(&self, _id: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn create_issue(
        &self,
        _title: &str,
        _body: Option<&str>,
        _labels: Vec<String>,
        _priority: Option<i64>,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn close_issue(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn search_issues(
        &self,
        _query: &str,
        _limit: i64,
        _provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn sync_issues(&self, _provider: Option<&str>, _full: bool) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn queue_task(
        &self,
        _objective: &str,
        _priority: i64,
        _model: Option<&str>,
        _conversation_id: Option<&str>,
        _pipeline: Option<&str>,
        _permission_mode: Option<&str>,
    ) -> CliResult<grove_core::orchestrator::TaskRecord> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn cancel_task(&self, _task_id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn start_run(&self, _req: StartRunRequest) -> CliResult<RunResult> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn drain_queue(&self, _project: &std::path::Path) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_logs(&self, _run_id: &str, _all: bool) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_report(&self, _run_id: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_plan(&self, _run_id: Option<&str>) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_subtasks(&self, _run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_sessions(&self, _run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn abort_run(&self, _run_id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn resume_run(&self, _run_id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn set_api_key(&self, _provider: &str, _key: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn remove_api_key(&self, _provider: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_models(&self, _provider: &str) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn select_llm(&self, _provider: &str, _model: Option<&str>) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn update_issue(
        &self,
        _id: &str,
        _title: Option<&str>,
        _status: Option<&str>,
        _label: Option<&str>,
        _assignee: Option<&str>,
        _priority: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn comment_issue(&self, _id: &str, _body: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn assign_issue(&self, _id: &str, _assignee: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn move_issue(&self, _id: &str, _status: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn reopen_issue(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn activity_issue(&self, _id: &str) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn push_issue(&self, _id: &str, _provider: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn issue_ready(&self, _id: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn connect_provider(
        &self,
        _provider: &str,
        _token: Option<&str>,
        _site: Option<&str>,
        _email: Option<&str>,
    ) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn disconnect_provider(&self, _provider: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn run_lint(&self, _fix: bool, _model: Option<&str>) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn run_ci(
        &self,
        _branch: Option<&str>,
        _wait: bool,
        _timeout: Option<u64>,
        _fix: bool,
        _model: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn set_workspace_name(&self, _name: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn archive_workspace(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn delete_workspace(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn set_project_name(&self, _name: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn set_project_settings(
        &self,
        _provider: Option<&str>,
        _parallel: Option<i64>,
        _pipeline: Option<&str>,
        _permission_mode: Option<&str>,
    ) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn archive_project(&self, _id: Option<&str>) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn delete_project(&self, _id: Option<&str>) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_conversation(
        &self,
        _id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn archive_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn delete_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn rebase_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn merge_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn send_signal(
        &self,
        _run_id: &str,
        _from: &str,
        _to: &str,
        _signal_type: &str,
        _payload: Option<&str>,
        _priority: Option<i64>,
    ) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn check_signals(&self, _run_id: &str, _agent: &str) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_signals(&self, _run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn run_hook(
        &self,
        _event: &str,
        _agent_type: Option<&str>,
        _run_id: Option<&str>,
        _session_id: Option<&str>,
        _tool: Option<&str>,
        _file_path: Option<&str>,
    ) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn clean_worktrees(&self) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn delete_worktree(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn run_cleanup(
        &self,
        _project: bool,
        _conversation: bool,
        _dry_run: bool,
        _yes: bool,
        _force: bool,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn run_gc(&self, _dry_run: bool) -> CliResult<serde_json::Value> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }

    fn get_run(&self, _run_id: &str) -> CliResult<Option<grove_core::orchestrator::RunRecord>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }
}
