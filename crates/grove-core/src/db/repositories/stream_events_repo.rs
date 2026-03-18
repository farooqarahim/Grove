use rusqlite::{Connection, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StreamEventRow {
    pub id: i64,
    pub run_id: String,
    pub session_id: Option<String>,
    pub kind: String,
    pub content_json: String,
    pub created_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<StreamEventRow> {
    Ok(StreamEventRow {
        id: r.get(0)?,
        run_id: r.get(1)?,
        session_id: r.get(2)?,
        kind: r.get(3)?,
        content_json: r.get(4)?,
        created_at: r.get(5)?,
    })
}

/// Insert a stream event and return its auto-generated row ID.
pub fn insert(
    conn: &Connection,
    run_id: &str,
    session_id: Option<&str>,
    kind: &str,
    content_json: &str,
) -> GroveResult<i64> {
    conn.execute(
        "INSERT INTO stream_events (run_id, session_id, kind, content_json)
         VALUES (?1, ?2, ?3, ?4)",
        params![run_id, session_id, kind, content_json],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List stream events for a run, optionally starting after a given ID.
///
/// `after_id` is exclusive: only events with `id > after_id` are returned.
/// `limit` caps the number of rows returned (0 means no limit).
pub fn list_for_run(
    conn: &Connection,
    run_id: &str,
    after_id: i64,
    limit: i64,
) -> GroveResult<Vec<StreamEventRow>> {
    let effective_limit = if limit <= 0 { i64::MAX } else { limit };
    let mut stmt = conn.prepare_cached(
        "SELECT id, run_id, session_id, kind, content_json, created_at
         FROM stream_events
         WHERE run_id = ?1 AND id > ?2
         ORDER BY id ASC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![run_id, after_id, effective_limit], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
