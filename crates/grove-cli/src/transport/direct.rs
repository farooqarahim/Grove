use std::path::{Path, PathBuf};

use crate::error::{CliError, CliResult};
use super::Transport;

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
        Err(CliError::Other("not yet implemented".into()))
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
}
