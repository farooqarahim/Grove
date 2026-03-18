use rusqlite::{Connection, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QaMessage {
    pub id: i64,
    pub run_id: String,
    pub session_id: Option<String>,
    pub direction: String,
    pub content: String,
    pub options_json: Option<String>,
    pub created_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<QaMessage> {
    Ok(QaMessage {
        id: r.get(0)?,
        run_id: r.get(1)?,
        session_id: r.get(2)?,
        direction: r.get(3)?,
        content: r.get(4)?,
        options_json: r.get(5)?,
        created_at: r.get(6)?,
    })
}

/// Insert a Q&A message (either a question from the agent or an answer from
/// the user) and return the row ID.
///
/// `direction` should be `"question"` or `"answer"`.
pub fn insert(
    conn: &Connection,
    run_id: &str,
    session_id: Option<&str>,
    direction: &str,
    content: &str,
    options_json: Option<&str>,
) -> GroveResult<i64> {
    conn.execute(
        "INSERT INTO qa_messages (run_id, session_id, direction, content, options_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![run_id, session_id, direction, content, options_json],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List all Q&A messages for a run, ordered by creation time.
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<QaMessage>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, run_id, session_id, direction, content, options_json, created_at
         FROM qa_messages
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
