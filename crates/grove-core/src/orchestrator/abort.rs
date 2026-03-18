use rusqlite::Connection;
use serde_json::json;
use uuid::Uuid;

use crate::checkpoint::{self, BudgetSnapshot, CheckpointPayload};
use crate::db::repositories::ownership_repo;
use crate::errors::GroveResult;
use crate::events;

use super::{RunState, transitions};

/// Transition `run_id` to `Paused`, release all ownership locks held by any
/// session in this run, and save a checkpoint so `resume` can restore state.
pub fn abort_gracefully(
    conn: &Connection,
    run_id: &str,
    objective: &str,
    budget_usd: f64,
    current_state: RunState,
) -> GroveResult<()> {
    transitions::apply_transition(conn, run_id, current_state, RunState::Paused)?;

    // Release all locks belonging to this run (sessions may have been mid-work).
    let locks = ownership_repo::list_all(conn)?;
    for lock in locks.iter().filter(|l| l.run_id == run_id) {
        let _ = ownership_repo::release(conn, run_id, &lock.path, &lock.owner_session_id);
    }

    // Capture the provider session/thread ID from the latest active session
    // so resume can continue the coding agent conversation.
    let provider_sid: Option<String> = conn
        .query_row(
            "SELECT provider_session_id FROM sessions \
             WHERE run_id = ?1 AND provider_session_id IS NOT NULL \
             ORDER BY created_at DESC LIMIT 1",
            [run_id],
            |r| r.get(0),
        )
        .ok();
    if let Some(ref sid) = provider_sid {
        let _ = conn.execute(
            "UPDATE runs SET provider_thread_id = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?2",
            rusqlite::params![sid, run_id],
        );
    }

    // Save a checkpoint so the run can be resumed.
    let checkpoint_id = format!("cp_{}", Uuid::new_v4().simple());
    let payload = CheckpointPayload {
        run_id: run_id.to_string(),
        stage: "paused".to_string(),
        active_sessions: vec![],
        pending_tasks: vec![objective.to_string()],
        ownership: vec![],
        budget: BudgetSnapshot {
            allocated_usd: budget_usd,
            used_usd: 0.0,
        },
    };
    checkpoint::save(conn, &checkpoint_id, &payload)?;

    events::emit(
        conn,
        run_id,
        None,
        "run_aborted",
        json!({ "checkpoint_id": checkpoint_id }),
    )?;

    Ok(())
}
