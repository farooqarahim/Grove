use chrono::Utc;
use rusqlite::Connection;
use serde_json::json;

use crate::errors::{GroveError, GroveResult};
use crate::events;

use super::{RunState, state_machine};

/// Validate the transition `from → to`, write it to the DB, and emit a
/// `run_state_changed` event. Returns `Err(InvalidTransition)` if the move
/// is not allowed by the state machine.
pub fn apply_transition(
    conn: &Connection,
    run_id: &str,
    from: RunState,
    to: RunState,
) -> GroveResult<()> {
    if !state_machine::is_valid_run_transition(from, to) {
        return Err(GroveError::InvalidTransition(format!(
            "run {run_id}: {} → {} is not allowed",
            from.as_str(),
            to.as_str()
        )));
    }

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE runs SET state=?1, updated_at=?2 WHERE id=?3",
        rusqlite::params![to.as_str(), now, run_id],
    )?;

    events::emit(
        conn,
        run_id,
        None,
        "run_state_changed",
        json!({ "from": from.as_str(), "to": to.as_str() }),
    )?;

    Ok(())
}
