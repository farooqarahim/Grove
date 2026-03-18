use crate::errors::McpError;
use serde_json::Value;

pub fn now_iso() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

pub fn gen_id(prefix: &str) -> String {
    format!(
        "{}_{}",
        prefix,
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

pub fn get_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, McpError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::InvalidParams {
            message: format!("missing required parameter: {}", key),
        })
}

pub fn get_str_opt<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

pub fn get_i64(params: &Value, key: &str) -> Result<i64, McpError> {
    params
        .get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| McpError::InvalidParams {
            message: format!("missing required parameter: {}", key),
        })
}

pub fn get_i64_opt(params: &Value, key: &str) -> Option<i64> {
    params.get(key).and_then(|v| v.as_i64())
}

pub fn get_bool_opt(params: &Value, key: &str) -> Option<bool> {
    params.get(key).and_then(|v| v.as_bool())
}

pub const GRAPH_COLS: &str = "\
    id, conversation_id, title, description, status, runtime_status, \
    parsing_status, execution_mode, active, rerun_count, max_reruns, \
    phases_created_count, steps_created_count, current_phase, next_step, \
    progress_summary, source_document_path, git_branch, git_commit_sha, \
    git_pr_url, git_merge_status, created_at, updated_at";

pub const PHASE_COLS: &str = "\
    id, graph_id, task_name, task_objective, outcome, ai_comments, grade, \
    reference_doc_path, ref_required, status, validation_status, ordinal, \
    depends_on_json, git_commit_sha, conversation_id, created_run_id, \
    executed_run_id, validator_run_id, judge_run_id, execution_agent, \
    created_at, updated_at";

pub const STEP_COLS: &str = "\
    id, phase_id, graph_id, task_name, task_objective, step_type, outcome, \
    ai_comments, grade, reference_doc_path, ref_required, status, ordinal, \
    execution_mode, depends_on_json, run_iteration, max_iterations, \
    judge_feedback_json, builder_run_id, verdict_run_id, judge_run_id, \
    conversation_id, created_run_id, executed_run_id, execution_agent, \
    created_at, updated_at";

pub fn graph_row_to_json(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    let active_int: i64 = r.get(8)?;
    Ok(serde_json::json!({
        "id": r.get::<_, String>(0)?,
        "conversation_id": r.get::<_, String>(1)?,
        "title": r.get::<_, String>(2)?,
        "description": r.get::<_, Option<String>>(3)?,
        "status": r.get::<_, String>(4)?,
        "runtime_status": r.get::<_, String>(5)?,
        "parsing_status": r.get::<_, String>(6)?,
        "execution_mode": r.get::<_, String>(7)?,
        "active": active_int != 0,
        "rerun_count": r.get::<_, i64>(9)?,
        "max_reruns": r.get::<_, i64>(10)?,
        "phases_created_count": r.get::<_, i64>(11)?,
        "steps_created_count": r.get::<_, i64>(12)?,
        "current_phase": r.get::<_, Option<String>>(13)?,
        "next_step": r.get::<_, Option<String>>(14)?,
        "progress_summary": r.get::<_, Option<String>>(15)?,
        "source_document_path": r.get::<_, Option<String>>(16)?,
        "git_branch": r.get::<_, Option<String>>(17)?,
        "git_commit_sha": r.get::<_, Option<String>>(18)?,
        "git_pr_url": r.get::<_, Option<String>>(19)?,
        "git_merge_status": r.get::<_, Option<String>>(20)?,
        "created_at": r.get::<_, String>(21)?,
        "updated_at": r.get::<_, String>(22)?,
    }))
}

pub fn phase_row_to_json(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    let ref_req_int: i64 = r.get(8)?;
    Ok(serde_json::json!({
        "id": r.get::<_, String>(0)?,
        "graph_id": r.get::<_, String>(1)?,
        "task_name": r.get::<_, String>(2)?,
        "task_objective": r.get::<_, String>(3)?,
        "outcome": r.get::<_, Option<String>>(4)?,
        "ai_comments": r.get::<_, Option<String>>(5)?,
        "grade": r.get::<_, Option<i64>>(6)?,
        "reference_doc_path": r.get::<_, Option<String>>(7)?,
        "ref_required": ref_req_int != 0,
        "status": r.get::<_, String>(9)?,
        "validation_status": r.get::<_, String>(10)?,
        "ordinal": r.get::<_, i64>(11)?,
        "depends_on_json": r.get::<_, String>(12)?,
        "git_commit_sha": r.get::<_, Option<String>>(13)?,
        "conversation_id": r.get::<_, Option<String>>(14)?,
        "created_run_id": r.get::<_, Option<String>>(15)?,
        "executed_run_id": r.get::<_, Option<String>>(16)?,
        "validator_run_id": r.get::<_, Option<String>>(17)?,
        "judge_run_id": r.get::<_, Option<String>>(18)?,
        "execution_agent": r.get::<_, Option<String>>(19)?,
        "created_at": r.get::<_, String>(20)?,
        "updated_at": r.get::<_, String>(21)?,
    }))
}

pub fn step_row_to_json(r: &rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value> {
    let ref_req_int: i64 = r.get(10)?;
    Ok(serde_json::json!({
        "id": r.get::<_, String>(0)?,
        "phase_id": r.get::<_, String>(1)?,
        "graph_id": r.get::<_, String>(2)?,
        "task_name": r.get::<_, String>(3)?,
        "task_objective": r.get::<_, String>(4)?,
        "step_type": r.get::<_, String>(5)?,
        "outcome": r.get::<_, Option<String>>(6)?,
        "ai_comments": r.get::<_, Option<String>>(7)?,
        "grade": r.get::<_, Option<i64>>(8)?,
        "reference_doc_path": r.get::<_, Option<String>>(9)?,
        "ref_required": ref_req_int != 0,
        "status": r.get::<_, String>(11)?,
        "ordinal": r.get::<_, i64>(12)?,
        "execution_mode": r.get::<_, String>(13)?,
        "depends_on_json": r.get::<_, String>(14)?,
        "run_iteration": r.get::<_, i64>(15)?,
        "max_iterations": r.get::<_, i64>(16)?,
        "judge_feedback_json": r.get::<_, String>(17)?,
        "builder_run_id": r.get::<_, Option<String>>(18)?,
        "verdict_run_id": r.get::<_, Option<String>>(19)?,
        "judge_run_id": r.get::<_, Option<String>>(20)?,
        "conversation_id": r.get::<_, Option<String>>(21)?,
        "created_run_id": r.get::<_, Option<String>>(22)?,
        "executed_run_id": r.get::<_, Option<String>>(23)?,
        "execution_agent": r.get::<_, Option<String>>(24)?,
        "created_at": r.get::<_, String>(25)?,
        "updated_at": r.get::<_, String>(26)?,
    }))
}
