use std::path::{Path, PathBuf};

const DEFAULT_REPORT_RUN_LIMIT: i64 = 50;

use super::{RunResult, StartRunRequest, Transport};
use crate::error::{CliError, CliResult};
use grove_core::llm::{AuthInfo, AuthStore, LlmProviderKind, LlmRouter};

pub struct DirectTransport {
    project: PathBuf,
}

impl DirectTransport {
    #[allow(dead_code)] // called from GroveTransport::detect (Task 6)
    pub fn new(project: &Path) -> Self {
        Self {
            project: project.to_owned(),
        }
    }
}

impl Transport for DirectTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        grove_core::orchestrator::list_runs(&self.project, limit).map_err(CliError::Core)
    }

    // Stubs — will be filled in Tasks 7–15
    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        grove_core::orchestrator::list_tasks(&self.project).map_err(CliError::Core)
    }

    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        Err(CliError::Other("not yet implemented".into()))
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        Err(CliError::Other("not yet implemented".into()))
    }

    fn list_conversations(
        &self,
        _: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        Err(CliError::Other("not yet implemented".into()))
    }

    fn list_issues(&self, _cached: bool) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Other("not yet available".into()))
    }

    fn get_issue(&self, _id: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn create_issue(
        &self,
        _title: &str,
        _body: Option<&str>,
        _labels: Vec<String>,
        _priority: Option<i64>,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn close_issue(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not yet available".into()))
    }

    fn search_issues(
        &self,
        _query: &str,
        _limit: i64,
        _provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Other("not yet available".into()))
    }

    fn sync_issues(&self, _provider: Option<&str>, _full: bool) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
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
        grove_core::orchestrator::queue_task(
            &self.project,
            objective,
            None, // budget_usd
            priority,
            model,
            None, // provider
            conversation_id,
            None, // resume_provider_session_id
            pipeline,
            permission_mode,
            false, // disable_phase_gates
        )
        .map_err(CliError::Core)
    }

    fn cancel_task(&self, task_id: &str) -> CliResult<()> {
        grove_core::orchestrator::cancel_task(&self.project, task_id).map_err(CliError::Core)
    }

    fn drain_queue(&self, _project: &std::path::Path) -> CliResult<()> {
        Err(CliError::Other(
            "drain_queue not available in direct mode".into(),
        ))
    }

    fn get_logs(&self, run_id: &str, all: bool) -> CliResult<Vec<serde_json::Value>> {
        let events = if all {
            grove_core::orchestrator::run_events_all(&self.project, run_id)
        } else {
            grove_core::orchestrator::run_events(&self.project, run_id)
        }
        .map_err(CliError::Core)?;

        events
            .into_iter()
            .map(|e| serde_json::to_value(&e).map_err(|err| CliError::Other(err.to_string())))
            .collect()
    }

    fn get_report(&self, _run_id: &str) -> CliResult<serde_json::Value> {
        // cost_report returns aggregate data across all completed runs, not per-run.
        let report = grove_core::orchestrator::cost_report(&self.project, DEFAULT_REPORT_RUN_LIMIT)
            .map_err(CliError::Core)?;
        serde_json::to_value(&report).map_err(|e| CliError::Other(e.to_string()))
    }

    fn get_plan(&self, run_id: Option<&str>) -> CliResult<serde_json::Value> {
        let rid = run_id.ok_or_else(|| CliError::Other("run_id is required for plan".into()))?;
        let steps = grove_core::orchestrator::list_plan_steps(&self.project, rid)
            .map_err(CliError::Core)?;
        serde_json::to_value(&steps).map_err(|e| CliError::Other(e.to_string()))
    }

    fn get_subtasks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        let rid =
            run_id.ok_or_else(|| CliError::Other("run_id is required for subtasks".into()))?;
        let steps = grove_core::orchestrator::list_plan_steps(&self.project, rid)
            .map_err(CliError::Core)?;
        steps
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn get_sessions(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let sessions = grove_core::orchestrator::list_sessions(&self.project, run_id)
            .map_err(CliError::Core)?;
        sessions
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn abort_run(&self, run_id: &str) -> CliResult<()> {
        grove_core::orchestrator::abort_run(&self.project, run_id).map_err(CliError::Core)
    }

    fn resume_run(&self, run_id: &str) -> CliResult<()> {
        grove_core::orchestrator::resume_run(&self.project, run_id)
            .map(|_| ())
            .map_err(CliError::Core)
    }

    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>> {
        let statuses = LlmRouter::providers();
        statuses
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
                let val = serde_json::json!({
                    "provider": s.kind.id(),
                    "name": s.name,
                    "authenticated": s.authenticated,
                    "key_hint": key_hint,
                    "model_count": s.model_count,
                    "default_model": s.default_model,
                });
                Ok(val)
            })
            .collect()
    }

    fn set_api_key(&self, provider: &str, key: &str) -> CliResult<()> {
        let kind = LlmProviderKind::from_str(provider)
            .ok_or_else(|| CliError::BadArg(format!("unknown provider: {provider}")))?;
        LlmRouter::set_api_key(kind, key).map_err(|e| CliError::Other(e.to_string()))
    }

    fn remove_api_key(&self, provider: &str) -> CliResult<()> {
        let kind = LlmProviderKind::from_str(provider)
            .ok_or_else(|| CliError::BadArg(format!("unknown provider: {provider}")))?;
        LlmRouter::remove_api_key(kind).map_err(|e| CliError::Other(e.to_string()))
    }

    fn list_models(&self, provider: &str) -> CliResult<Vec<serde_json::Value>> {
        let kind = LlmProviderKind::from_str(provider)
            .ok_or_else(|| CliError::BadArg(format!("unknown provider: {provider}")))?;
        let models = LlmRouter::models(kind);
        models
            .iter()
            .map(|m| {
                let val = serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "context_window": m.context_window,
                    "max_output_tokens": m.max_output_tokens,
                    "cost_input_per_m": m.cost_input_per_m,
                    "cost_output_per_m": m.cost_output_per_m,
                    "vision": m.capabilities.vision,
                    "tools": m.capabilities.tools,
                    "reasoning": m.capabilities.reasoning,
                });
                Ok(val)
            })
            .collect()
    }

    fn select_llm(&self, _provider: &str, _model: Option<&str>) -> CliResult<()> {
        // Workspace-level LLM selection requires a DB connection with a workspace_id.
        // That context is not available in direct mode without further scaffolding (Task 14).
        Err(CliError::Other(
            "llm select not yet available in direct mode".into(),
        ))
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
        Err(CliError::Other("not yet available".into()))
    }

    fn comment_issue(&self, _id: &str, _body: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn assign_issue(&self, _id: &str, _assignee: &str) -> CliResult<()> {
        Err(CliError::Other("not yet available".into()))
    }

    fn move_issue(&self, _id: &str, _status: &str) -> CliResult<()> {
        Err(CliError::Other("not yet available".into()))
    }

    fn reopen_issue(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not yet available".into()))
    }

    fn activity_issue(&self, _id: &str) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Other("not yet available".into()))
    }

    fn push_issue(&self, _id: &str, _provider: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn issue_ready(&self, _id: &str) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Other("not yet available".into()))
    }

    fn connect_provider(
        &self,
        _provider: &str,
        _token: Option<&str>,
        _site: Option<&str>,
        _email: Option<&str>,
    ) -> CliResult<()> {
        Err(CliError::Other("not yet available".into()))
    }

    fn disconnect_provider(&self, _provider: &str) -> CliResult<()> {
        Err(CliError::Other("not yet available".into()))
    }

    fn run_lint(&self, _fix: bool, _model: Option<&str>) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn run_ci(
        &self,
        _branch: Option<&str>,
        _wait: bool,
        _timeout: Option<u64>,
        _fix: bool,
        _model: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult> {
        let task = grove_core::orchestrator::queue_task(
            &self.project,
            &req.objective,
            None, // budget_usd
            0,    // priority (default)
            req.model.as_deref(),
            None, // provider
            req.conversation_id.as_deref(),
            None, // resume_provider_session_id
            req.pipeline.as_deref(),
            req.permission_mode.as_deref(),
            false, // disable_phase_gates
        )
        .map_err(CliError::Core)?;

        let task_id = task.id;
        Ok(RunResult {
            run_id: task.run_id.unwrap_or_else(|| task_id.clone()),
            task_id,
            state: task.state,
            objective: task.objective,
        })
    }
}
