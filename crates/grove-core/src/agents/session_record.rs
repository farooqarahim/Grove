use serde::{Deserialize, Serialize};

use crate::db::repositories::sessions_repo::SessionRow;

use super::{AgentState, AgentType};

/// Full session record matching the `sessions` DB table.
///
/// Unlike `AgentSessionRecord` (the lightweight variant in `mod.rs`), this
/// struct carries all columns including `started_at`, `ended_at`, and
/// `updated_at`, and provides a direct mapping from a `SessionRow`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub run_id: String,
    pub agent_type: AgentType,
    pub state: AgentState,
    pub worktree_path: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub provider_session_id: Option<String>,
    pub last_heartbeat: Option<String>,
    pub stalled_since: Option<String>,
}

impl SessionRecord {
    /// Convert a raw `SessionRow` from the DB repository into a typed record.
    /// Unknown `agent_type` strings default to `AgentType::Builder`.
    /// Unknown `state` strings default to `AgentState::Failed`.
    pub fn from_db_row(row: SessionRow) -> Self {
        let agent_type = AgentType::from_str(&row.agent_type).unwrap_or(AgentType::Builder);
        let state = AgentState::from_str(&row.state).unwrap_or(AgentState::Failed);
        Self {
            id: row.id,
            run_id: row.run_id,
            agent_type,
            state,
            worktree_path: row.worktree_path,
            started_at: row.started_at,
            ended_at: row.ended_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
            provider_session_id: row.provider_session_id,
            last_heartbeat: row.last_heartbeat,
            stalled_since: row.stalled_since,
        }
    }
}
