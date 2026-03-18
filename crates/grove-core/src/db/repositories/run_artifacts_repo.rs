use rusqlite::{Connection, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunArtifact {
    pub id: i64,
    pub run_id: String,
    pub agent: String,
    pub filename: String,
    pub content_hash: String,
    pub size_bytes: i64,
    pub created_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunArtifact> {
    Ok(RunArtifact {
        id: r.get(0)?,
        run_id: r.get(1)?,
        agent: r.get(2)?,
        filename: r.get(3)?,
        content_hash: r.get(4)?,
        size_bytes: r.get(5)?,
        created_at: r.get(6)?,
    })
}

/// Record an artifact produced by an agent and return the row ID.
pub fn record_artifact(
    conn: &Connection,
    run_id: &str,
    agent: &str,
    filename: &str,
    content_hash: &str,
    size_bytes: i64,
) -> GroveResult<i64> {
    conn.execute(
        "INSERT INTO run_artifacts (run_id, agent, filename, content_hash, size_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![run_id, agent, filename, content_hash, size_bytes],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List all artifacts for a run, ordered by creation time.
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<RunArtifact>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, run_id, agent, filename, content_hash, size_bytes, created_at
         FROM run_artifacts
         WHERE run_id = ?1
         ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![run_id], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}
