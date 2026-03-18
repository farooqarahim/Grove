use tauri::{Emitter as _, State};

use super::emit;
use crate::state::AppState;

// ── Streaming infrastructure ─────────────────────────────────────────────────

/// A `StreamSink` implementation that emits `grove://agent-output` events to the
/// Tauri frontend in real time.
///
/// The `run_id` is stored behind an `Arc<Mutex<String>>` so the `on_run_created`
/// callback can set it after the run record is inserted into the DB (but before
/// `engine::run_agents` starts emitting events).
pub struct TauriStreamSink {
    app_handle: tauri::AppHandle,
    run_id: std::sync::Arc<parking_lot::Mutex<String>>,
    pool: grove_core::db::DbPool,
}

impl TauriStreamSink {
    pub fn new(app_handle: tauri::AppHandle, pool: grove_core::db::DbPool) -> Self {
        Self {
            app_handle,
            run_id: std::sync::Arc::new(parking_lot::Mutex::new(String::new())),
            pool,
        }
    }

    /// Return a clone of the shared run_id Arc so external code can set it.
    pub fn run_id_handle(&self) -> std::sync::Arc<parking_lot::Mutex<String>> {
        std::sync::Arc::clone(&self.run_id)
    }
}

fn stream_event_kind(event: &grove_core::providers::StreamOutputEvent) -> &'static str {
    match event {
        grove_core::providers::StreamOutputEvent::System { .. } => "system",
        grove_core::providers::StreamOutputEvent::AssistantText { .. } => "assistant_text",
        grove_core::providers::StreamOutputEvent::ToolUse { .. } => "tool_use",
        grove_core::providers::StreamOutputEvent::ToolResult { .. } => "tool_result",
        grove_core::providers::StreamOutputEvent::Result { .. } => "result",
        grove_core::providers::StreamOutputEvent::RawLine { .. } => "raw_line",
        grove_core::providers::StreamOutputEvent::SkillLoaded { .. } => "skill_loaded",
        grove_core::providers::StreamOutputEvent::PhaseStart { .. } => "phase_start",
        grove_core::providers::StreamOutputEvent::PhaseGate { .. } => "phase_gate",
        grove_core::providers::StreamOutputEvent::PhaseEnd { .. } => "phase_end",
        grove_core::providers::StreamOutputEvent::Question { .. } => "question",
        grove_core::providers::StreamOutputEvent::UserAnswer { .. } => "user_answer",
        grove_core::providers::StreamOutputEvent::ScopeCheckPassed { .. } => "scope_check_passed",
        grove_core::providers::StreamOutputEvent::ScopeViolation { .. } => "scope_violation",
        grove_core::providers::StreamOutputEvent::ScopeRetry { .. } => "scope_retry",
    }
}

fn stream_event_session_id(event: &grove_core::providers::StreamOutputEvent) -> Option<&str> {
    match event {
        grove_core::providers::StreamOutputEvent::System { session_id, .. }
        | grove_core::providers::StreamOutputEvent::Result { session_id, .. } => {
            session_id.as_deref()
        }
        _ => None,
    }
}

fn persist_stream_event(
    conn: &rusqlite::Connection,
    run_id: &str,
    event: &grove_core::providers::StreamOutputEvent,
) {
    if run_id.is_empty()
        || matches!(
            event,
            grove_core::providers::StreamOutputEvent::RawLine { .. }
        )
    {
        return;
    }
    if let Ok(content_json) = serde_json::to_string(event) {
        let _ = grove_core::db::repositories::stream_events_repo::insert(
            conn,
            run_id,
            stream_event_session_id(event),
            stream_event_kind(event),
            &content_json,
        );
    }
}

impl grove_core::providers::StreamSink for TauriStreamSink {
    fn on_event(&self, event: grove_core::providers::StreamOutputEvent) {
        let run_id = self.run_id.lock().clone();
        let payload = serde_json::json!({
            "run_id": run_id,
            "event": event.clone(),
        });
        if let Err(e) = self.app_handle.emit("grove://agent-output", payload) {
            tracing::warn!(
                run_id = %run_id,
                error = %e,
                "failed to emit agent-output event"
            );
        }

        if let Ok(conn) = self.pool.get() {
            persist_stream_event(&conn, &run_id, &event);

            // Persist questions to DB so Thread can load them on reconnect
            if let grove_core::providers::StreamOutputEvent::Question {
                ref question,
                ref options,
                ..
            } = event
            {
                let options_json = if options.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(options).unwrap_or_default())
                };
                let _ = grove_core::db::repositories::qa_messages_repo::insert(
                    &conn,
                    &run_id,
                    None,
                    "question",
                    question,
                    options_json.as_deref(),
                );
                // Emit qa-message event so frontend invalidates qaMessages cache
                let _ = self.app_handle.emit(
                    "grove://qa-message",
                    serde_json::json!({ "run_id": run_id, "direction": "question" }),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::persist_stream_event;
    use grove_core::db::repositories::{qa_messages_repo, stream_events_repo};
    use grove_core::db::{DbHandle, initialize};
    use grove_core::providers::StreamOutputEvent;

    #[test]
    fn persist_stream_event_writes_structured_events_and_skips_raw_lines() {
        let tmp = std::env::temp_dir().join(format!("grove-stream-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp project dir");
        initialize(&tmp).expect("initialize db");
        let conn = DbHandle::new(&tmp).connect().expect("connect db");

        persist_stream_event(
            &conn,
            "run_1",
            &StreamOutputEvent::System {
                message: "started".into(),
                session_id: Some("sess_1".into()),
            },
        );
        persist_stream_event(
            &conn,
            "run_1",
            &StreamOutputEvent::RawLine {
                line: "{\"kind\":\"raw\"}".into(),
            },
        );

        let rows = stream_events_repo::list_for_run(&conn, "run_1", 0, 100).expect("list events");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, "system");
        assert_eq!(rows[0].session_id.as_deref(), Some("sess_1"));
        assert!(rows[0].content_json.contains("\"kind\":\"system\""));

        let qa_rows = qa_messages_repo::list_for_run(&conn, "run_1").expect("list questions");
        assert!(qa_rows.is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

// ── Stream events query ─────────────────────────────────────────────────────

#[tauri::command]
pub fn get_stream_events(
    state: State<'_, AppState>,
    run_id: String,
    after_id: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<grove_core::db::repositories::stream_events_repo::StreamEventRow>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::stream_events_repo::list_for_run(
        &conn,
        &run_id,
        after_id.unwrap_or(0),
        limit.unwrap_or(500),
    )
    .map_err(|e| e.to_string())
}

// ── Run artifacts query ─────────────────────────────────────────────────────

#[tauri::command]
pub fn get_run_artifacts(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<grove_core::db::repositories::run_artifacts_repo::RunArtifact>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::run_artifacts_repo::list_for_run(&conn, &run_id)
        .map_err(|e| e.to_string())
}

// ── Artifact content ────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_artifact_content(
    state: State<'_, AppState>,
    run_id: String,
    filename: String,
) -> Result<String, String> {
    // Resolve the run's worktree path from the sessions table (first session).
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let worktree: String = conn
        .query_row(
            "SELECT worktree FROM sessions WHERE run_id = ?1 LIMIT 1",
            [&run_id],
            |r| r.get(0),
        )
        .map_err(|e| format!("no session found for run {run_id}: {e}"))?;

    let file_path = std::path::Path::new(&worktree).join(&filename);
    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("failed to read artifact {}: {e}", file_path.display()))
}

// ── Q&A messaging ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn send_agent_message(
    state: State<'_, AppState>,
    run_id: String,
    content: String,
    session_id: Option<String>,
) -> Result<i64, String> {
    // 1. Write to live agent stdin if handle exists.
    {
        let mut inputs = state.agent_inputs.lock();
        if let Some(handle) = inputs.get_mut(&run_id) {
            if let Err(e) = handle.write_answer(&content) {
                tracing::warn!(run_id = %run_id, error = %e, "failed to write answer to agent stdin");
            }
        }
    }

    // 2. Persist to DB (always, for history/replay).
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let id = grove_core::db::repositories::qa_messages_repo::insert(
        &conn,
        &run_id,
        session_id.as_deref(),
        "answer",
        &content,
        None,
    )
    .map_err(|e| e.to_string())?;

    // 3. Emit event for frontend update.
    emit(
        &state.app_handle,
        "grove://qa-message",
        serde_json::json!({
            "run_id": run_id,
            "direction": "answer",
        }),
    );

    Ok(id)
}

#[tauri::command]
pub fn list_qa_messages(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<grove_core::db::repositories::qa_messages_repo::QaMessage>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::qa_messages_repo::list_for_run(&conn, &run_id)
        .map_err(|e| e.to_string())
}
