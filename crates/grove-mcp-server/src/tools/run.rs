use crate::errors::McpError;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use super::helpers::{get_i64_opt, get_str, get_str_opt, now_iso};

pub fn run_get_context(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let run = conn
        .query_row(
            "SELECT objective, state, current_agent, conversation_id FROM runs WHERE id=?1",
            [run_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query run context".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "run".into(),
            id: run_id.to_string(),
        })?;

    let checkpoints = run_checkpoints(conn, run_id)?;
    let artifacts = run_artifacts(conn, run_id, None)?;
    let recent_messages = if let Some(ref conversation_id) = run.3 {
        let mut stmt = conn
            .prepare(
                "SELECT role, agent_type, content
                 FROM messages WHERE conversation_id=?1
                 ORDER BY created_at DESC LIMIT 6",
            )
            .map_err(|e| McpError::Database {
                operation: "prepare recent messages query".into(),
                cause: e.to_string(),
            })?;
        let rows = stmt
            .query_map([conversation_id], |r| {
                Ok(json!({
                    "role": r.get::<_, String>(0)?,
                    "agent_type": r.get::<_, Option<String>>(1)?,
                    "content": r.get::<_, String>(2)?,
                }))
            })
            .map_err(|e| McpError::Database {
                operation: "query recent messages".into(),
                cause: e.to_string(),
            })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| McpError::Database {
                operation: "read message row".into(),
                cause: e.to_string(),
            })?);
        }
        out.reverse();
        out
    } else {
        Vec::new()
    };

    Ok(json!({
        "run_id": run_id,
        "objective": run.0,
        "state": run.1,
        "current_agent": run.2,
        "conversation_id": run.3,
        "checkpoints": checkpoints,
        "artifacts": artifacts,
        "recent_messages": recent_messages,
    }))
}

pub fn run_get_current_phase(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let row = conn
        .query_row(
            "SELECT state, pipeline, current_agent FROM runs WHERE id=?1",
            [run_id],
            |r| {
                Ok(json!({
                    "run_id": run_id,
                    "state": r.get::<_, String>(0)?,
                    "pipeline": r.get::<_, Option<String>>(1)?,
                    "current_agent": r.get::<_, Option<String>>(2)?,
                }))
            },
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query run current phase".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "run".into(),
            id: run_id.to_string(),
        })?;
    let pending = conn
        .query_row(
            "SELECT id, agent, artifact_path, created_at
             FROM phase_checkpoints WHERE run_id=?1 AND status='pending'
             ORDER BY id DESC LIMIT 1",
            [run_id],
            |r| {
                Ok(json!({
                    "checkpoint_id": r.get::<_, i64>(0)?,
                    "agent": r.get::<_, String>(1)?,
                    "artifact_path": r.get::<_, Option<String>>(2)?,
                    "created_at": r.get::<_, String>(3)?,
                }))
            },
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query pending phase checkpoint".into(),
            cause: e.to_string(),
        })?;
    Ok(json!({
        "run": row,
        "pending_gate": pending,
    }))
}

pub fn run_get_phase_artifacts(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let agent = get_str_opt(params, "agent");
    Ok(json!({
        "run_id": run_id,
        "artifacts": run_artifacts(conn, run_id, agent)?,
    }))
}

pub fn run_record_artifact(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let agent = get_str(params, "agent")?;
    let filename = get_str(params, "filename")?;
    let content_hash = get_str_opt(params, "content_hash").unwrap_or("");
    let size_bytes = get_i64_opt(params, "size_bytes").unwrap_or(0);
    conn.execute(
        "INSERT INTO run_artifacts (run_id, agent, filename, content_hash, size_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![run_id, agent, filename, content_hash, size_bytes],
    )
    .map_err(|e| McpError::Database {
        operation: "insert run artifact".into(),
        cause: e.to_string(),
    })?;
    Ok(json!({
        "run_id": run_id,
        "artifact_id": conn.last_insert_rowid(),
        "agent": agent,
        "filename": filename,
    }))
}

pub fn run_request_gate(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let agent = get_str(params, "agent")?;
    let artifact_path = get_str_opt(params, "artifact_path");
    conn.execute(
        "INSERT INTO phase_checkpoints (run_id, agent, status, artifact_path)
         VALUES (?1, ?2, 'pending', ?3)",
        params![run_id, agent, artifact_path],
    )
    .map_err(|e| McpError::Database {
        operation: "insert phase checkpoint gate".into(),
        cause: e.to_string(),
    })?;
    Ok(json!({
        "run_id": run_id,
        "checkpoint_id": conn.last_insert_rowid(),
        "status": "pending",
    }))
}

pub async fn run_wait_for_gate(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let timeout_ms = get_i64_opt(params, "timeout_ms").unwrap_or(900_000).max(1) as u64;
    let checkpoint = conn
        .query_row(
            "SELECT id FROM phase_checkpoints WHERE run_id=?1 AND status='pending' ORDER BY id DESC LIMIT 1",
            [run_id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query pending checkpoint for gate wait".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "pending gate".into(),
            id: run_id.to_string(),
        })?;
    let start = Instant::now();
    loop {
        let decision = conn
            .query_row(
                "SELECT status, decision, decided_at FROM phase_checkpoints WHERE id=?1",
                [checkpoint],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .map_err(|e| McpError::Database {
                operation: "poll checkpoint decision".into(),
                cause: e.to_string(),
            })?;
        if decision.0 != "pending" {
            return Ok(json!({
                "run_id": run_id,
                "checkpoint_id": checkpoint,
                "decision": decision.0,
                "notes": decision.1,
                "decided_at": decision.2,
            }));
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            return Err(McpError::Timeout {
                operation: format!("gate decision for run {run_id}"),
                elapsed_secs: timeout_ms / 1000,
            });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

pub fn run_get_next_step(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let row = conn
        .query_row(
            "SELECT pipeline, current_agent, state FROM runs WHERE id=?1",
            [run_id],
            |r| {
                Ok(json!({
                    "run_id": run_id,
                    "pipeline": r.get::<_, Option<String>>(0)?,
                    "current_agent": r.get::<_, Option<String>>(1)?,
                    "state": r.get::<_, String>(2)?,
                }))
            },
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query run next step".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "run".into(),
            id: run_id.to_string(),
        })?;
    Ok(row)
}

pub fn run_complete_phase(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let agent = get_str(params, "agent")?;
    conn.execute(
        "UPDATE runs SET current_agent=?1, updated_at=?2 WHERE id=?3",
        params![agent, now_iso(), run_id],
    )
    .map_err(|e| McpError::Database {
        operation: "update run current agent".into(),
        cause: e.to_string(),
    })?;
    Ok(json!({
        "run_id": run_id,
        "current_agent": agent,
        "updated": true,
    }))
}

pub fn run_abort_check(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let state = conn
        .query_row("SELECT state FROM runs WHERE id=?1", [run_id], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query run state for abort check".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "run".into(),
            id: run_id.to_string(),
        })?;
    Ok(json!({
        "run_id": run_id,
        "state": state,
        "aborted": state == "aborted",
        "paused": state == "paused" || state == "waiting_for_gate",
    }))
}

pub fn run_budget_status(conn: &Connection, params: &Value) -> Result<Value, McpError> {
    let run_id = get_str(params, "run_id")?;
    let row = conn
        .query_row(
            "SELECT budget_usd, cost_used_usd, state FROM runs WHERE id=?1",
            [run_id],
            |r| {
                Ok((
                    r.get::<_, f64>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|e| McpError::Database {
            operation: "query run budget status".into(),
            cause: e.to_string(),
        })?
        .ok_or_else(|| McpError::NotFound {
            resource: "run".into(),
            id: run_id.to_string(),
        })?;
    let remaining = (row.0 - row.1).max(0.0);
    Ok(json!({
        "run_id": run_id,
        "budget_usd": row.0,
        "cost_used_usd": row.1,
        "remaining_usd": remaining,
        "state": row.2,
    }))
}

fn run_artifacts(
    conn: &Connection,
    run_id: &str,
    agent: Option<&str>,
) -> Result<Vec<Value>, McpError> {
    let mut out = Vec::new();
    if let Some(agent) = agent {
        let mut stmt = conn
            .prepare(
                "SELECT id, agent, filename, content_hash, size_bytes, created_at
                 FROM run_artifacts WHERE run_id=?1 AND agent=?2 ORDER BY id ASC",
            )
            .map_err(|e| McpError::Database {
                operation: "prepare run artifacts by agent query".into(),
                cause: e.to_string(),
            })?;
        let rows = stmt
            .query_map(params![run_id, agent], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "agent": r.get::<_, String>(1)?,
                    "filename": r.get::<_, String>(2)?,
                    "content_hash": r.get::<_, String>(3)?,
                    "size_bytes": r.get::<_, i64>(4)?,
                    "created_at": r.get::<_, String>(5)?,
                }))
            })
            .map_err(|e| McpError::Database {
                operation: "query run artifacts by agent".into(),
                cause: e.to_string(),
            })?;
        for row in rows {
            out.push(row.map_err(|e| McpError::Database {
                operation: "read run artifact row".into(),
                cause: e.to_string(),
            })?);
        }
    } else {
        let mut stmt = conn
            .prepare(
                "SELECT id, agent, filename, content_hash, size_bytes, created_at
                 FROM run_artifacts WHERE run_id=?1 ORDER BY id ASC",
            )
            .map_err(|e| McpError::Database {
                operation: "prepare run artifacts query".into(),
                cause: e.to_string(),
            })?;
        let rows = stmt
            .query_map(params![run_id], |r| {
                Ok(json!({
                    "id": r.get::<_, i64>(0)?,
                    "agent": r.get::<_, String>(1)?,
                    "filename": r.get::<_, String>(2)?,
                    "content_hash": r.get::<_, String>(3)?,
                    "size_bytes": r.get::<_, i64>(4)?,
                    "created_at": r.get::<_, String>(5)?,
                }))
            })
            .map_err(|e| McpError::Database {
                operation: "query run artifacts".into(),
                cause: e.to_string(),
            })?;
        for row in rows {
            out.push(row.map_err(|e| McpError::Database {
                operation: "read run artifact row".into(),
                cause: e.to_string(),
            })?);
        }
    }
    Ok(out)
}

fn run_checkpoints(conn: &Connection, run_id: &str) -> Result<Vec<Value>, McpError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent, status, decision, artifact_path, created_at, decided_at
             FROM phase_checkpoints WHERE run_id=?1 ORDER BY id ASC",
        )
        .map_err(|e| McpError::Database {
            operation: "prepare run checkpoints query".into(),
            cause: e.to_string(),
        })?;
    let rows = stmt
        .query_map([run_id], |r| {
            Ok(json!({
                "id": r.get::<_, i64>(0)?,
                "agent": r.get::<_, String>(1)?,
                "status": r.get::<_, String>(2)?,
                "decision": r.get::<_, Option<String>>(3)?,
                "artifact_path": r.get::<_, Option<String>>(4)?,
                "created_at": r.get::<_, String>(5)?,
                "decided_at": r.get::<_, Option<String>>(6)?,
            }))
        })
        .map_err(|e| McpError::Database {
            operation: "query run checkpoints".into(),
            cause: e.to_string(),
        })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| McpError::Database {
            operation: "read checkpoint row".into(),
            cause: e.to_string(),
        })?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use serde_json::json;

    fn setup_run_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE runs (
                id TEXT PRIMARY KEY,
                objective TEXT NOT NULL,
                state TEXT NOT NULL,
                budget_usd REAL NOT NULL DEFAULT 0,
                cost_used_usd REAL NOT NULL DEFAULT 0,
                pipeline TEXT,
                current_agent TEXT,
                conversation_id TEXT,
                updated_at TEXT DEFAULT '',
                created_at TEXT DEFAULT ''
            );
            CREATE TABLE messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT,
                run_id TEXT,
                role TEXT NOT NULL,
                agent_type TEXT,
                session_id TEXT,
                content TEXT NOT NULL,
                created_at TEXT DEFAULT '',
                user_id TEXT
            );
            CREATE TABLE phase_checkpoints (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                agent TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                decision TEXT,
                decided_at TEXT,
                artifact_path TEXT,
                created_at TEXT DEFAULT ''
            );
            CREATE TABLE run_artifacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                agent TEXT NOT NULL,
                filename TEXT NOT NULL,
                content_hash TEXT NOT NULL DEFAULT '',
                size_bytes INTEGER NOT NULL DEFAULT 0,
                created_at TEXT DEFAULT ''
            );
            ",
        )
        .unwrap();
        conn
    }

    #[test]
    fn run_context_reports_artifacts_and_checkpoints() {
        let conn = setup_run_db();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, current_agent, conversation_id)
             VALUES ('run1', 'ship feature', 'executing', 10.0, 1.5, 'reviewer', 'conv1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, run_id, role, agent_type, content)
             VALUES ('m1', 'conv1', 'run1', 'agent', 'build_prd', 'Drafted PRD')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO phase_checkpoints (run_id, agent, status, decision, artifact_path)
             VALUES ('run1', 'build_prd', 'approved', 'looks good', 'PRD.md')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO run_artifacts (run_id, agent, filename, content_hash, size_bytes)
             VALUES ('run1', 'build_prd', 'PRD.md', 'abc', 42)",
            [],
        )
        .unwrap();

        let result = run_get_context(&conn, &json!({ "run_id": "run1" })).unwrap();
        assert_eq!(result["objective"], "ship feature");
        assert_eq!(result["current_agent"], "reviewer");
        assert_eq!(result["checkpoints"].as_array().unwrap().len(), 1);
        assert_eq!(result["artifacts"].as_array().unwrap().len(), 1);
        assert_eq!(result["recent_messages"].as_array().unwrap().len(), 1);
    }
}
