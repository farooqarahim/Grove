use crate::errors::McpError;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::helpers::{get_str, get_str_opt, now_iso};

type StageRow = (String, String, i64, String, String, i64, Option<String>);

pub fn get_pipeline_stage(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;

    let result: Option<StageRow> = conn
        .query_row(
            "SELECT id, stage_name, ordinal, instructions, status, gate_required, gate_context \
             FROM pipeline_stages \
             WHERE run_id = ?1 AND status IN ('pending', 'inprogress') \
             ORDER BY ordinal ASC \
             LIMIT 1",
            params![run_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, i64>(5)?,
                    r.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query next pipeline stage".into(),
            cause: e.to_string(),
        })?;

    match result {
        Some((id, stage_name, ordinal, instructions, status, gate_required, gate_context)) => {
            if status == "pending" {
                conn.execute(
                    "UPDATE pipeline_stages SET status = 'inprogress' WHERE id = ?1",
                    params![id],
                )
                .map_err(|e| McpError::Database {
                    operation: "update pipeline stage to inprogress".into(),
                    cause: e.to_string(),
                })?;
            }

            Ok(json!({
                "stage_id": id,
                "stage_name": stage_name,
                "ordinal": ordinal,
                "instructions": instructions,
                "gate_required": gate_required != 0,
                "gate_context": gate_context,
                "has_more": true,
            }))
        }
        None => {
            let total: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pipeline_stages WHERE run_id = ?1",
                    params![run_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            let completed: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM pipeline_stages WHERE run_id = ?1 AND status = 'completed'",
                    params![run_id],
                    |r| r.get(0),
                )
                .unwrap_or(0);

            Ok(json!({
                "stage_id": null,
                "all_completed": completed == total && total > 0,
                "total_stages": total,
                "completed_stages": completed,
                "has_more": false,
            }))
        }
    }
}

pub fn complete_pipeline_stage(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let stage_id = get_str(params, "stage_id")?;
    let summary = get_str(params, "summary")?;
    let artifacts = get_str_opt(params, "artifacts_json").unwrap_or("[]");
    let now = now_iso();

    let current_status: String = conn
        .query_row(
            "SELECT status FROM pipeline_stages WHERE id = ?1 AND run_id = ?2",
            params![stage_id, run_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "check pipeline stage status".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: format!("pipeline stage {stage_id} for run"),
            id: run_id.to_string(),
        })?;

    if current_status != "inprogress" {
        return Err(McpError::InvalidParams {
            message: format!(
                "stage {} is '{}', expected 'inprogress'",
                stage_id, current_status
            ),
        });
    }

    let gate_required: bool = conn
        .query_row(
            "SELECT gate_required FROM pipeline_stages WHERE id = ?1",
            params![stage_id],
            |r| {
                let v: i64 = r.get(0)?;
                Ok(v != 0)
            },
        )
        .unwrap_or(false);

    let new_status = if gate_required {
        "gate_pending"
    } else {
        "completed"
    };

    conn.execute(
        "UPDATE pipeline_stages SET status = ?1, summary = ?2, artifacts_json = ?3, completed_at = ?4 WHERE id = ?5",
        params![new_status, summary, artifacts, now, stage_id],
    )
    .map_err(|e| McpError::Database {
        operation: "complete pipeline stage".into(),
        cause: e.to_string(),
    })?;

    Ok(json!({
        "completed": true,
        "gate_pending": gate_required,
        "status": new_status,
    }))
}

pub fn check_pipeline_gate(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let stage_id = get_str(params, "stage_id")?;

    let row: Option<(String, Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT status, gate_decision, gate_context \
             FROM pipeline_stages \
             WHERE id = ?1 AND run_id = ?2",
            params![stage_id, run_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query pipeline gate status".into(),
            cause: e.to_string(),
        })?;

    match row {
        None => Err(McpError::NotFound {
            resource: format!("pipeline stage {stage_id} for run"),
            id: run_id.to_string(),
        }),
        Some((status, gate_decision, gate_context)) => {
            let approved = matches!(
                gate_decision.as_deref(),
                Some("approved") | Some("approved_with_note") | Some("auto_approved")
            );
            let rejected = matches!(gate_decision.as_deref(), Some("rejected"));

            if approved && status == "gate_pending" {
                let now = now_iso();
                conn.execute(
                    "UPDATE pipeline_stages SET status = 'completed', completed_at = ?1 WHERE id = ?2",
                    params![now, stage_id],
                )
                .map_err(|e| McpError::Database {
                    operation: "approve pipeline gate stage".into(),
                    cause: e.to_string(),
                })?;
            }

            Ok(json!({
                "status": status,
                "gate_decision": gate_decision,
                "gate_context": gate_context,
                "approved": approved,
                "rejected": rejected,
                "pending": !approved && !rejected,
            }))
        }
    }
}
