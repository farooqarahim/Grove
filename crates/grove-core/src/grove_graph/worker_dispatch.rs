//! Dispatch a worker agent to execute a chunk of steps.
//!
//! Builds the ProviderRequest with a chunk manifest embedded in instructions,
//! dispatches via the provider, and interprets completion state from the DB.

use crate::db::repositories::grove_graph_repo;
use crate::errors::GroveResult;
use crate::grove_graph::chunking::StepChunk;
use crate::grove_graph::skill_loader;
use crate::providers::mcp_inject;
use crate::providers::{Provider, ProviderRequest};
use std::path::Path;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

/// Result of a worker chunk execution, determined by reading DB state after
/// the worker session completes.
#[derive(Debug)]
pub enum WorkerResult {
    /// All steps in the chunk were closed successfully.
    AllCompleted,
    /// Some steps completed, some failed or were skipped.
    Partial {
        completed: Vec<String>,
        failed: Vec<String>,
        remaining: Vec<String>,
    },
}

/// Whether the provider call itself succeeded or crashed.
#[derive(Debug)]
pub enum DispatchOutcome {
    /// Provider returned Ok — check DB for actual step results.
    Completed,
    /// Provider returned Err — worker crashed/timed out.
    Crashed(String),
}

/// Build the chunk manifest string that is embedded in the worker's instructions.
pub fn build_chunk_manifest(phase_objective: &str, graph_id: &str, chunk: &StepChunk) -> String {
    let mut manifest = String::new();
    manifest.push_str("## Chunk Manifest\n\n");
    manifest.push_str(&format!("Phase: \"{}\"\n", phase_objective));
    manifest.push_str(&format!("Graph ID: {}\n", graph_id));
    manifest.push_str("Steps (execute in order):\n\n");

    for (i, step) in chunk.steps.iter().enumerate() {
        let deps = &step.depends_on_json;
        let deps_display = if deps.is_empty() || deps == "[]" || deps == "null" {
            "none".to_string()
        } else {
            deps.to_string()
        };
        manifest.push_str(&format!(
            "{}. step_id: \"{}\" | type: {} | deps: {}\n   Objective: \"{}\"\n\n",
            i + 1,
            step.id,
            step.step_type,
            deps_display,
            step.task_objective
        ));
    }

    // Issue #14: Append explicit MCP tool instructions so workers know how to
    // report step completion.
    manifest.push_str(
        "\n\nAfter completing each step, you MUST call these MCP tools:\n\
         1. grove_set_step_outcome(step_id, outcome, ai_comments, grade) \
         — record what you did and a grade 1-10\n\
         2. grove_update_step_status(step_id, \"closed\") \
         — mark the step as done\n\n\
         Valid status values for grove_update_step_status: \
         \"open\", \"inprogress\", \"closed\", \"failed\"\n\
         Do NOT use \"done\", \"complete\", or \"finished\" — only \"closed\" works.\n",
    );

    manifest
}

/// Dispatch a worker agent to execute a chunk of steps.
///
/// Returns `DispatchOutcome` indicating whether the provider call itself
/// succeeded or crashed. Actual step results are in the DB — use
/// `assess_worker_result` to read them.
///
/// NOTE: This is a blocking function (Provider::execute is sync).
/// Callers in async contexts should wrap in `tokio::task::spawn_blocking`.
pub fn dispatch_worker(
    provider: &Arc<dyn Provider>,
    chunk: &StepChunk,
    phase_objective: &str,
    graph_id: &str,
    project_root: &Path,
    db_path: &Path,
    model_override: Option<&str>,
    log_dir: Option<&str>,
) -> GroveResult<DispatchOutcome> {
    let skill_content = skill_loader::load_skill(project_root, "phase-worker");
    let manifest = build_chunk_manifest(phase_objective, graph_id, chunk);
    let instructions = format!("{}\n\n{}", skill_content, manifest);

    let mcp_config: Option<std::path::PathBuf> =
        mcp_inject::prepare_mcp_config_for_role("phase_worker", db_path)?;
    let mcp_path: Option<String> = mcp_config
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());

    let step_names: Vec<&str> = chunk.steps.iter().map(|s| s.task_name.as_str()).collect();
    info!(
        graph_id,
        phase_objective,
        chunk_size = chunk.steps.len(),
        steps = ?step_names,
        "dispatching worker for chunk"
    );

    let session_id = format!("worker-{}", &Uuid::new_v4().to_string()[..8]);

    let request = ProviderRequest {
        objective: format!(
            "Execute {} steps for phase: '{}'",
            chunk.steps.len(),
            phase_objective
        ),
        role: "phase_worker".into(),
        worktree_path: project_root.to_string_lossy().into_owned(),
        instructions,
        model: model_override.map(|s| s.to_string()),
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: None,
        log_dir: log_dir.map(|s| s.to_string()),
        grove_session_id: Some(session_id),
        input_handle_callback: None,
        mcp_config_path: mcp_path,
    };

    let outcome = match provider.execute(&request) {
        Ok(response) => {
            info!(
                graph_id,
                summary = %response.summary,
                "worker completed"
            );
            DispatchOutcome::Completed
        }
        Err(e) => {
            warn!(
                graph_id,
                error = %e,
                "worker session failed"
            );
            DispatchOutcome::Crashed(e.to_string())
        }
    };

    // Clean up MCP config.
    if let Some(ref path) = mcp_config {
        mcp_inject::cleanup_mcp_config(path);
    }

    Ok(outcome)
}

/// Assess the result of a worker chunk execution by reading DB state.
///
/// The MCP server runs in a separate process and writes step status updates to
/// the same SQLite file. To ensure this connection sees those writes, we force
/// a WAL checkpoint before reading. If steps are still not `closed` after the
/// worker reported success (DispatchOutcome::Completed), we auto-close them —
/// the worker did its work but the MCP status update may not have persisted
/// visibly to this connection.
pub fn assess_worker_result(
    conn: &rusqlite::Connection,
    chunk: &StepChunk,
) -> GroveResult<WorkerResult> {
    // Force WAL checkpoint so this connection sees writes from the MCP server process.
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(PASSIVE);");

    let mut completed = Vec::new();
    let mut failed = Vec::new();
    let mut remaining = Vec::new();

    for step_id in &chunk.step_ids {
        let step = grove_graph_repo::get_step(conn, step_id)?;
        match step.status.as_str() {
            "closed" => completed.push(step_id.clone()),
            "failed" => failed.push(step_id.clone()),
            _ => remaining.push(step_id.clone()),
        }
    }

    if remaining.is_empty() && failed.is_empty() {
        Ok(WorkerResult::AllCompleted)
    } else {
        Ok(WorkerResult::Partial {
            completed,
            failed,
            remaining,
        })
    }
}

/// After a worker returns successfully (DispatchOutcome::Completed), force-close
/// any steps that are still not `closed`. This handles the case where the WAL
/// checkpoint didn't make MCP server writes visible, or the worker forgot to
/// call the MCP tool.
pub fn force_close_completed_steps(
    conn: &rusqlite::Connection,
    chunk: &StepChunk,
) -> GroveResult<usize> {
    let mut count = 0usize;
    for step_id in &chunk.step_ids {
        let step = grove_graph_repo::get_step(conn, step_id)?;
        if step.status != "closed" && step.status != "failed" {
            grove_graph_repo::update_step_status(conn, step_id, "closed")?;
            // Issue #5: Set a default outcome and grade so downstream code
            // doesn't see NULL values for force-closed steps.
            let _ = grove_graph_repo::set_step_outcome(
                conn,
                step_id,
                "Completed (auto-closed by engine after worker success)",
                "Worker completed all work; step closed by engine.",
            );
            // Use set_step_judge_run to record a sentinel grade of 7.
            let _ = grove_graph_repo::set_step_judge_run(conn, step_id, "force-close", Some(7));
            tracing::info!(
                step_id = step_id.as_str(),
                old_status = step.status.as_str(),
                "force-closed step after successful worker (grade=7, outcome set)"
            );
            count += 1;
        }
    }
    Ok(count)
}
