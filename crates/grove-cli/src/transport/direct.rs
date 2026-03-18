use std::path::{Path, PathBuf};

use crate::error::{CliError, CliResult};
use super::{RunResult, StartRunRequest, Transport};

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
            None,          // budget_usd
            priority,
            model,
            None,          // provider
            conversation_id,
            None,          // resume_provider_session_id
            pipeline,
            permission_mode,
            false,         // disable_phase_gates
        )
        .map_err(CliError::Core)
    }

    fn cancel_task(&self, task_id: &str) -> CliResult<()> {
        grove_core::orchestrator::cancel_task(&self.project, task_id).map_err(CliError::Core)
    }

    fn drain_queue(&self, _project: &std::path::Path) -> CliResult<()> {
        Err(CliError::Other("drain_queue not available in direct mode".into()))
    }

    fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult> {
        let task = grove_core::orchestrator::queue_task(
            &self.project,
            &req.objective,
            None,                                   // budget_usd
            0,                                      // priority (default)
            req.model.as_deref(),
            None,                                   // provider
            req.conversation_id.as_deref(),
            None,                                   // resume_provider_session_id
            req.pipeline.as_deref(),
            req.permission_mode.as_deref(),
            false,                                  // disable_phase_gates
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
