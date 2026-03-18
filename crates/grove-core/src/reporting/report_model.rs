use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::repositories::{runs_repo, sessions_repo};
use crate::errors::GroveResult;
use crate::events;

/// Summary of a single agent session within a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub agent_type: String,
    pub state: String,
    pub worktree_path: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
}

/// A single event entry for the report timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntry {
    pub created_at: String,
    pub event_type: String,
    pub session_id: Option<String>,
    /// Structured JSON payload — sent as a proper JSON object over IPC.
    pub payload: Value,
}

/// Full report for a run: metadata + session summaries + event timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunReport {
    pub run_id: String,
    pub objective: String,
    pub state: String,
    pub created_at: String,
    pub sessions: Vec<SessionSummary>,
    pub events: Vec<EventEntry>,
}

impl RunReport {
    /// Build a `RunReport` by querying the DB for the given `run_id`.
    pub fn from_db(conn: &Connection, run_id: &str) -> GroveResult<Self> {
        let run = runs_repo::get(conn, run_id)?;

        let session_rows = sessions_repo::list_for_run(conn, run_id)?;
        let sessions = session_rows
            .into_iter()
            .map(|r| SessionSummary {
                id: r.id,
                agent_type: r.agent_type,
                state: r.state,
                worktree_path: r.worktree_path,
                started_at: r.started_at,
                ended_at: r.ended_at,
            })
            .collect();

        let event_rows = events::list_for_run(conn, run_id)?;
        let events = event_rows
            .into_iter()
            .map(|e| EventEntry {
                created_at: e.created_at,
                event_type: e.event_type,
                session_id: e.session_id,
                payload: e.payload.clone(),
            })
            .collect();

        Ok(RunReport {
            run_id: run.id,
            objective: run.objective,
            state: run.state,
            created_at: run.created_at,
            sessions,
            events,
        })
    }
}
