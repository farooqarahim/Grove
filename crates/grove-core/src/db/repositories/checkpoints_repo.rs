use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CheckpointRow {
    pub id: String,
    pub run_id: String,
    pub stage: String,
    pub data_json: String,
    pub created_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CheckpointRow> {
    Ok(CheckpointRow {
        id: r.get(0)?,
        run_id: r.get(1)?,
        stage: r.get(2)?,
        data_json: r.get(3)?,
        created_at: r.get(4)?,
    })
}

pub fn save(conn: &mut Connection, row: &CheckpointRow) -> GroveResult<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO checkpoints (id, run_id, stage, data_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![row.id, row.run_id, row.stage, row.data_json, row.created_at],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> GroveResult<CheckpointRow> {
    let row = conn
        .query_row(
            "SELECT id, run_id, stage, data_json, created_at FROM checkpoints WHERE id=?1",
            [id],
            map_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("checkpoint {id}")))
}

/// Return the most recently created checkpoint for a run.
pub fn latest_for_run(conn: &Connection, run_id: &str) -> GroveResult<Option<CheckpointRow>> {
    let row = conn
        .query_row(
            "SELECT id, run_id, stage, data_json, created_at FROM checkpoints
             WHERE run_id=?1 ORDER BY created_at DESC LIMIT 1",
            [run_id],
            map_row,
        )
        .optional()?;
    Ok(row)
}

pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<CheckpointRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, stage, data_json, created_at FROM checkpoints
         WHERE run_id=?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([run_id], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}
