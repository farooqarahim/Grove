use std::path::{Path, PathBuf};

use super::{RunResult, StartRunRequest, Transport};
use crate::error::{CliError, CliResult};

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

    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Other("not yet implemented".into()))
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
            .map(|e| {
                serde_json::to_value(&e).map_err(|err| CliError::Other(err.to_string()))
            })
            .collect()
    }

    fn get_report(&self, run_id: &str) -> CliResult<serde_json::Value> {
        let report =
            grove_core::orchestrator::cost_report(&self.project, 50).map_err(CliError::Core)?;
        // Attach the specific run_id to contextualize the report.
        let mut val =
            serde_json::to_value(&report).map_err(|e| CliError::Other(e.to_string()))?;
        if let Some(obj) = val.as_object_mut() {
            obj.insert("run_id".to_string(), serde_json::Value::String(run_id.to_string()));
        }
        Ok(val)
    }

    fn get_plan(&self, run_id: Option<&str>) -> CliResult<serde_json::Value> {
        let rid = run_id.ok_or_else(|| CliError::Other("run_id is required for plan".into()))?;
        let steps =
            grove_core::orchestrator::list_plan_steps(&self.project, rid).map_err(CliError::Core)?;
        serde_json::to_value(&steps).map_err(|e| CliError::Other(e.to_string()))
    }

    fn get_subtasks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        let rid =
            run_id.ok_or_else(|| CliError::Other("run_id is required for subtasks".into()))?;
        let steps =
            grove_core::orchestrator::list_plan_steps(&self.project, rid).map_err(CliError::Core)?;
        steps
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn get_sessions(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let sessions =
            grove_core::orchestrator::list_sessions(&self.project, run_id).map_err(CliError::Core)?;
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
