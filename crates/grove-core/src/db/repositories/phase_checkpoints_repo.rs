/// Repository for phase_checkpoints table — gate decisions during pipeline execution.
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::errors::GroveResult;

/// A phase checkpoint record — represents a gate point in pipeline execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseCheckpoint {
    pub id: i64,
    pub run_id: String,
    pub agent: String,
    /// "pending", "approved", "rejected", "skipped"
    pub status: String,
    /// User's decision text (optional notes).
    pub decision: Option<String>,
    pub decided_at: Option<String>,
    /// Path to the artifact produced by this agent (if any).
    pub artifact_path: Option<String>,
    pub created_at: String,
}

/// Insert a new phase checkpoint when an agent completes and hits a gate.
pub fn insert(
    conn: &Connection,
    run_id: &str,
    agent: &str,
    artifact_path: Option<&str>,
) -> GroveResult<i64> {
    conn.execute(
        "INSERT INTO phase_checkpoints (run_id, agent, status, artifact_path) VALUES (?1, ?2, 'pending', ?3)",
        rusqlite::params![run_id, agent, artifact_path],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Submit a gate decision (approve, reject, skip).
pub fn submit_decision(
    conn: &Connection,
    checkpoint_id: i64,
    decision: &str,
    notes: Option<&str>,
) -> GroveResult<()> {
    conn.execute(
        "UPDATE phase_checkpoints SET status = ?1, decision = ?2, decided_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id = ?3",
        rusqlite::params![decision, notes, checkpoint_id],
    )?;
    Ok(())
}

/// Get all phase checkpoints for a run.
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<PhaseCheckpoint>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, agent, status, decision, decided_at, artifact_path, created_at
         FROM phase_checkpoints WHERE run_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([run_id], |r| {
        Ok(PhaseCheckpoint {
            id: r.get(0)?,
            run_id: r.get(1)?,
            agent: r.get(2)?,
            status: r.get(3)?,
            decision: r.get(4)?,
            decided_at: r.get(5)?,
            artifact_path: r.get(6)?,
            created_at: r.get(7)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get the latest pending checkpoint for a run (the one awaiting user decision).
pub fn get_pending(conn: &Connection, run_id: &str) -> GroveResult<Option<PhaseCheckpoint>> {
    let result = conn.query_row(
        "SELECT id, run_id, agent, status, decision, decided_at, artifact_path, created_at
         FROM phase_checkpoints WHERE run_id = ?1 AND status = 'pending' ORDER BY id DESC LIMIT 1",
        [run_id],
        |r| {
            Ok(PhaseCheckpoint {
                id: r.get(0)?,
                run_id: r.get(1)?,
                agent: r.get(2)?,
                status: r.get(3)?,
                decision: r.get(4)?,
                decided_at: r.get(5)?,
                artifact_path: r.get(6)?,
                created_at: r.get(7)?,
            })
        },
    );
    match result {
        Ok(cp) => Ok(Some(cp)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Get the notes/decision text for a checkpoint.
pub fn get_notes(conn: &Connection, checkpoint_id: i64) -> Option<String> {
    conn.query_row(
        "SELECT decision FROM phase_checkpoints WHERE id = ?1",
        [checkpoint_id],
        |r| r.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
    .filter(|s| !s.is_empty())
}

/// Update the current_agent and pipeline on a run.
pub fn update_run_phase(
    conn: &Connection,
    run_id: &str,
    pipeline: &str,
    current_agent: &str,
) -> GroveResult<()> {
    conn.execute(
        "UPDATE runs SET pipeline = ?1, current_agent = ?2 WHERE id = ?3",
        rusqlite::params![pipeline, current_agent, run_id],
    )?;
    Ok(())
}
