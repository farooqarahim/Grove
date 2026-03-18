use rusqlite::{Connection, TransactionBehavior, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone)]
pub struct EventRow {
    pub id: i64,
    pub run_id: String,
    pub session_id: Option<String>,
    pub event_type: String,
    pub payload_json: String,
    pub created_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<EventRow> {
    Ok(EventRow {
        id: r.get(0)?,
        run_id: r.get(1)?,
        session_id: r.get(2)?,
        event_type: r.get(3)?,
        payload_json: r.get(4)?,
        created_at: r.get(5)?,
    })
}

pub fn insert(conn: &mut Connection, row: &EventRow) -> GroveResult<i64> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO events (run_id, session_id, type, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            row.run_id,
            row.session_id,
            row.event_type,
            row.payload_json,
            row.created_at,
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(id)
}

pub fn list_all(conn: &Connection, limit: i64) -> GroveResult<Vec<EventRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, session_id, type, payload_json, created_at
         FROM events ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}
