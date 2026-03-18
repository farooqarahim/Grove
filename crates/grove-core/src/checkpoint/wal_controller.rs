use rusqlite::Connection;

use crate::errors::GroveResult;

#[derive(Debug)]
pub struct WalStats {
    /// Pages in the WAL that have not yet been written back to the main DB file.
    pub wal_pages: i64,
    /// Pages successfully checkpointed in this call.
    pub checkpointed_pages: i64,
}

/// Run a passive checkpoint — non-blocking, does not interrupt active readers
/// or writers. Safe to call after any run completes.
///
/// Returns `WalStats` with page counts; does NOT return an error if the WAL
/// still has un-checkpointed pages (that is normal when readers are active).
pub fn passive_checkpoint(conn: &Connection) -> GroveResult<WalStats> {
    run_checkpoint(conn, "PASSIVE")
}

/// Run a full checkpoint — blocks until all WAL pages are written back.
/// Use only in maintenance mode (no concurrent writers expected).
pub fn full_checkpoint(conn: &Connection) -> GroveResult<WalStats> {
    run_checkpoint(conn, "FULL")
}

/// Return the current number of pages in the WAL file.
pub fn wal_size_pages(conn: &Connection) -> GroveResult<i64> {
    // PRAGMA wal_checkpoint returns (busy, log, checkpointed).
    // We just want the log (total WAL pages) without doing a full checkpoint.
    let stats = run_checkpoint(conn, "PASSIVE")?;
    Ok(stats.wal_pages)
}

fn run_checkpoint(conn: &Connection, mode: &str) -> GroveResult<WalStats> {
    // PRAGMA wal_checkpoint(MODE) returns a single row: (busy, log, checkpointed)
    // busy         — 1 if WAL could not be fully checkpointed due to active readers
    // log          — total pages in the WAL
    // checkpointed — pages checkpointed in this call
    let (_, log, checkpointed): (i64, i64, i64) =
        conn.query_row(&format!("PRAGMA wal_checkpoint({mode})"), [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;

    Ok(WalStats {
        wal_pages: log,
        checkpointed_pages: checkpointed,
    })
}
