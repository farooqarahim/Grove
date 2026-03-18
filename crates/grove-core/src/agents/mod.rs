pub mod lifecycle;
pub mod session_record;
pub mod types;

use chrono::Utc;
use serde::{Deserialize, Serialize};

// Re-export the full AgentType from types.rs. All existing code that uses
// `crate::agents::AgentType` continues to work unchanged.
pub use types::AgentType;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentState {
    Queued,
    Running,
    Waiting,
    Completed,
    Failed,
    Killed,
}

impl AgentState {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentState::Queued => "queued",
            AgentState::Running => "running",
            AgentState::Waiting => "waiting",
            AgentState::Completed => "completed",
            AgentState::Failed => "failed",
            AgentState::Killed => "killed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(AgentState::Queued),
            "running" => Some(AgentState::Running),
            "waiting" => Some(AgentState::Waiting),
            "completed" => Some(AgentState::Completed),
            "failed" => Some(AgentState::Failed),
            "killed" => Some(AgentState::Killed),
            _ => None,
        }
    }
}

/// Lightweight session record used by the orchestrator.
/// For the full DB-mapped version see `session_record::SessionRecord`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSessionRecord {
    pub id: String,
    pub run_id: String,
    pub agent_type: AgentType,
    pub state: AgentState,
    pub worktree_path: String,
    pub created_at: String,
}

impl AgentSessionRecord {
    pub fn new(id: String, run_id: String, agent_type: AgentType, worktree_path: String) -> Self {
        Self {
            id,
            run_id,
            agent_type,
            state: AgentState::Queued,
            worktree_path,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}
