pub mod direct;
pub mod socket;

#[cfg(test)]
use crate::error::CliError;
use crate::error::CliResult;

// ── Verified grove-core type paths ──────────────────────────────────────────
// grove_core::orchestrator::RunRecord    — orchestrator/mod.rs
// grove_core::orchestrator::TaskRecord   — orchestrator/mod.rs
// grove_core::GroveError                 — re-exported via lib.rs
// grove_core::db::repositories::workspaces_repo::WorkspaceRow
// grove_core::db::repositories::projects_repo::ProjectRow
// grove_core::db::repositories::conversations_repo::ConversationRow
//
// Transport trait: each method is implemented by:
//   1. DirectTransport   — in-process grove-core call
//   2. SocketTransport   — JSON-RPC over Unix domain socket
//   3. TestTransport     — test double (#[cfg(test)] only)

/// Parameters for the `run` command — queues a task and returns its initial state.
pub struct StartRunRequest {
    pub objective: String,
    pub pipeline: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub conversation_id: Option<String>,
    /// Reserved for future use (continue from last session).
    #[allow(dead_code)]
    pub continue_last: bool,
    /// Reserved for future use (link to an issue).
    #[allow(dead_code)]
    pub issue_id: Option<String>,
    /// Reserved for future use (limit parallel agent count).
    #[allow(dead_code)]
    pub max_agents: Option<u16>,
}

/// Result returned by `start_run` — sourced from the newly-queued TaskRecord.
pub struct RunResult {
    pub run_id: String,
    #[allow(dead_code)]
    pub task_id: String,
    pub state: String,
    pub objective: String,
}

/// Sync transport trait. Grows as commands are implemented in Tasks 8–15.
/// ⚠️  list_tasks() has NO limit — apply limit client-side in the command handler.
pub trait Transport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>>;
    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>>;
    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>>;
    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>>;
    fn list_conversations(
        &self,
        limit: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>>;
    fn list_issues(&self, cached: bool) -> CliResult<Vec<serde_json::Value>>;
    fn get_issue(&self, id: &str) -> CliResult<serde_json::Value>;
    fn create_issue(
        &self,
        title: &str,
        body: Option<&str>,
        labels: Vec<String>,
        priority: Option<i64>,
    ) -> CliResult<serde_json::Value>;
    fn close_issue(&self, id: &str) -> CliResult<()>;
    fn search_issues(
        &self,
        query: &str,
        limit: i64,
        provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>>;
    fn sync_issues(&self, provider: Option<&str>, full: bool) -> CliResult<serde_json::Value>;

    // ── Task 8 mutation methods ──────────────────────────────────────────────
    fn queue_task(
        &self,
        objective: &str,
        priority: i64,
        model: Option<&str>,
        conversation_id: Option<&str>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<grove_core::orchestrator::TaskRecord>;
    fn cancel_task(&self, task_id: &str) -> CliResult<()>;
    fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult>;
    fn drain_queue(&self, project: &std::path::Path) -> CliResult<()>;

    // ── Task 9 read/mutation methods ─────────────────────────────────────────
    fn get_logs(&self, run_id: &str, all: bool) -> CliResult<Vec<serde_json::Value>>;
    fn get_report(&self, run_id: &str) -> CliResult<serde_json::Value>;
    fn get_plan(&self, run_id: Option<&str>) -> CliResult<serde_json::Value>;
    fn get_subtasks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>>;
    fn get_sessions(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>>;
    fn abort_run(&self, run_id: &str) -> CliResult<()>;
    fn resume_run(&self, run_id: &str) -> CliResult<()>;

    // ── Task 11 auth + llm methods ───────────────────────────────────────────
    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>>;
    fn set_api_key(&self, provider: &str, key: &str) -> CliResult<()>;
    fn remove_api_key(&self, provider: &str) -> CliResult<()>;
    fn list_models(&self, provider: &str) -> CliResult<Vec<serde_json::Value>>;
    fn select_llm(&self, provider: &str, model: Option<&str>) -> CliResult<()>;

    // ── Task 13 issue mutation methods ──────────────────────────────────────
    fn update_issue(
        &self,
        id: &str,
        title: Option<&str>,
        status: Option<&str>,
        label: Option<&str>,
        assignee: Option<&str>,
        priority: Option<&str>,
    ) -> CliResult<serde_json::Value>;
    fn comment_issue(&self, id: &str, body: &str) -> CliResult<serde_json::Value>;
    fn assign_issue(&self, id: &str, assignee: &str) -> CliResult<()>;
    fn move_issue(&self, id: &str, status: &str) -> CliResult<()>;
    fn reopen_issue(&self, id: &str) -> CliResult<()>;
    fn activity_issue(&self, id: &str) -> CliResult<Vec<serde_json::Value>>;
    fn push_issue(&self, id: &str, provider: &str) -> CliResult<serde_json::Value>;
    /// Mark an issue as ready for review.
    /// Pass `"current"` as `id` to resolve the current branch's linked issue server-side.
    fn issue_ready(&self, id: &str) -> CliResult<serde_json::Value>;
    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>>;
    fn connect_provider(
        &self,
        provider: &str,
        token: Option<&str>,
        site: Option<&str>,
        email: Option<&str>,
    ) -> CliResult<()>;
    fn disconnect_provider(&self, provider: &str) -> CliResult<()>;
    fn run_lint(&self, fix: bool, model: Option<&str>) -> CliResult<serde_json::Value>;
    fn run_ci(
        &self,
        branch: Option<&str>,
        wait: bool,
        timeout: Option<u64>,
        fix: bool,
        model: Option<&str>,
    ) -> CliResult<serde_json::Value>;

    // ── Task 14 workspace mutation methods ───────────────────────────────────
    fn set_workspace_name(&self, name: &str) -> CliResult<()>;
    fn archive_workspace(&self, id: &str) -> CliResult<()>;
    fn delete_workspace(&self, id: &str) -> CliResult<()>;

    // ── Task 14 project read/mutation methods ─────────────────────────────
    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>>;
    fn set_project_name(&self, name: &str) -> CliResult<()>;
    fn set_project_settings(
        &self,
        provider: Option<&str>,
        parallel: Option<i64>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<()>;
    fn archive_project(&self, id: Option<&str>) -> CliResult<()>;
    fn delete_project(&self, id: Option<&str>) -> CliResult<()>;

    // ── Task 14 conversation read/mutation methods ─────────────────────────
    fn get_conversation(
        &self,
        id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>>;
    fn archive_conversation(&self, id: &str) -> CliResult<()>;
    fn delete_conversation(&self, id: &str) -> CliResult<()>;
    fn rebase_conversation(&self, id: &str) -> CliResult<()>;
    fn merge_conversation(&self, id: &str) -> CliResult<()>;

    // ── Task 15 signal methods ────────────────────────────────────────────────
    fn send_signal(
        &self,
        run_id: &str,
        from: &str,
        to: &str,
        signal_type: &str,
        payload: Option<&str>,
        priority: Option<i64>,
    ) -> CliResult<()>;
    fn check_signals(&self, run_id: &str, agent: &str) -> CliResult<Vec<serde_json::Value>>;
    fn list_signals(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>>;

    // ── Task 15 hook methods ──────────────────────────────────────────────────
    fn run_hook(
        &self,
        event: &str,
        agent_type: Option<&str>,
        run_id: Option<&str>,
        session_id: Option<&str>,
        tool: Option<&str>,
        file_path: Option<&str>,
    ) -> CliResult<()>;

    // ── Task 15 worktree methods ──────────────────────────────────────────────
    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>>;
    fn clean_worktrees(&self) -> CliResult<serde_json::Value>;
    fn delete_worktree(&self, id: &str) -> CliResult<()>;
    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value>;

    // ── Task 16 TUI run-watch method ──────────────────────────────────────────
    /// Fetch a single run by id. Returns `Ok(None)` when the run does not exist.
    fn get_run(&self, run_id: &str) -> CliResult<Option<grove_core::orchestrator::RunRecord>>;

    // ── Task 15 cleanup/gc methods ────────────────────────────────────────────
    fn run_cleanup(
        &self,
        project: bool,
        conversation: bool,
        dry_run: bool,
        yes: bool,
        force: bool,
    ) -> CliResult<serde_json::Value>;
    fn run_gc(&self, dry_run: bool) -> CliResult<serde_json::Value>;

    // ── Ownership locks, merge queue, publish retry ───────────────────────────
    fn list_ownership_locks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>>;
    fn list_merge_queue(&self, conversation_id: &str) -> CliResult<Vec<serde_json::Value>>;
    fn retry_publish_run(&self, run_id: &str) -> CliResult<()>;
}

/// Runtime transport — auto-detects socket vs direct at startup.
#[allow(dead_code)] // variants wired in Task 6
pub enum GroveTransport {
    Direct(direct::DirectTransport),
    Socket(socket::SocketTransport),
    #[cfg(test)]
    Test(TestTransport),
}

impl GroveTransport {
    #[allow(dead_code)] // called from Task 6 dispatch
    pub fn detect(project: &std::path::Path) -> Self {
        let local_sock = project.join(".grove/grove.sock");
        let global_sock = dirs::home_dir()
            .map(|h| h.join(".grove/grove.sock"))
            .unwrap_or_default();
        if local_sock.exists() || global_sock.exists() {
            let sock = if local_sock.exists() {
                local_sock
            } else {
                global_sock
            };
            GroveTransport::Socket(socket::SocketTransport::new(sock))
        } else {
            GroveTransport::Direct(direct::DirectTransport::new(project))
        }
    }
}

impl Transport for GroveTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        match self {
            GroveTransport::Direct(t) => t.list_runs(limit),
            GroveTransport::Socket(t) => t.list_runs(limit),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_runs(limit),
        }
    }

    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        match self {
            GroveTransport::Direct(t) => t.list_tasks(),
            GroveTransport::Socket(t) => t.list_tasks(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_tasks(),
        }
    }

    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        match self {
            GroveTransport::Direct(t) => t.get_workspace(),
            GroveTransport::Socket(t) => t.get_workspace(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_workspace(),
        }
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        match self {
            GroveTransport::Direct(t) => t.list_projects(),
            GroveTransport::Socket(t) => t.list_projects(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_projects(),
        }
    }

    fn list_conversations(
        &self,
        limit: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        match self {
            GroveTransport::Direct(t) => t.list_conversations(limit),
            GroveTransport::Socket(t) => t.list_conversations(limit),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_conversations(limit),
        }
    }

    fn list_issues(&self, cached: bool) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_issues(cached),
            GroveTransport::Socket(t) => t.list_issues(cached),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_issues(cached),
        }
    }

    fn get_issue(&self, id: &str) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.get_issue(id),
            GroveTransport::Socket(t) => t.get_issue(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_issue(id),
        }
    }

    fn create_issue(
        &self,
        title: &str,
        body: Option<&str>,
        labels: Vec<String>,
        priority: Option<i64>,
    ) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.create_issue(title, body, labels, priority),
            GroveTransport::Socket(t) => t.create_issue(title, body, labels, priority),
            #[cfg(test)]
            GroveTransport::Test(t) => t.create_issue(title, body, labels, priority),
        }
    }

    fn close_issue(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.close_issue(id),
            GroveTransport::Socket(t) => t.close_issue(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.close_issue(id),
        }
    }

    fn search_issues(
        &self,
        query: &str,
        limit: i64,
        provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.search_issues(query, limit, provider),
            GroveTransport::Socket(t) => t.search_issues(query, limit, provider),
            #[cfg(test)]
            GroveTransport::Test(t) => t.search_issues(query, limit, provider),
        }
    }

    fn sync_issues(&self, provider: Option<&str>, full: bool) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.sync_issues(provider, full),
            GroveTransport::Socket(t) => t.sync_issues(provider, full),
            #[cfg(test)]
            GroveTransport::Test(t) => t.sync_issues(provider, full),
        }
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
        match self {
            GroveTransport::Direct(t) => t.queue_task(
                objective,
                priority,
                model,
                conversation_id,
                pipeline,
                permission_mode,
            ),
            GroveTransport::Socket(t) => t.queue_task(
                objective,
                priority,
                model,
                conversation_id,
                pipeline,
                permission_mode,
            ),
            #[cfg(test)]
            GroveTransport::Test(t) => t.queue_task(
                objective,
                priority,
                model,
                conversation_id,
                pipeline,
                permission_mode,
            ),
        }
    }

    fn cancel_task(&self, task_id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.cancel_task(task_id),
            GroveTransport::Socket(t) => t.cancel_task(task_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.cancel_task(task_id),
        }
    }

    fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult> {
        match self {
            GroveTransport::Direct(t) => t.start_run(req),
            GroveTransport::Socket(t) => t.start_run(req),
            #[cfg(test)]
            GroveTransport::Test(t) => t.start_run(req),
        }
    }

    fn drain_queue(&self, project: &std::path::Path) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.drain_queue(project),
            GroveTransport::Socket(t) => t.drain_queue(project),
            #[cfg(test)]
            GroveTransport::Test(t) => t.drain_queue(project),
        }
    }

    fn get_logs(&self, run_id: &str, all: bool) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.get_logs(run_id, all),
            GroveTransport::Socket(t) => t.get_logs(run_id, all),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_logs(run_id, all),
        }
    }

    fn get_report(&self, run_id: &str) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.get_report(run_id),
            GroveTransport::Socket(t) => t.get_report(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_report(run_id),
        }
    }

    fn get_plan(&self, run_id: Option<&str>) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.get_plan(run_id),
            GroveTransport::Socket(t) => t.get_plan(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_plan(run_id),
        }
    }

    fn get_subtasks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.get_subtasks(run_id),
            GroveTransport::Socket(t) => t.get_subtasks(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_subtasks(run_id),
        }
    }

    fn get_sessions(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.get_sessions(run_id),
            GroveTransport::Socket(t) => t.get_sessions(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_sessions(run_id),
        }
    }

    fn abort_run(&self, run_id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.abort_run(run_id),
            GroveTransport::Socket(t) => t.abort_run(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.abort_run(run_id),
        }
    }

    fn resume_run(&self, run_id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.resume_run(run_id),
            GroveTransport::Socket(t) => t.resume_run(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.resume_run(run_id),
        }
    }

    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_providers(),
            GroveTransport::Socket(t) => t.list_providers(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_providers(),
        }
    }

    fn set_api_key(&self, provider: &str, key: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.set_api_key(provider, key),
            GroveTransport::Socket(t) => t.set_api_key(provider, key),
            #[cfg(test)]
            GroveTransport::Test(t) => t.set_api_key(provider, key),
        }
    }

    fn remove_api_key(&self, provider: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.remove_api_key(provider),
            GroveTransport::Socket(t) => t.remove_api_key(provider),
            #[cfg(test)]
            GroveTransport::Test(t) => t.remove_api_key(provider),
        }
    }

    fn list_models(&self, provider: &str) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_models(provider),
            GroveTransport::Socket(t) => t.list_models(provider),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_models(provider),
        }
    }

    fn select_llm(&self, provider: &str, model: Option<&str>) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.select_llm(provider, model),
            GroveTransport::Socket(t) => t.select_llm(provider, model),
            #[cfg(test)]
            GroveTransport::Test(t) => t.select_llm(provider, model),
        }
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
        match self {
            GroveTransport::Direct(t) => {
                t.update_issue(id, title, status, label, assignee, priority)
            }
            GroveTransport::Socket(t) => {
                t.update_issue(id, title, status, label, assignee, priority)
            }
            #[cfg(test)]
            GroveTransport::Test(t) => t.update_issue(id, title, status, label, assignee, priority),
        }
    }

    fn comment_issue(&self, id: &str, body: &str) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.comment_issue(id, body),
            GroveTransport::Socket(t) => t.comment_issue(id, body),
            #[cfg(test)]
            GroveTransport::Test(t) => t.comment_issue(id, body),
        }
    }

    fn assign_issue(&self, id: &str, assignee: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.assign_issue(id, assignee),
            GroveTransport::Socket(t) => t.assign_issue(id, assignee),
            #[cfg(test)]
            GroveTransport::Test(t) => t.assign_issue(id, assignee),
        }
    }

    fn move_issue(&self, id: &str, status: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.move_issue(id, status),
            GroveTransport::Socket(t) => t.move_issue(id, status),
            #[cfg(test)]
            GroveTransport::Test(t) => t.move_issue(id, status),
        }
    }

    fn reopen_issue(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.reopen_issue(id),
            GroveTransport::Socket(t) => t.reopen_issue(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.reopen_issue(id),
        }
    }

    fn activity_issue(&self, id: &str) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.activity_issue(id),
            GroveTransport::Socket(t) => t.activity_issue(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.activity_issue(id),
        }
    }

    fn push_issue(&self, id: &str, provider: &str) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.push_issue(id, provider),
            GroveTransport::Socket(t) => t.push_issue(id, provider),
            #[cfg(test)]
            GroveTransport::Test(t) => t.push_issue(id, provider),
        }
    }

    fn issue_ready(&self, id: &str) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.issue_ready(id),
            GroveTransport::Socket(t) => t.issue_ready(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.issue_ready(id),
        }
    }

    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.connect_status(),
            GroveTransport::Socket(t) => t.connect_status(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.connect_status(),
        }
    }

    fn connect_provider(
        &self,
        provider: &str,
        token: Option<&str>,
        site: Option<&str>,
        email: Option<&str>,
    ) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.connect_provider(provider, token, site, email),
            GroveTransport::Socket(t) => t.connect_provider(provider, token, site, email),
            #[cfg(test)]
            GroveTransport::Test(t) => t.connect_provider(provider, token, site, email),
        }
    }

    fn disconnect_provider(&self, provider: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.disconnect_provider(provider),
            GroveTransport::Socket(t) => t.disconnect_provider(provider),
            #[cfg(test)]
            GroveTransport::Test(t) => t.disconnect_provider(provider),
        }
    }

    fn run_lint(&self, fix: bool, model: Option<&str>) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.run_lint(fix, model),
            GroveTransport::Socket(t) => t.run_lint(fix, model),
            #[cfg(test)]
            GroveTransport::Test(t) => t.run_lint(fix, model),
        }
    }

    fn run_ci(
        &self,
        branch: Option<&str>,
        wait: bool,
        timeout: Option<u64>,
        fix: bool,
        model: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.run_ci(branch, wait, timeout, fix, model),
            GroveTransport::Socket(t) => t.run_ci(branch, wait, timeout, fix, model),
            #[cfg(test)]
            GroveTransport::Test(t) => t.run_ci(branch, wait, timeout, fix, model),
        }
    }

    fn set_workspace_name(&self, name: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.set_workspace_name(name),
            GroveTransport::Socket(t) => t.set_workspace_name(name),
            #[cfg(test)]
            GroveTransport::Test(t) => t.set_workspace_name(name),
        }
    }

    fn archive_workspace(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.archive_workspace(id),
            GroveTransport::Socket(t) => t.archive_workspace(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.archive_workspace(id),
        }
    }

    fn delete_workspace(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.delete_workspace(id),
            GroveTransport::Socket(t) => t.delete_workspace(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.delete_workspace(id),
        }
    }

    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>> {
        match self {
            GroveTransport::Direct(t) => t.get_project(),
            GroveTransport::Socket(t) => t.get_project(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_project(),
        }
    }

    fn set_project_name(&self, name: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.set_project_name(name),
            GroveTransport::Socket(t) => t.set_project_name(name),
            #[cfg(test)]
            GroveTransport::Test(t) => t.set_project_name(name),
        }
    }

    fn set_project_settings(
        &self,
        provider: Option<&str>,
        parallel: Option<i64>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => {
                t.set_project_settings(provider, parallel, pipeline, permission_mode)
            }
            GroveTransport::Socket(t) => {
                t.set_project_settings(provider, parallel, pipeline, permission_mode)
            }
            #[cfg(test)]
            GroveTransport::Test(t) => {
                t.set_project_settings(provider, parallel, pipeline, permission_mode)
            }
        }
    }

    fn archive_project(&self, id: Option<&str>) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.archive_project(id),
            GroveTransport::Socket(t) => t.archive_project(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.archive_project(id),
        }
    }

    fn delete_project(&self, id: Option<&str>) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.delete_project(id),
            GroveTransport::Socket(t) => t.delete_project(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.delete_project(id),
        }
    }

    fn get_conversation(
        &self,
        id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        match self {
            GroveTransport::Direct(t) => t.get_conversation(id),
            GroveTransport::Socket(t) => t.get_conversation(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_conversation(id),
        }
    }

    fn archive_conversation(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.archive_conversation(id),
            GroveTransport::Socket(t) => t.archive_conversation(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.archive_conversation(id),
        }
    }

    fn delete_conversation(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.delete_conversation(id),
            GroveTransport::Socket(t) => t.delete_conversation(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.delete_conversation(id),
        }
    }

    fn rebase_conversation(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.rebase_conversation(id),
            GroveTransport::Socket(t) => t.rebase_conversation(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.rebase_conversation(id),
        }
    }

    fn merge_conversation(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.merge_conversation(id),
            GroveTransport::Socket(t) => t.merge_conversation(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.merge_conversation(id),
        }
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
        match self {
            GroveTransport::Direct(t) => {
                t.send_signal(run_id, from, to, signal_type, payload, priority)
            }
            GroveTransport::Socket(t) => {
                t.send_signal(run_id, from, to, signal_type, payload, priority)
            }
            #[cfg(test)]
            GroveTransport::Test(t) => {
                t.send_signal(run_id, from, to, signal_type, payload, priority)
            }
        }
    }

    fn check_signals(&self, run_id: &str, agent: &str) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.check_signals(run_id, agent),
            GroveTransport::Socket(t) => t.check_signals(run_id, agent),
            #[cfg(test)]
            GroveTransport::Test(t) => t.check_signals(run_id, agent),
        }
    }

    fn list_signals(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_signals(run_id),
            GroveTransport::Socket(t) => t.list_signals(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_signals(run_id),
        }
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
        match self {
            GroveTransport::Direct(t) => {
                t.run_hook(event, agent_type, run_id, session_id, tool, file_path)
            }
            GroveTransport::Socket(t) => {
                t.run_hook(event, agent_type, run_id, session_id, tool, file_path)
            }
            #[cfg(test)]
            GroveTransport::Test(t) => {
                t.run_hook(event, agent_type, run_id, session_id, tool, file_path)
            }
        }
    }

    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_worktrees(),
            GroveTransport::Socket(t) => t.list_worktrees(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_worktrees(),
        }
    }

    fn clean_worktrees(&self) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.clean_worktrees(),
            GroveTransport::Socket(t) => t.clean_worktrees(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.clean_worktrees(),
        }
    }

    fn delete_worktree(&self, id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.delete_worktree(id),
            GroveTransport::Socket(t) => t.delete_worktree(id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.delete_worktree(id),
        }
    }

    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.delete_all_worktrees(),
            GroveTransport::Socket(t) => t.delete_all_worktrees(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.delete_all_worktrees(),
        }
    }

    fn run_cleanup(
        &self,
        project: bool,
        conversation: bool,
        dry_run: bool,
        yes: bool,
        force: bool,
    ) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.run_cleanup(project, conversation, dry_run, yes, force),
            GroveTransport::Socket(t) => t.run_cleanup(project, conversation, dry_run, yes, force),
            #[cfg(test)]
            GroveTransport::Test(t) => t.run_cleanup(project, conversation, dry_run, yes, force),
        }
    }

    fn run_gc(&self, dry_run: bool) -> CliResult<serde_json::Value> {
        match self {
            GroveTransport::Direct(t) => t.run_gc(dry_run),
            GroveTransport::Socket(t) => t.run_gc(dry_run),
            #[cfg(test)]
            GroveTransport::Test(t) => t.run_gc(dry_run),
        }
    }

    fn get_run(&self, run_id: &str) -> CliResult<Option<grove_core::orchestrator::RunRecord>> {
        match self {
            GroveTransport::Direct(t) => t.get_run(run_id),
            GroveTransport::Socket(t) => t.get_run(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.get_run(run_id),
        }
    }

    fn list_ownership_locks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_ownership_locks(run_id),
            GroveTransport::Socket(t) => t.list_ownership_locks(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_ownership_locks(run_id),
        }
    }

    fn list_merge_queue(&self, conversation_id: &str) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_merge_queue(conversation_id),
            GroveTransport::Socket(t) => t.list_merge_queue(conversation_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_merge_queue(conversation_id),
        }
    }

    fn retry_publish_run(&self, run_id: &str) -> CliResult<()> {
        match self {
            GroveTransport::Direct(t) => t.retry_publish_run(run_id),
            GroveTransport::Socket(t) => t.retry_publish_run(run_id),
            #[cfg(test)]
            GroveTransport::Test(t) => t.retry_publish_run(run_id),
        }
    }
}

/// Test-only in-memory transport — all methods return empty/default.
#[cfg(test)]
#[derive(Default)]
pub struct TestTransport;

#[cfg(test)]
impl Transport for TestTransport {
    fn list_runs(&self, _: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        Ok(vec![])
    }

    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        Ok(vec![])
    }

    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        Ok(None)
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        Ok(vec![])
    }

    fn list_conversations(
        &self,
        _: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        Ok(vec![])
    }

    fn list_issues(&self, _cached: bool) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn get_issue(&self, _id: &str) -> CliResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    fn create_issue(
        &self,
        _title: &str,
        _body: Option<&str>,
        _labels: Vec<String>,
        _priority: Option<i64>,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not implemented".into()))
    }

    fn close_issue(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn search_issues(
        &self,
        _query: &str,
        _limit: i64,
        _provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn sync_issues(&self, _provider: Option<&str>, _full: bool) -> CliResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
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
        Err(CliError::Other("not implemented".into()))
    }

    fn cancel_task(&self, _task_id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn start_run(&self, _req: StartRunRequest) -> CliResult<RunResult> {
        Err(CliError::Other("not implemented".into()))
    }

    fn drain_queue(&self, _project: &std::path::Path) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn get_logs(&self, _run_id: &str, _all: bool) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn get_report(&self, _run_id: &str) -> CliResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    fn get_plan(&self, _run_id: Option<&str>) -> CliResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    fn get_subtasks(&self, _run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn get_sessions(&self, _run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn abort_run(&self, _run_id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn resume_run(&self, _run_id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn set_api_key(&self, _provider: &str, _key: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn remove_api_key(&self, _provider: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn list_models(&self, _provider: &str) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn select_llm(&self, _provider: &str, _model: Option<&str>) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
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
        Err(CliError::Other("not implemented".into()))
    }

    fn comment_issue(&self, _id: &str, _body: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not implemented".into()))
    }

    fn assign_issue(&self, _id: &str, _assignee: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn move_issue(&self, _id: &str, _status: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn reopen_issue(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn activity_issue(&self, _id: &str) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn push_issue(&self, _id: &str, _provider: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not implemented".into()))
    }

    fn issue_ready(&self, _id: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not implemented".into()))
    }

    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn connect_provider(
        &self,
        _provider: &str,
        _token: Option<&str>,
        _site: Option<&str>,
        _email: Option<&str>,
    ) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn disconnect_provider(&self, _provider: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn run_lint(&self, _fix: bool, _model: Option<&str>) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not implemented".into()))
    }

    fn run_ci(
        &self,
        _branch: Option<&str>,
        _wait: bool,
        _timeout: Option<u64>,
        _fix: bool,
        _model: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not implemented".into()))
    }

    fn set_workspace_name(&self, _name: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn archive_workspace(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn delete_workspace(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>> {
        Ok(None)
    }

    fn set_project_name(&self, _name: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn set_project_settings(
        &self,
        _provider: Option<&str>,
        _parallel: Option<i64>,
        _pipeline: Option<&str>,
        _permission_mode: Option<&str>,
    ) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn archive_project(&self, _id: Option<&str>) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn delete_project(&self, _id: Option<&str>) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn get_conversation(
        &self,
        _id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        Ok(None)
    }

    fn archive_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn delete_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn rebase_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn merge_conversation(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
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
        Err(CliError::Other("not implemented".into()))
    }

    fn check_signals(&self, _run_id: &str, _agent: &str) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn list_signals(&self, _run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
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
        Ok(())
    }

    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn clean_worktrees(&self) -> CliResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    fn delete_worktree(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }

    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not implemented".into()))
    }

    fn run_cleanup(
        &self,
        _project: bool,
        _conversation: bool,
        _dry_run: bool,
        _yes: bool,
        _force: bool,
    ) -> CliResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    fn run_gc(&self, _dry_run: bool) -> CliResult<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }

    fn get_run(&self, _run_id: &str) -> CliResult<Option<grove_core::orchestrator::RunRecord>> {
        Ok(None)
    }

    fn list_ownership_locks(&self, _run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn list_merge_queue(&self, _conversation_id: &str) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    fn retry_publish_run(&self, _run_id: &str) -> CliResult<()> {
        Err(CliError::Other("not implemented".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_list_runs_returns_empty() {
        let t = TestTransport;
        assert!(t.list_runs(10).unwrap().is_empty());
    }
}
