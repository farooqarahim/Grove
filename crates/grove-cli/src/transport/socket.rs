use std::path::PathBuf;

use crate::error::{CliError, CliResult};
use super::Transport;

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

    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }
}
