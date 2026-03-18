use chrono::Utc;
use rusqlite::Connection;
use serde_json::json;

use crate::db::repositories::ownership_repo;
use crate::db::repositories::sessions_repo::{self, SessionRow};
use crate::errors::{GroveError, GroveResult};
use crate::events;

use super::{AgentState, AgentType, session_record::SessionRecord};

/// Insert a new session row in `Queued` state and return its record.
pub fn spawn_session(
    conn: &mut Connection,
    run_id: &str,
    agent_type: AgentType,
    worktree_path: &str,
) -> GroveResult<SessionRecord> {
    use uuid::Uuid;
    let session_id = format!("sess_{}", Uuid::new_v4().simple());
    let now = Utc::now().to_rfc3339();

    let row = SessionRow {
        id: session_id.clone(),
        run_id: run_id.to_string(),
        agent_type: agent_type.as_str().to_string(),
        state: AgentState::Queued.as_str().to_string(),
        worktree_path: worktree_path.to_string(),
        started_at: None,
        ended_at: None,
        created_at: now.clone(),
        updated_at: now,
        provider_session_id: None,
        last_heartbeat: None,
        stalled_since: None,
        checkpoint_sha: None,
        parent_checkpoint_sha: None,
        branch: None,
        pid: None,
    };

    sessions_repo::insert(conn, &row)?;

    events::emit(
        conn,
        run_id,
        Some(&session_id),
        crate::events::event_types::SESSION_SPAWNED,
        json!({ "agent_type": agent_type.as_str(), "worktree_path": worktree_path }),
    )?;

    Ok(SessionRecord::from_db_row(row))
}

/// Transition a session to `Completed` or `Failed` and record timing.
pub fn finish_session(conn: &Connection, session_id: &str, outcome: AgentState) -> GroveResult<()> {
    if outcome != AgentState::Completed && outcome != AgentState::Failed {
        return Err(GroveError::Runtime(format!(
            "finish_session called with non-terminal state '{}'",
            outcome.as_str()
        )));
    }

    let now = Utc::now().to_rfc3339();
    sessions_repo::set_state(conn, session_id, outcome.as_str(), None, Some(&now), &now)?;

    // Fetch run_id for event emission.
    let run_id: String = conn.query_row(
        "SELECT run_id FROM sessions WHERE id=?1",
        [session_id],
        |r| r.get(0),
    )?;

    events::emit(
        conn,
        &run_id,
        Some(session_id),
        crate::events::event_types::SESSION_STATE_CHANGED,
        json!({ "state": outcome.as_str() }),
    )?;

    Ok(())
}

/// Immediately kill a session: set state to `Killed`, release all ownership
/// locks it holds, and emit an event.
pub fn kill_session(conn: &mut Connection, session_id: &str) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    sessions_repo::set_state(
        conn,
        session_id,
        AgentState::Killed.as_str(),
        None,
        Some(&now),
        &now,
    )?;

    ownership_repo::release_all_for_session(conn, session_id)?;

    let run_id: String = conn.query_row(
        "SELECT run_id FROM sessions WHERE id=?1",
        [session_id],
        |r| r.get(0),
    )?;

    events::emit(
        conn,
        &run_id,
        Some(session_id),
        crate::events::event_types::SESSION_STATE_CHANGED,
        json!({ "state": "killed" }),
    )?;

    Ok(())
}

/// List all sessions for a run as typed `SessionRecord`s.
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<SessionRecord>> {
    let rows = sessions_repo::list_for_run(conn, run_id)?;
    Ok(rows.into_iter().map(SessionRecord::from_db_row).collect())
}
