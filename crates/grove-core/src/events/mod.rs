pub mod event_types;
pub mod model;
pub mod redaction;
pub mod writer_queue;

use chrono::Utc;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::GroveResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: i64,
    pub run_id: String,
    pub session_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

/// Insert a new event row, redacting any secret patterns in the payload.
pub fn emit(
    conn: &Connection,
    run_id: &str,
    session_id: Option<&str>,
    event_type: &str,
    payload: Value,
) -> GroveResult<()> {
    let raw = serde_json::to_string(&payload)?;
    let safe = redaction::redact(&raw);
    conn.execute(
        "INSERT INTO events (run_id, session_id, type, payload_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            run_id,
            session_id,
            event_type,
            safe,
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

/// Fetch all events for a run in insertion order.
///
/// Returns an error if any stored payload cannot be deserialized as JSON
/// rather than silently substituting `null`.
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<EventRecord>> {
    fetch_events(conn, run_id, None)
}

/// Fetch the most recent `limit` events for a run, returned oldest-first.
///
/// Used by `grove logs` to bound output size.
pub fn list_for_run_tail(
    conn: &Connection,
    run_id: &str,
    limit: i64,
) -> GroveResult<Vec<EventRecord>> {
    let mut events = fetch_events_desc(conn, run_id, limit)?;
    events.reverse();
    Ok(events)
}

fn fetch_events(
    conn: &Connection,
    run_id: &str,
    limit: Option<i64>,
) -> GroveResult<Vec<EventRecord>> {
    let sql = match limit {
        Some(_) => {
            "SELECT id, run_id, session_id, type, payload_json, created_at \
                     FROM events WHERE run_id = ?1 ORDER BY id ASC LIMIT ?2"
        }
        None => {
            "SELECT id, run_id, session_id, type, payload_json, created_at \
                 FROM events WHERE run_id = ?1 ORDER BY id ASC"
        }
    };

    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<_> = match limit {
        Some(n) => stmt
            .query_map(params![run_id, n], map_row)?
            .collect::<Result<_, _>>()?,
        None => stmt
            .query_map([run_id], map_row)?
            .collect::<Result<_, _>>()?,
    };
    rows.into_iter().map(deserialize_row).collect()
}

fn fetch_events_desc(conn: &Connection, run_id: &str, limit: i64) -> GroveResult<Vec<EventRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, session_id, type, payload_json, created_at \
         FROM events WHERE run_id = ?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows: Vec<_> = stmt
        .query_map(params![run_id, limit], map_row)?
        .collect::<Result<_, _>>()?;
    rows.into_iter().map(deserialize_row).collect()
}

type RawEventRow = (i64, String, Option<String>, String, String, String);

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawEventRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
    ))
}

fn deserialize_row(row: RawEventRow) -> GroveResult<EventRecord> {
    let (id, run_id, session_id, event_type, payload_json, created_at) = row;
    let payload = serde_json::from_str::<Value>(&payload_json)?;
    Ok(EventRecord {
        id,
        run_id,
        session_id,
        event_type,
        payload,
        created_at,
    })
}
