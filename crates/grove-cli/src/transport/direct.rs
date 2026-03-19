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
        match grove_core::orchestrator::get_workspace(&self.project) {
            Ok(row) => Ok(Some(row)),
            Err(grove_core::GroveError::NotFound(_)) => Ok(None),
            Err(e) => Err(CliError::Core(e)),
        }
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        grove_core::orchestrator::list_projects(&self.project).map_err(CliError::Core)
    }

    fn list_conversations(
        &self,
        limit: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        grove_core::orchestrator::list_conversations(&self.project, limit).map_err(CliError::Core)
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

    fn set_workspace_name(&self, name: &str) -> CliResult<()> {
        grove_core::orchestrator::update_workspace_name(&self.project, name).map_err(CliError::Core)
    }

    fn archive_workspace(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::archive_workspace(&self.project, id).map_err(CliError::Core)
    }

    fn delete_workspace(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::delete_workspace(&self.project, id).map_err(CliError::Core)
    }

    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>> {
        match grove_core::orchestrator::get_project(&self.project) {
            Ok(row) => Ok(Some(row)),
            Err(grove_core::GroveError::NotFound(_)) => Ok(None),
            Err(e) => Err(CliError::Core(e)),
        }
    }

    fn set_project_name(&self, name: &str) -> CliResult<()> {
        // Resolve the current project id then rename it.
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        grove_core::orchestrator::update_project_name(&self.project, &project.id, name)
            .map_err(CliError::Core)
    }

    fn set_project_settings(
        &self,
        provider: Option<&str>,
        parallel: Option<i64>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<()> {
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        let mut settings =
            grove_core::orchestrator::get_project_settings(&self.project, &project.id)
                .map_err(CliError::Core)?;
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
        grove_core::orchestrator::update_project_settings(&self.project, &project.id, &settings)
            .map_err(CliError::Core)
    }

    fn archive_project(&self, id: Option<&str>) -> CliResult<()> {
        let project_id = match id {
            Some(i) => i.to_string(),
            None => {
                grove_core::orchestrator::get_project(&self.project)
                    .map_err(CliError::Core)?
                    .id
            }
        };
        grove_core::orchestrator::archive_project(&self.project, &project_id)
            .map_err(CliError::Core)
    }

    fn delete_project(&self, id: Option<&str>) -> CliResult<()> {
        let project_id = match id {
            Some(i) => i.to_string(),
            None => {
                grove_core::orchestrator::get_project(&self.project)
                    .map_err(CliError::Core)?
                    .id
            }
        };
        grove_core::orchestrator::delete_project(&self.project, &project_id).map_err(CliError::Core)
    }

    fn get_conversation(
        &self,
        id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        match grove_core::orchestrator::get_conversation(&self.project, id) {
            Ok(row) => Ok(Some(row)),
            Err(grove_core::GroveError::NotFound(_)) => Ok(None),
            Err(e) => Err(CliError::Core(e)),
        }
    }

    fn archive_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::archive_conversation(&self.project, id).map_err(CliError::Core)
    }

    fn delete_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::delete_conversation(&self.project, id).map_err(CliError::Core)
    }

    fn rebase_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::rebase_conversation(&self.project, id)
            .map(|_| ())
            .map_err(CliError::Core)
    }

    fn merge_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::merge_conversation(&self.project, id)
            .map(|_| ())
            .map_err(CliError::Core)
    }

    // ── Task 15 signal methods (direct DB access via grove-core) ──────────────

    fn send_signal(
        &self,
        run_id: &str,
        from: &str,
        to: &str,
        signal_type: &str,
        payload: Option<&str>,
        priority: Option<i64>,
    ) -> CliResult<()> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let sig_type = grove_core::signals::SignalType::parse(signal_type)
            .ok_or_else(|| CliError::BadArg(format!("unknown signal type: {signal_type}")))?;
        let sig_priority = priority
            .map(|p| match p {
                i64::MIN..=-1 => grove_core::signals::SignalPriority::Low,
                0 => grove_core::signals::SignalPriority::Normal,
                1 => grove_core::signals::SignalPriority::High,
                _ => grove_core::signals::SignalPriority::Urgent,
            })
            .unwrap_or_default();
        let payload_val: serde_json::Value = payload
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);
        grove_core::signals::send_signal(
            &conn,
            run_id,
            from,
            to,
            sig_type,
            sig_priority,
            payload_val,
        )
        .map(|_| ())
        .map_err(CliError::Core)
    }

    fn check_signals(&self, run_id: &str, agent: &str) -> CliResult<Vec<serde_json::Value>> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let signals =
            grove_core::signals::check_signals(&conn, run_id, agent).map_err(CliError::Core)?;
        signals
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn list_signals(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let signals = grove_core::signals::list_for_run(&conn, run_id).map_err(CliError::Core)?;
        signals
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    // ── Task 15 hook methods ──────────────────────────────────────────────────

    fn run_hook(
        &self,
        _event: &str,
        _agent_type: Option<&str>,
        _run_id: Option<&str>,
        _session_id: Option<&str>,
        _tool: Option<&str>,
        _file_path: Option<&str>,
    ) -> CliResult<()> {
        // Hooks are dispatched by the grove daemon; no direct-mode equivalent yet.
        Err(CliError::Other("not yet available".into()))
    }

    // ── Task 15 worktree methods ──────────────────────────────────────────────

    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Other("not yet available".into()))
    }

    fn clean_worktrees(&self) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn delete_worktree(&self, _id: &str) -> CliResult<()> {
        Err(CliError::Other("not yet available".into()))
    }

    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    // ── Task 15 cleanup/gc methods ────────────────────────────────────────────

    fn run_cleanup(
        &self,
        _project: bool,
        _conversation: bool,
        _dry_run: bool,
        _yes: bool,
        _force: bool,
    ) -> CliResult<serde_json::Value> {
        Err(CliError::Other("not yet available".into()))
    }

    fn run_gc(&self, _dry_run: bool) -> CliResult<serde_json::Value> {
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
