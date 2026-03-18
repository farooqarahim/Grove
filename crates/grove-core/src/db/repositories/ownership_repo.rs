use rusqlite::{Connection, TransactionBehavior, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OwnershipLockRow {
    pub id: i64,
    pub run_id: String,
    pub path: String,
    pub owner_session_id: String,
    pub created_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<OwnershipLockRow> {
    Ok(OwnershipLockRow {
        id: r.get(0)?,
        run_id: r.get(1)?,
        path: r.get(2)?,
        owner_session_id: r.get(3)?,
        created_at: r.get(4)?,
    })
}

/// Attempt to acquire the lock for `path` within `run_id` by `session_id`.
///
/// Returns `Ok(true)` if inserted, `Ok(false)` if the same session already holds it.
/// Returns `Err` if a *different* session holds the lock (UNIQUE constraint violation).
pub fn acquire(
    conn: &mut Connection,
    run_id: &str,
    path: &str,
    session_id: &str,
    created_at: &str,
) -> GroveResult<bool> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = tx.execute(
        "INSERT OR IGNORE INTO ownership_locks (run_id, path, owner_session_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![run_id, path, session_id, created_at],
    );
    match result {
        Ok(n) => {
            tx.commit()?;
            Ok(n > 0)
        }
        Err(e) => Err(e.into()),
    }
}

/// Check whether `session_id` owns the lock for `path` in this run.
pub fn is_held_by(
    conn: &Connection,
    run_id: &str,
    path: &str,
    session_id: &str,
) -> GroveResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ownership_locks
         WHERE run_id=?1 AND path=?2 AND owner_session_id=?3",
        params![run_id, path, session_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Return the session that holds the lock, if any.
pub fn current_holder(conn: &Connection, run_id: &str, path: &str) -> GroveResult<Option<String>> {
    let holder: Option<String> = conn
        .query_row(
            "SELECT owner_session_id FROM ownership_locks WHERE run_id=?1 AND path=?2",
            params![run_id, path],
            |r| r.get(0),
        )
        .optional()?;
    Ok(holder)
}

/// Release a specific lock held by `session_id`.
pub fn release(
    conn: &Connection,
    run_id: &str,
    path: &str,
    session_id: &str,
) -> GroveResult<usize> {
    let n = conn.execute(
        "DELETE FROM ownership_locks WHERE run_id=?1 AND path=?2 AND owner_session_id=?3",
        params![run_id, path, session_id],
    )?;
    Ok(n)
}

/// Release all locks held by `session_id` (called on session end).
pub fn release_all_for_session(conn: &Connection, session_id: &str) -> GroveResult<usize> {
    let n = conn.execute(
        "DELETE FROM ownership_locks WHERE owner_session_id=?1",
        [session_id],
    )?;
    Ok(n)
}

/// List all currently held locks across all runs.
pub fn list_all(conn: &Connection) -> GroveResult<Vec<OwnershipLockRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, path, owner_session_id, created_at
         FROM ownership_locks ORDER BY run_id, path",
    )?;
    let rows = stmt.query_map([], map_row)?.collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Release all locks held by any session for `run_id`.
/// Called when a run completes or fails so stale records don't accumulate.
pub fn release_all_for_run(conn: &Connection, run_id: &str) -> GroveResult<usize> {
    let n = conn.execute("DELETE FROM ownership_locks WHERE run_id=?1", [run_id])?;
    Ok(n)
}

/// List all currently held locks for a specific run.
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<OwnershipLockRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, path, owner_session_id, created_at
         FROM ownership_locks WHERE run_id=?1 ORDER BY path",
    )?;
    let rows = stmt
        .query_map([run_id], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

// Bring optional extension into scope for `current_holder`.
use rusqlite::OptionalExtension;
