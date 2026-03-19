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
// Transport trait growth: each task (8–15) ADDS mutation methods AND updates:
//   1. DirectTransport impl   — real grove-core call
//   2. SocketTransport impl   — socket stub (Err for now)
//   3. TestTransport impl     — Ok(default) or Err as appropriate

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
    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>>;

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
    // Tasks 12–15 add more methods here. Update all 3 impls + TestTransport each time.
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

    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_issues(),
            GroveTransport::Socket(t) => t.list_issues(),
            #[cfg(test)]
            GroveTransport::Test(t) => t.list_issues(),
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

    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> {
        Ok(vec![])
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_list_runs_returns_empty() {
        let t = TestTransport::default();
        assert!(t.list_runs(10).unwrap().is_empty());
    }
}
