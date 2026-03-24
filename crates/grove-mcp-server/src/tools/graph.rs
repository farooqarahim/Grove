use crate::errors::McpError;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use super::helpers::{
    gen_id, get_bool_opt, get_i64, get_str, get_str_opt, graph_row_to_json, now_iso,
    phase_row_to_json, step_row_to_json, GRAPH_COLS, PHASE_COLS, STEP_COLS,
};

pub fn create_graph(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let title = get_str(params, "title")?;
    let conversation_id = get_str(params, "conversation_id")?;
    let description = get_str_opt(params, "description").unwrap_or("");

    let id = gen_id("gg");
    let now = now_iso();

    conn.execute(
        "INSERT INTO grove_graphs \
            (id, conversation_id, title, description, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, conversation_id, title, description, now, now],
    )
    .map_err(|e| McpError::Database {
        operation: "create graph".into(),
        cause: e.to_string(),
    })?;

    Ok(json!({
        "graph_id": id,
        "status": "open"
    }))
}

pub fn add_phase(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let graph_id = get_str(params, "graph_id")?;
    let title = get_str(params, "title")?;
    let task_objective = get_str(params, "task_objective")?;
    let ordinal = get_i64(params, "ordinal")?;
    let ref_required = get_bool_opt(params, "ref_required").unwrap_or(false);
    let reference_doc_path = get_str_opt(params, "reference_doc_path");

    let depends_on = params
        .get("depends_on")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "[]".to_string());

    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM graph_phases WHERE graph_id=?1 AND task_name=?2",
            params![graph_id, title],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "check duplicate phase".into(),
            cause: e.to_string(),
        })?;

    if let Some(existing_id) = existing {
        return Ok(json!({
            "phase_id": existing_id,
            "created": false
        }));
    }

    let id = gen_id("gp");
    let now = now_iso();
    let ref_req_int: i64 = if ref_required { 1 } else { 0 };

    conn.execute(
        "INSERT INTO graph_phases \
            (id, graph_id, task_name, task_objective, ordinal, depends_on_json, \
             ref_required, reference_doc_path, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            graph_id,
            title,
            task_objective,
            ordinal,
            depends_on,
            ref_req_int,
            reference_doc_path,
            now,
            now,
        ],
    )
    .map_err(|e| McpError::Database {
        operation: "insert graph phase".into(),
        cause: e.to_string(),
    })?;

    conn.execute(
        "UPDATE grove_graphs SET phases_created_count = phases_created_count + 1, updated_at = ?1 WHERE id = ?2",
        params![now, graph_id],
    )
    .map_err(|e| McpError::Database {
        operation: "update graph phase count".into(),
        cause: e.to_string(),
    })?;

    Ok(json!({
        "phase_id": id,
        "created": true
    }))
}

pub fn add_step(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let phase_id = get_str(params, "phase_id")?;
    let graph_id = get_str(params, "graph_id")?;
    let title = get_str(params, "title")?;
    let task_objective = get_str(params, "task_objective")?;
    let ordinal = get_i64(params, "ordinal")?;
    let step_type = get_str_opt(params, "step_type").unwrap_or("code");
    let execution_mode = get_str_opt(params, "execution_mode").unwrap_or("auto");
    let ref_required = get_bool_opt(params, "ref_required").unwrap_or(false);
    let reference_doc_path = get_str_opt(params, "reference_doc_path");

    let depends_on = params
        .get("depends_on")
        .map(|v| v.to_string())
        .unwrap_or_else(|| "[]".to_string());

    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM graph_steps WHERE phase_id=?1 AND task_name=?2",
            params![phase_id, title],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "check duplicate step".into(),
            cause: e.to_string(),
        })?;

    if let Some(existing_id) = existing {
        return Ok(json!({
            "step_id": existing_id,
            "created": false
        }));
    }

    let id = gen_id("gs");
    let now = now_iso();
    let ref_req_int: i64 = if ref_required { 1 } else { 0 };

    conn.execute(
        "INSERT INTO graph_steps \
            (id, phase_id, graph_id, task_name, task_objective, ordinal, step_type, \
             execution_mode, depends_on_json, ref_required, reference_doc_path, \
             created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            phase_id,
            graph_id,
            title,
            task_objective,
            ordinal,
            step_type,
            execution_mode,
            depends_on,
            ref_req_int,
            reference_doc_path,
            now,
            now,
        ],
    )
    .map_err(|e| McpError::Database {
        operation: "insert graph step".into(),
        cause: e.to_string(),
    })?;

    conn.execute(
        "UPDATE grove_graphs SET steps_created_count = steps_created_count + 1, updated_at = ?1 WHERE id = ?2",
        params![now, graph_id],
    )
    .map_err(|e| McpError::Database {
        operation: "update graph step count".into(),
        cause: e.to_string(),
    })?;

    Ok(json!({
        "step_id": id,
        "created": true
    }))
}

pub fn update_phase_status(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let phase_id = get_str(params, "phase_id")?;
    let status = get_str(params, "status")?;

    const VALID_PHASE_STATUSES: &[&str] = &["open", "inprogress", "closed", "failed"];
    if !VALID_PHASE_STATUSES.contains(&status) {
        return Err(McpError::InvalidParams {
            message: format!(
                "invalid status '{}' — valid values: open, inprogress, closed, failed",
                status
            ),
        });
    }

    let now = now_iso();

    let n = conn
        .execute(
            "UPDATE graph_phases SET status=?1, updated_at=?2 WHERE id=?3",
            params![status, now, phase_id],
        )
        .map_err(|e| McpError::Database {
            operation: "update phase status".into(),
            cause: e.to_string(),
        })?;

    if n == 0 {
        return Err(McpError::NotFound {
            resource: "phase".into(),
            id: phase_id.to_string(),
        });
    }

    Ok(json!({ "updated": true }))
}

pub fn update_step_status(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let step_id = get_str(params, "step_id")?;
    let status = get_str(params, "status")?;

    const VALID_STEP_STATUSES: &[&str] = &["open", "inprogress", "closed", "failed"];
    if !VALID_STEP_STATUSES.contains(&status) {
        return Err(McpError::InvalidParams {
            message: format!(
                "invalid status '{}' — valid values: open, inprogress, closed, failed",
                status
            ),
        });
    }

    let now = now_iso();

    let n = conn
        .execute(
            "UPDATE graph_steps SET status=?1, updated_at=?2 WHERE id=?3",
            params![status, now, step_id],
        )
        .map_err(|e| McpError::Database {
            operation: "update step status".into(),
            cause: e.to_string(),
        })?;

    if n == 0 {
        return Err(McpError::NotFound {
            resource: "step".into(),
            id: step_id.to_string(),
        });
    }

    Ok(json!({ "updated": true }))
}

pub fn set_step_outcome(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let step_id = get_str(params, "step_id")?;
    let outcome = get_str(params, "outcome")?;
    let ai_comments = get_str(params, "ai_comments")?;
    let grade = super::helpers::get_i64_opt(params, "grade");
    let now = now_iso();

    let n = conn
        .execute(
            "UPDATE graph_steps SET outcome=?1, ai_comments=?2, grade=?3, updated_at=?4 WHERE id=?5",
            params![outcome, ai_comments, grade, now, step_id],
        )
        .map_err(|e| McpError::Database {
            operation: "set step outcome".into(),
            cause: e.to_string(),
        })?;

    if n == 0 {
        return Err(McpError::NotFound {
            resource: "step".into(),
            id: step_id.to_string(),
        });
    }

    Ok(json!({ "updated": true }))
}

pub fn set_phase_outcome(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let phase_id = get_str(params, "phase_id")?;
    let outcome = get_str(params, "outcome")?;
    let ai_comments = get_str(params, "ai_comments")?;
    let grade = super::helpers::get_i64_opt(params, "grade");
    let now = now_iso();

    let n = conn
        .execute(
            "UPDATE graph_phases SET outcome=?1, ai_comments=?2, grade=?3, updated_at=?4 WHERE id=?5",
            params![outcome, ai_comments, grade, now, phase_id],
        )
        .map_err(|e| McpError::Database {
            operation: "set phase outcome".into(),
            cause: e.to_string(),
        })?;

    if n == 0 {
        return Err(McpError::NotFound {
            resource: "phase".into(),
            id: phase_id.to_string(),
        });
    }

    Ok(json!({ "updated": true }))
}

pub fn list_graph_phases(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let graph_id = get_str(params, "graph_id")?;

    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM graph_phases WHERE graph_id=?1 ORDER BY ordinal",
            PHASE_COLS
        ))
        .map_err(|e| McpError::Database {
            operation: "prepare phase list query".into(),
            cause: e.to_string(),
        })?;

    let phases: Vec<Value> = stmt
        .query_map(params![graph_id], phase_row_to_json)
        .map_err(|e| McpError::Database {
            operation: "query phase list".into(),
            cause: e.to_string(),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| McpError::Database {
            operation: "read phase list rows".into(),
            cause: e.to_string(),
        })?;

    Ok(json!({ "phases": phases }))
}

pub fn list_graph_steps(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let phase_id = get_str(params, "phase_id")?;

    let mut stmt = conn
        .prepare(&format!(
            "SELECT {} FROM graph_steps WHERE phase_id=?1 ORDER BY ordinal",
            STEP_COLS
        ))
        .map_err(|e| McpError::Database {
            operation: "prepare step list query".into(),
            cause: e.to_string(),
        })?;

    let steps: Vec<Value> = stmt
        .query_map(params![phase_id], step_row_to_json)
        .map_err(|e| McpError::Database {
            operation: "query step list".into(),
            cause: e.to_string(),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| McpError::Database {
            operation: "read step list rows".into(),
            cause: e.to_string(),
        })?;

    Ok(json!({ "steps": steps }))
}

pub fn get_graph_progress(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let graph_id = get_str(params, "graph_id")?;

    let graph: Value = conn
        .query_row(
            &format!("SELECT {} FROM grove_graphs WHERE id=?1", GRAPH_COLS),
            params![graph_id],
            graph_row_to_json,
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query graph".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "graph".into(),
            id: graph_id.to_string(),
        })?;

    let mut phase_stmt = conn
        .prepare(&format!(
            "SELECT {} FROM graph_phases WHERE graph_id=?1 ORDER BY ordinal",
            PHASE_COLS
        ))
        .map_err(|e| McpError::Database {
            operation: "prepare graph phases query".into(),
            cause: e.to_string(),
        })?;

    let phases: Vec<Value> = phase_stmt
        .query_map(params![graph_id], phase_row_to_json)
        .map_err(|e| McpError::Database {
            operation: "query graph phases".into(),
            cause: e.to_string(),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| McpError::Database {
            operation: "read graph phase rows".into(),
            cause: e.to_string(),
        })?;

    let mut step_stmt = conn
        .prepare(&format!(
            "SELECT {} FROM graph_steps WHERE graph_id=?1 ORDER BY ordinal",
            STEP_COLS
        ))
        .map_err(|e| McpError::Database {
            operation: "prepare graph steps query".into(),
            cause: e.to_string(),
        })?;

    let all_steps: Vec<Value> = step_stmt
        .query_map(params![graph_id], step_row_to_json)
        .map_err(|e| McpError::Database {
            operation: "query graph steps".into(),
            cause: e.to_string(),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| McpError::Database {
            operation: "read graph step rows".into(),
            cause: e.to_string(),
        })?;

    let mut phases_with_steps: Vec<Value> = Vec::new();
    for phase in &phases {
        let phase_id_val = phase.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let phase_steps: Vec<&Value> = all_steps
            .iter()
            .filter(|s| s.get("phase_id").and_then(|v| v.as_str()).unwrap_or("") == phase_id_val)
            .collect();

        phases_with_steps.push(json!({
            "phase": phase,
            "steps": phase_steps,
        }));
    }

    let mut total_steps: i64 = 0;
    let mut open_steps: i64 = 0;
    let mut inprogress_steps: i64 = 0;
    let mut closed_steps: i64 = 0;
    let mut failed_steps: i64 = 0;

    for step in &all_steps {
        total_steps += 1;
        match step.get("status").and_then(|v| v.as_str()).unwrap_or("") {
            "open" => open_steps += 1,
            "inprogress" => inprogress_steps += 1,
            "closed" => closed_steps += 1,
            "failed" => failed_steps += 1,
            _ => {}
        }
    }

    let mut total_phases: i64 = 0;
    let mut open_phases: i64 = 0;
    let mut inprogress_phases: i64 = 0;
    let mut closed_phases: i64 = 0;
    let mut failed_phases: i64 = 0;

    for phase in &phases {
        total_phases += 1;
        match phase.get("status").and_then(|v| v.as_str()).unwrap_or("") {
            "open" => open_phases += 1,
            "inprogress" => inprogress_phases += 1,
            "closed" => closed_phases += 1,
            "failed" => failed_phases += 1,
            _ => {}
        }
    }

    Ok(json!({
        "graph": graph,
        "phases_with_steps": phases_with_steps,
        "counts": {
            "phases": {
                "total": total_phases,
                "open": open_phases,
                "inprogress": inprogress_phases,
                "closed": closed_phases,
                "failed": failed_phases,
            },
            "steps": {
                "total": total_steps,
                "open": open_steps,
                "inprogress": inprogress_steps,
                "closed": closed_steps,
                "failed": failed_steps,
            }
        }
    }))
}

pub fn get_step_pipeline_state(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let step_id = get_str(params, "step_id")?;

    let step: Value = conn
        .query_row(
            &format!("SELECT {} FROM graph_steps WHERE id=?1", STEP_COLS),
            params![step_id],
            step_row_to_json,
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query step pipeline state".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "step".into(),
            id: step_id.to_string(),
        })?;

    let judge_feedback_str = step
        .get("judge_feedback_json")
        .and_then(|v| v.as_str())
        .unwrap_or("[]");
    let judge_feedback: Value =
        serde_json::from_str(judge_feedback_str).unwrap_or_else(|_| json!([]));

    let status = step.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let grade = step.get("grade").and_then(|v| v.as_i64());
    let pipeline_stage = if status == "failed" {
        "failed"
    } else if grade.is_some() && status == "closed" {
        "done"
    } else if status == "inprogress" {
        "building"
    } else {
        "pending"
    };

    Ok(json!({
        "step": step,
        "judge_feedback": judge_feedback,
        "pipeline_stage": pipeline_stage,
    }))
}

pub fn get_step_dependencies_status(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let step_id = get_str(params, "step_id")?;

    let depends_on_json: String = conn
        .query_row(
            "SELECT depends_on_json FROM graph_steps WHERE id=?1",
            params![step_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query step depends_on_json".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "step".into(),
            id: step_id.to_string(),
        })?;

    let deps: Vec<String> =
        serde_json::from_str::<Vec<String>>(&depends_on_json).unwrap_or_default();

    if deps.is_empty() {
        return Ok(json!({
            "ready": true,
            "pending_deps": []
        }));
    }

    let mut pending = Vec::new();
    for dep_id in &deps {
        let status: Option<String> = conn
            .query_row(
                "SELECT status FROM graph_steps WHERE id=?1",
                params![dep_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| McpError::Database {
                operation: format!("check step dependency {dep_id}"),
                cause: e.to_string(),
            })?;

        match status {
            Some(s) if s == "closed" => {}
            Some(s) => pending.push(json!({ "id": dep_id, "status": s })),
            None => pending.push(json!({ "id": dep_id, "status": "not_found" })),
        }
    }

    Ok(json!({
        "ready": pending.is_empty(),
        "pending_deps": pending
    }))
}

pub fn check_runtime_status(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let graph_id = get_str(params, "graph_id")?;

    let runtime_status: String = conn
        .query_row(
            "SELECT runtime_status FROM grove_graphs WHERE id=?1",
            params![graph_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query graph runtime status".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "graph".into(),
            id: graph_id.to_string(),
        })?;

    Ok(json!({ "runtime_status": runtime_status }))
}
