pub mod executor;
pub mod policy;
pub mod queue;

use chrono::Utc;
use rusqlite::{Connection, params};

use crate::errors::GroveResult;

pub fn enqueue(
    conn: &Connection,
    run_id: &str,
    session_id: &str,
    branch_name: &str,
) -> GroveResult<i64> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO merge_queue (run_id, session_id, branch_name, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'queued', ?4, ?5)",
        params![run_id, session_id, branch_name, now, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn mark_status(
    conn: &Connection,
    id: i64,
    status: &str,
    error: Option<&str>,
) -> GroveResult<()> {
    conn.execute(
        "UPDATE merge_queue SET status = ?1, error = ?2, updated_at = ?3 WHERE id = ?4",
        params![status, error, Utc::now().to_rfc3339(), id],
    )?;
    Ok(())
}
