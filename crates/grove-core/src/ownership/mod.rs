pub mod registry;

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};

use crate::errors::{GroveError, GroveResult};

pub fn acquire(
    conn: &Connection,
    run_id: &str,
    path: &str,
    owner_session_id: &str,
) -> GroveResult<()> {
    let changed = conn.execute(
        "INSERT OR IGNORE INTO ownership_locks (run_id, path, owner_session_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![run_id, path, owner_session_id, Utc::now().to_rfc3339()],
    )?;

    if changed == 0 {
        // 6.7: identify who currently holds the lock for a useful error message.
        let holder: String = conn
            .query_row(
                "SELECT owner_session_id FROM ownership_locks WHERE run_id=?1 AND path=?2",
                params![run_id, path],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "unknown".to_string());
        return Err(GroveError::OwnershipConflict {
            path: path.to_string(),
            holder,
        });
    }

    Ok(())
}

pub fn release(
    conn: &Connection,
    run_id: &str,
    path: &str,
    owner_session_id: &str,
) -> GroveResult<()> {
    conn.execute(
        "DELETE FROM ownership_locks WHERE run_id = ?1 AND path = ?2 AND owner_session_id = ?3",
        params![run_id, path, owner_session_id],
    )?;
    Ok(())
}
