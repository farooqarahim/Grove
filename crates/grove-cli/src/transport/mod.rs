pub mod direct;
pub mod socket;

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
    // Tasks 8–15 add more methods here. Update all 3 impls + TestTransport each time.
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
