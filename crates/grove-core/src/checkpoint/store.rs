use chrono::Utc;
use rusqlite::Connection;

use crate::db::repositories::checkpoints_repo::{self, CheckpointRow};
use crate::errors::GroveResult;

use super::payload::CheckpointPayload;

/// Persist a checkpoint payload.
pub fn save(conn: &mut Connection, id: &str, payload: &CheckpointPayload) -> GroveResult<()> {
    let row = CheckpointRow {
        id: id.to_string(),
        run_id: payload.run_id.clone(),
        stage: payload.stage.clone(),
        data_json: payload.to_json()?,
        created_at: Utc::now().to_rfc3339(),
    };
    checkpoints_repo::save(conn, &row)
}

/// Load a checkpoint by id and deserialise its payload.
pub fn load(conn: &Connection, id: &str) -> GroveResult<CheckpointPayload> {
    let row = checkpoints_repo::get(conn, id)?;
    CheckpointPayload::from_json(&row.data_json)
}

/// Return the most recently saved checkpoint for `run_id`, or `None`.
pub fn latest_for_run(conn: &Connection, run_id: &str) -> GroveResult<Option<CheckpointPayload>> {
    match checkpoints_repo::latest_for_run(conn, run_id)? {
        Some(row) => Ok(Some(CheckpointPayload::from_json(&row.data_json)?)),
        None => Ok(None),
    }
}
