pub mod payload;
pub mod store;
pub mod wal_controller;

use chrono::Utc;
use rusqlite::{Connection, params};

use crate::errors::GroveResult;

// Re-export the types orchestrator/mod.rs imports directly.
pub use payload::{BudgetSnapshot, CheckpointPayload, OwnershipSnapshot};

/// Persist a checkpoint.
///
/// Uses a plain INSERT (no explicit transaction) so callers holding
/// a `&Connection` can call this directly, as the original API did.
pub fn save(
    conn: &Connection,
    checkpoint_id: &str,
    payload: &CheckpointPayload,
) -> GroveResult<()> {
    conn.execute(
        "INSERT INTO checkpoints (id, run_id, stage, data_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            checkpoint_id,
            payload.run_id,
            payload.stage,
            serde_json::to_string(payload)?,
            Utc::now().to_rfc3339(),
        ],
    )?;
    Ok(())
}

/// Return the latest checkpoint for a run, or `None`.
pub fn latest_for_run(conn: &Connection, run_id: &str) -> GroveResult<Option<CheckpointPayload>> {
    store::latest_for_run(conn, run_id)
}
