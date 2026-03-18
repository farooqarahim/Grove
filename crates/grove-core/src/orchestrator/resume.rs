use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use serde_json::json;

use crate::checkpoint;
use crate::config::GroveConfig;
use crate::errors::{GroveError, GroveResult};
use crate::events;
use crate::providers::Provider;
use crate::reporting;

use super::{RunExecutionResult, RunState, engine, transitions};

/// Resume a paused or failed run from its latest checkpoint.
///
/// Restores `pending_tasks` from the checkpoint payload and re-runs the
/// remaining agent plan via `engine::run_agents`.
pub fn resume_from_checkpoint(
    conn: &mut Connection,
    run_id: &str,
    project_root: &Path,
    cfg: &GroveConfig,
    provider: Arc<dyn Provider>,
    current_state: RunState,
) -> GroveResult<RunExecutionResult> {
    let cp = checkpoint::latest_for_run(conn, run_id)?
        .ok_or_else(|| GroveError::NotFound(format!("no checkpoint found for run {run_id}")))?;

    let objective = cp
        .pending_tasks
        .first()
        .cloned()
        .unwrap_or_else(|| "unknown objective".to_string());

    transitions::apply_transition(conn, run_id, current_state, RunState::Executing)?;

    events::emit(
        conn,
        run_id,
        None,
        "run_resumed",
        json!({ "stage": cp.stage, "pending_tasks": cp.pending_tasks }),
    )?;

    // Read the original pipeline settings from the run record.
    let (pipeline_str, disable_phase_gates, provider_thread_id): (
        Option<String>,
        bool,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT pipeline, disable_phase_gates, provider_thread_id FROM runs WHERE id=?1",
            [run_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, Option<bool>>(1)?.unwrap_or(false),
                    r.get(2)?,
                ))
            },
        )
        .unwrap_or((None, false, None));
    let run_intent = super::intent::resume_run_intent(&objective, pipeline_str.as_deref());
    let plan = run_intent.plan.clone();
    let pause_after =
        super::effective_pause_after(&[], &run_intent.phase_gates, disable_phase_gates);

    // Look up conversation_id from the run row (may be NULL for pre-0010 runs).
    let conversation_id: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [run_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();

    let null_sink = crate::providers::NullSink;
    engine::run_agents(
        conn,
        run_id,
        &objective,
        &plan,
        Arc::clone(&provider),
        cfg,
        project_root,
        None,
        Some(run_intent.shared_context.as_str()),
        Some(&run_intent.agent_briefs),
        false, // interactive
        &pause_after,
        None, // plan_steps — resume uses legacy in-memory plan
        conversation_id.as_deref(),
        None,               // abort_handle — resume path has no GUI abort
        provider_thread_id, // initial_provider_session_id — resume the coding agent conversation
        &null_sink,
        None, // input_handle_callback — resume path has no live Q&A
        None, // run_control_callback — resume path has no live gate control
    )?;

    if let Some(ref conversation_id) = conversation_id {
        if let Err(err) = super::run_memory::write_verdict_log(
            project_root,
            conversation_id,
            run_id,
            &objective,
            &run_intent,
            RunState::Completed.as_str(),
            None,
            None,
        ) {
            tracing::warn!(
                run_id = %run_id,
                conversation_id = %conversation_id,
                error = %err,
                "failed to write classic run verdict log after resume"
            );
        }
    }

    let report = reporting::generate_report_with_conn(conn, project_root, run_id)?;

    let flat_plan: Vec<String> = plan
        .iter()
        .flat_map(|s| s.iter().map(|a| a.as_str().to_string()))
        .collect();

    Ok(RunExecutionResult {
        run_id: run_id.to_string(),
        state: RunState::Completed.as_str().to_string(),
        objective,
        report_path: Some(report.to_string_lossy().to_string()),
        plan: flat_plan,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn resume_reads_pipeline_from_db() {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let handle = crate::db::DbHandle::new(dir.path());
        let conn = handle.connect().unwrap();

        // Insert a run with pipeline="plan", state="paused", and provider_thread_id
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, pipeline, provider_thread_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params!["run-123", "test", "paused", 10.0, 0.0, "plan", "thread-abc", now, now],
        )
        .unwrap();

        // Read pipeline, disable_phase_gates, and provider_thread_id back
        let (pipeline_str, disable_phase_gates, provider_thread_id): (
            Option<String>,
            bool,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT pipeline, disable_phase_gates, provider_thread_id FROM runs WHERE id=?1",
                ["run-123"],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get::<_, Option<bool>>(1)?.unwrap_or(false),
                        r.get(2)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(pipeline_str.as_deref(), Some("plan"));
        assert!(!disable_phase_gates);
        assert_eq!(provider_thread_id.as_deref(), Some("thread-abc"));

        // Verify PipelineKind round-trip
        let kind = pipeline_str
            .as_deref()
            .and_then(crate::orchestrator::pipeline::PipelineKind::from_str)
            .unwrap_or_default();
        assert_eq!(kind, crate::orchestrator::pipeline::PipelineKind::Plan);
    }
}
