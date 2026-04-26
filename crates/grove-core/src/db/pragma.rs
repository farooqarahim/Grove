use rusqlite::Connection;

use crate::errors::GroveResult;

/// Apply all required PRAGMAs on a freshly opened connection.
/// Must be called before any queries.
pub fn apply(conn: &Connection) -> GroveResult<()> {
    conn.pragma_update(None, "busy_timeout", 30000i64)?;
    // INCREMENTAL auto-vacuum: SQLite reclaims free pages incrementally
    // rather than never (NONE) or on every commit (FULL). For existing
    // databases this is a no-op until a VACUUM is run; new databases pick
    // it up immediately.
    conn.pragma_update(None, "auto_vacuum", 2i64)?; // 2 = INCREMENTAL
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "cache_size", -8000i64)?; // 8 MB page cache
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    Ok(())
}
