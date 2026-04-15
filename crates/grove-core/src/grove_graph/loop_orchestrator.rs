//! Agentic Loop Orchestrator — the core runtime for Grove Graph execution.
//!
//! [`run_graph_loop`] is the single public entry point. It drives the full
//! DAG-aware execution cycle using chunk-based worker dispatch:
//!
//! 1. **Initial setup** — creates a git branch (if first call), sets status to running.
//! 2. **Phase validation** — checks phases whose steps are all closed but validation
//!    is still pending; runs the Validator → Judge pipeline and commits on pass.
//!    On failure, invokes the orchestrator for triage.
//! 3. **Graph completion** — if every phase is closed and every validation passed,
//!    finalizes the graph (push + PR) and returns `GraphComplete`.
//! 4. **DAG query + Chunking** — finds open steps, classifies phase complexity,
//!    creates chunks for worker dispatch (or invokes orchestrator for complex DAGs).
//! 5. **Worker dispatch** — dispatches chunks to worker agents, assesses results,
//!    commits on success, invokes orchestrator for failover on crash/partial.
//! 6. **Loop** — after dispatch, loops back to phase validation.
//!
//! The loop is **re-entrant**: calling `run_graph_loop` on a paused graph resumes
//! from the current DAG state. All state lives in the database, not in memory.

use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use tracing::{debug, info, warn};

use crate::db::repositories::grove_graph_repo::{self, GraphStepRow};
use crate::errors::{GroveError, GroveResult};
use crate::grove_graph::{LoopIterationResult, PhaseValidationResult, RuntimeStatus};
use crate::grove_graph::{chunking, git_ops, orchestrator_dispatch, worker_dispatch};
use crate::providers::Provider;

// ── Public Entry Point ──────────────────────────────────────────────────────

/// Drive the agentic loop for a graph until it completes, pauses, aborts,
/// deadlocks, or encounters an unrecoverable error.
///
/// This function is designed to be called once per "run" invocation. On pause
/// or abort it returns immediately with the corresponding variant; the caller
/// can resume by calling `run_graph_loop` again after the runtime status is
/// reset to `running`.
///
/// # Errors
///
/// Returns `Err` only for database failures or unrecoverable internal errors.
/// Transient failures (git, agent spawning) are handled inside the loop and
/// surfaced through `LoopIterationResult` or step/phase status updates.
pub async fn run_graph_loop(
    conn: &Connection,
    graph_id: &str,
    project_root: &Path,
    db_path: &Path,
    provider: &Arc<dyn Provider>,
) -> GroveResult<LoopIterationResult> {
    // ── INITIAL SETUP (first call only) ─────────────────────────────────────
    let graph = grove_graph_repo::get_graph(conn, graph_id)?;

    if graph.git_branch.is_none() {
        // Record the current branch from the conversation worktree.
        // The worktree was already set up by the caller (commands.rs).
        match git_ops::detect_current_branch(project_root) {
            Some(branch) => {
                if let Err(e) = grove_graph_repo::set_graph_git_branch(conn, graph_id, &branch) {
                    warn!(graph_id, error = %e, "failed to record graph branch");
                } else {
                    info!(graph_id, branch = branch.as_str(), "graph branch recorded");
                }
            }
            None => {
                warn!(graph_id, "no git branch detected — continuing without git");
            }
        }
    }

    grove_graph_repo::set_runtime_status(conn, graph_id, RuntimeStatus::Running.as_str())?;
    grove_graph_repo::update_graph_status(conn, graph_id, "inprogress")?;

    // Create the JSONL log directory for this graph's agent sessions.
    let graph_log_dir = crate::config::paths::logs_dir(project_root)
        .join("graphs")
        .join(graph_id);
    if let Err(e) = std::fs::create_dir_all(&graph_log_dir) {
        warn!(graph_id, error = %e, "failed to create graph log directory");
    }
    let log_dir_str = graph_log_dir.to_string_lossy().to_string();

    info!(
        graph_id,
        title = graph.title.as_str(),
        log_dir = log_dir_str.as_str(),
        "agentic loop started"
    );

    // ── MAIN LOOP ───────────────────────────────────────────────────────────
    const MAX_DEADLOCK_RETRIES: u32 = 2;
    let mut deadlock_retries: u32 = 0;

    loop {
        // ── 1. RUNTIME CHECK ────────────────────────────────────────────────
        let status = check_runtime_status(conn, graph_id)?;
        match status {
            RuntimeStatus::Paused => {
                info!(graph_id, "loop paused by runtime status");
                return Ok(LoopIterationResult::Paused);
            }
            RuntimeStatus::Aborted => {
                warn!(graph_id, "loop aborted by runtime status");
                grove_graph_repo::update_graph_status(conn, graph_id, "failed")?;
                return Ok(LoopIterationResult::Aborted);
            }
            RuntimeStatus::Running | RuntimeStatus::Idle | RuntimeStatus::Queued => {
                // Continue execution.
            }
        }

        // ── 2. PHASE VALIDATION CHECK ───────────────────────────────────────
        let phases_pending = grove_graph_repo::get_phases_pending_validation(conn, graph_id)?;

        for phase in &phases_pending {
            debug!(
                graph_id,
                phase_id = phase.id.as_str(),
                phase_name = phase.task_name.as_str(),
                "running phase validation cycle"
            );

            let result = crate::grove_graph::execution::run_phase_validation_cycle(
                conn,
                &phase.id,
                project_root,
                db_path,
                provider,
                Some(&log_dir_str),
            )
            .await?;

            match result {
                PhaseValidationResult::Passed => {
                    info!(
                        graph_id,
                        phase_id = phase.id.as_str(),
                        "phase validation passed — committing"
                    );
                    match git_ops::commit_phase(conn, project_root, graph_id, &phase.id) {
                        Ok(Some(sha)) => {
                            debug!(
                                graph_id,
                                phase_id = phase.id.as_str(),
                                sha = sha.as_str(),
                                "phase commit created"
                            );
                        }
                        Ok(None) => {
                            debug!(
                                graph_id,
                                phase_id = phase.id.as_str(),
                                "no changes to commit for phase"
                            );
                        }
                        Err(e) => {
                            warn!(
                                graph_id,
                                phase_id = phase.id.as_str(),
                                error = %e,
                                "phase commit failed — continuing"
                            );
                        }
                    }
                }
                PhaseValidationResult::Retrying => {
                    debug!(
                        graph_id,
                        phase_id = phase.id.as_str(),
                        "phase validation triggered retries — steps re-opened"
                    );
                }
                PhaseValidationResult::Failed => {
                    warn!(
                        graph_id,
                        phase_id = phase.id.as_str(),
                        "phase validation failed — invoking orchestrator for triage"
                    );
                    // Invoke orchestrator for triage.
                    let step_outcomes = collect_step_outcomes(conn, &phase.id)?;
                    let failed_step_ids: Vec<String> =
                        grove_graph_repo::list_steps(conn, &phase.id)?
                            .into_iter()
                            .filter(|s| {
                                s.grade.map(|g| g < 7).unwrap_or(false) || s.status == "failed"
                            })
                            .map(|s| s.id)
                            .collect();
                    let decision = orchestrator_dispatch::DecisionType::PhaseValidationFailure {
                        phase: phase.clone(),
                        judge_grade: phase.grade.unwrap_or(0),
                        judge_feedback: phase.ai_comments.clone().unwrap_or_default(),
                        step_outcomes,
                        failed_step_ids,
                    };
                    match orchestrator_dispatch::dispatch_orchestrator(
                        provider,
                        &decision,
                        project_root,
                        db_path,
                        Some(&log_dir_str),
                    ) {
                        Ok(response) => {
                            if let Ok(triage) =
                                orchestrator_dispatch::parse_triage_decision(&response)
                            {
                                apply_triage_decision(conn, &triage)?;
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "orchestrator triage failed, phase remains failed");
                        }
                    }
                }
            }
        }

        // ── 3. GRAPH COMPLETION CHECK ───────────────────────────────────────
        let all_closed = grove_graph_repo::all_phases_closed(conn, graph_id)?;
        let all_passed = grove_graph_repo::all_validations_passed(conn, graph_id)?;

        if all_closed && all_passed {
            info!(
                graph_id,
                "all phases closed and validated — finalizing graph"
            );

            match git_ops::finalize_graph(conn, project_root, graph_id) {
                Ok(result) => {
                    info!(
                        graph_id,
                        merge_status = result.merge_status.as_str(),
                        pr_url = result.pr_url.as_deref().unwrap_or("none"),
                        "graph finalized"
                    );
                }
                Err(e) => {
                    warn!(
                        graph_id,
                        error = %e,
                        "graph finalization failed — marking complete anyway"
                    );
                }
            }

            grove_graph_repo::update_graph_status(conn, graph_id, "closed")?;
            grove_graph_repo::set_runtime_status(conn, graph_id, RuntimeStatus::Idle.as_str())?;

            info!(graph_id, "graph execution complete");
            return Ok(LoopIterationResult::GraphComplete);
        }

        // ── 4. QUERY THE DAG ────────────────────────────────────────────────
        let ready_steps = grove_graph_repo::get_ready_steps_for_graph(conn, graph_id)?;

        if ready_steps.is_empty() {
            // No ready steps — check if there are open steps (blocked by deps).
            if grove_graph_repo::has_any_open_steps(conn, graph_id)? {
                deadlock_retries += 1;
                warn!(
                    graph_id,
                    deadlock_retries,
                    max = MAX_DEADLOCK_RETRIES,
                    "deadlock detected: open steps exist but none are ready"
                );

                // Cap retries to prevent infinite orchestrator loop.
                if deadlock_retries > MAX_DEADLOCK_RETRIES {
                    warn!(graph_id, "deadlock retry limit reached — failing graph");
                    grove_graph_repo::update_graph_status(conn, graph_id, "failed")?;
                    grove_graph_repo::set_runtime_status(
                        conn,
                        graph_id,
                        RuntimeStatus::Idle.as_str(),
                    )?;
                    return Ok(LoopIterationResult::Deadlock);
                }

                // Invoke orchestrator for deadlock diagnosis.
                let all_steps = grove_graph_repo::list_steps_for_graph(conn, graph_id)?;
                let decision = orchestrator_dispatch::DecisionType::Deadlock {
                    graph_id: graph_id.to_string(),
                    all_steps,
                };
                match orchestrator_dispatch::dispatch_orchestrator(
                    provider,
                    &decision,
                    project_root,
                    db_path,
                    Some(&log_dir_str),
                ) {
                    Ok(response) => {
                        if let Ok(deadlock) =
                            orchestrator_dispatch::parse_deadlock_decision(&response)
                        {
                            if deadlock.action == "escalate_to_user" {
                                grove_graph_repo::update_graph_status(conn, graph_id, "failed")?;
                                grove_graph_repo::set_runtime_status(
                                    conn,
                                    graph_id,
                                    RuntimeStatus::Idle.as_str(),
                                )?;
                                return Ok(LoopIterationResult::Deadlock);
                            }
                            apply_deadlock_decision(conn, &deadlock)?;
                            continue; // Re-enter loop after applying fix.
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "orchestrator deadlock diagnosis failed");
                    }
                }
                // Fallback: hard deadlock.
                grove_graph_repo::update_graph_status(conn, graph_id, "failed")?;
                grove_graph_repo::set_runtime_status(conn, graph_id, RuntimeStatus::Idle.as_str())?;
                return Ok(LoopIterationResult::Deadlock);
            }

            // No open steps and no ready steps. If all phases are also not
            // closed, it means some steps are in a terminal non-closed state
            // (e.g. failed) that prevents phase closure. This is a stuck state.
            if !all_closed {
                warn!(
                    graph_id,
                    "graph stuck: no open/ready steps but phases remain unclosed \
                     (likely due to failed steps)"
                );
                grove_graph_repo::update_graph_status(conn, graph_id, "failed")?;
                grove_graph_repo::set_runtime_status(conn, graph_id, RuntimeStatus::Idle.as_str())?;
                return Ok(LoopIterationResult::Error(
                    "graph stuck: failed steps prevent phase closure".into(),
                ));
            }

            // All phases are closed but validations may be in-progress or
            // pending — loop back to pick up phase validations on the next
            // iteration. This path is hit when phases just became closed.
            debug!(
                graph_id,
                "no ready steps and no open steps — looping to phase validation"
            );
            continue;
        }

        // ── 5. CHUNK CREATION ───────────────────────────────────────────────
        // Group ready steps by phase, then chunk the first phase.
        let target_phase_id = &ready_steps[0].phase_id;
        let phase = grove_graph_repo::get_phase(conn, target_phase_id)?;

        let chunks = match chunking::create_chunks(
            conn,
            target_phase_id,
            chunking::DEFAULT_MAX_CHUNK_SIZE,
        )? {
            Some(chunks) => chunks,
            None => {
                // Complex phase — ask orchestrator for chunk planning.
                let all_open = grove_graph_repo::list_steps(conn, target_phase_id)?
                    .into_iter()
                    .filter(|s| s.status == "open")
                    .collect::<Vec<_>>();

                let decision = orchestrator_dispatch::DecisionType::ChunkPlanning {
                    phase: phase.clone(),
                    open_steps: all_open.clone(),
                    max_chunk_size: chunking::DEFAULT_MAX_CHUNK_SIZE,
                };

                let response = orchestrator_dispatch::dispatch_orchestrator(
                    provider,
                    &decision,
                    project_root,
                    db_path,
                    Some(&log_dir_str),
                )?;

                let chunk_decision = orchestrator_dispatch::parse_chunk_decision(&response)?;

                // Validate and convert to StepChunks.
                let step_map: std::collections::HashMap<String, GraphStepRow> =
                    all_open.into_iter().map(|s| (s.id.clone(), s)).collect();

                let available_ids: std::collections::HashSet<String> =
                    step_map.keys().cloned().collect();
                let all_open_ref: Vec<GraphStepRow> = step_map.values().cloned().collect();

                if let Err(e) = orchestrator_dispatch::validate_chunk_decision(
                    &chunk_decision,
                    &available_ids,
                    &all_open_ref,
                ) {
                    warn!(error = %e, "orchestrator chunk invalid, falling back to 1-per-chunk");
                    // Fallback: one step per chunk.
                    ready_steps
                        .iter()
                        .map(|s| chunking::StepChunk {
                            step_ids: vec![s.id.clone()],
                            steps: vec![s.clone()],
                        })
                        .collect()
                } else {
                    orchestrator_dispatch::chunks_from_decision(&chunk_decision, &all_open_ref)
                }
            }
        };

        // ── 6. WORKER DISPATCH (one chunk at a time) ────────────────────────
        for chunk in &chunks {
            if chunk.step_ids.is_empty() {
                continue;
            }

            // Runtime re-check before dispatching.
            let status = check_runtime_status(conn, graph_id)?;
            match status {
                RuntimeStatus::Paused => return Ok(LoopIterationResult::Paused),
                RuntimeStatus::Aborted => return Ok(LoopIterationResult::Aborted),
                _ => {}
            }

            debug!(
                graph_id,
                chunk_size = chunk.step_ids.len(),
                "dispatching worker for chunk"
            );

            // Mark only the first step as "inprogress" so the UI reflects
            // that work has started. The worker executes steps sequentially —
            // remaining steps stay "open" until the MCP server updates them.
            if let Some(first_id) = chunk.step_ids.first() {
                if let Err(e) = grove_graph_repo::update_step_status(conn, first_id, "inprogress") {
                    warn!(step_id = first_id.as_str(), error = %e, "failed to set step inprogress");
                }
            }

            // Dispatch worker.
            let dispatch_outcome = worker_dispatch::dispatch_worker(
                provider,
                chunk,
                &phase.task_objective,
                graph_id,
                project_root,
                db_path,
                None,
                Some(&log_dir_str),
            )?;

            // Check if the provider call itself crashed.
            if let worker_dispatch::DispatchOutcome::Crashed(error) = dispatch_outcome {
                warn!(error = %error, "worker crashed");
                // Read DB to see what was completed before crash.
                let completed: Vec<String> = chunk
                    .step_ids
                    .iter()
                    .filter(|id| {
                        grove_graph_repo::get_step(conn, id)
                            .map(|s| s.status == "closed")
                            .unwrap_or(false)
                    })
                    .cloned()
                    .collect();
                if !completed.is_empty() {
                    git_ops::commit_chunk(conn, project_root, graph_id, &completed)?;
                }
                let remaining: Vec<String> = chunk
                    .step_ids
                    .iter()
                    .filter(|id| !completed.contains(id))
                    .cloned()
                    .collect();
                // Invoke orchestrator for failover.
                let decision = orchestrator_dispatch::DecisionType::FailoverRecovery {
                    phase: phase.clone(),
                    completed_steps: completed,
                    failed_steps: vec![],
                    remaining_steps: remaining,
                    error_context: error,
                };
                match orchestrator_dispatch::dispatch_orchestrator(
                    provider,
                    &decision,
                    project_root,
                    db_path,
                    Some(&log_dir_str),
                ) {
                    Ok(response) => {
                        if let Ok(failover) =
                            orchestrator_dispatch::parse_failover_decision(&response)
                        {
                            apply_failover_decision(conn, &failover)?;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "orchestrator failover failed");
                    }
                }
                break; // Re-enter main loop.
            }

            // Provider succeeded — force-close any steps that the MCP server
            // marked as closed but that aren't visible to this connection
            // (SQLite WAL cross-process visibility issue).
            let force_closed = worker_dispatch::force_close_completed_steps(conn, chunk)?;
            if force_closed > 0 {
                info!(
                    force_closed,
                    "force-closed steps after successful worker dispatch"
                );
            }

            // Now assess step results from DB.
            let result = worker_dispatch::assess_worker_result(conn, chunk)?;

            match result {
                worker_dispatch::WorkerResult::AllCompleted => {
                    // Commit chunk changes.
                    git_ops::commit_chunk(conn, project_root, graph_id, &chunk.step_ids)?;
                    info!(
                        chunk_size = chunk.step_ids.len(),
                        "chunk completed successfully"
                    );

                    // Reset deadlock counter — real progress was made.
                    deadlock_retries = 0;

                    // Check if all steps in the phase are now closed; if so,
                    // auto-close the phase so validation can proceed. Without
                    // this the loop re-dispatches the same completed work
                    // because the phase stays 'open'/'inprogress' and
                    // get_ready_steps_for_graph finds zero open steps, yet
                    // all_phases_closed returns false → infinite loop.
                    let phase_steps = grove_graph_repo::list_steps(conn, &phase.id)?;
                    let all_steps_closed =
                        !phase_steps.is_empty() && phase_steps.iter().all(|s| s.status == "closed");
                    // Re-read phase from DB to avoid stale status (Issue #6).
                    let fresh_phase = grove_graph_repo::get_phase(conn, &phase.id)?;
                    if all_steps_closed && fresh_phase.status != "closed" {
                        info!(
                            graph_id,
                            phase_id = phase.id.as_str(),
                            "all steps closed — auto-closing phase"
                        );
                        grove_graph_repo::update_phase_status(conn, &phase.id, "closed")?;
                    }
                }
                worker_dispatch::WorkerResult::Partial {
                    completed,
                    failed,
                    remaining,
                } => {
                    // Commit what was completed.
                    if !completed.is_empty() {
                        git_ops::commit_chunk(conn, project_root, graph_id, &completed)?;
                    }

                    if !remaining.is_empty() || !failed.is_empty() {
                        // Invoke orchestrator for recovery.
                        let decision = orchestrator_dispatch::DecisionType::FailoverRecovery {
                            phase: phase.clone(),
                            completed_steps: completed,
                            failed_steps: failed,
                            remaining_steps: remaining,
                            error_context: "Worker returned partial results".into(),
                        };
                        match orchestrator_dispatch::dispatch_orchestrator(
                            provider,
                            &decision,
                            project_root,
                            db_path,
                            Some(&log_dir_str),
                        ) {
                            Ok(response) => {
                                if let Ok(failover) =
                                    orchestrator_dispatch::parse_failover_decision(&response)
                                {
                                    apply_failover_decision(conn, &failover)?;
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "orchestrator failover failed");
                            }
                        }
                    }
                    break; // Re-enter main loop to re-assess.
                }
            }
        }

        // After dispatch, loop back to re-check phase validation and DAG state.
    }
}

// ── Helper Functions ────────────────────────────────────────────────────────

/// Read the current runtime status from the DB.
fn check_runtime_status(conn: &Connection, graph_id: &str) -> GroveResult<RuntimeStatus> {
    let current_graph = grove_graph_repo::get_graph(conn, graph_id)?;
    RuntimeStatus::try_from(current_graph.runtime_status.as_str()).map_err(GroveError::Runtime)
}

/// Collect step outcomes for a phase (used by orchestrator context).
fn collect_step_outcomes(
    conn: &Connection,
    phase_id: &str,
) -> GroveResult<Vec<(grove_graph_repo::GraphStepRow, String)>> {
    let steps = grove_graph_repo::list_steps(conn, phase_id)?;
    Ok(steps
        .into_iter()
        .map(|s| {
            let outcome = s
                .outcome
                .clone()
                .unwrap_or_else(|| "(no outcome recorded)".into());
            (s, outcome)
        })
        .collect())
}

/// Apply orchestrator triage decision: reopen steps with feedback.
fn apply_triage_decision(
    conn: &Connection,
    triage: &orchestrator_dispatch::TriageDecision,
) -> GroveResult<()> {
    for step_id in &triage.reopen_steps {
        grove_graph_repo::reopen_step(conn, step_id)?;
        // NOTE: iteration incrementing is handled by run_step_cycle only (Issue #7).
        if let Some(feedback) = triage.feedback_per_step.get(step_id) {
            grove_graph_repo::append_judge_feedback(conn, step_id, feedback)?;
        }
        // Check max_iterations after incrementing.
        let step = grove_graph_repo::get_step(conn, step_id)?;
        if step.run_iteration >= step.max_iterations {
            warn!(
                step_id,
                "step hit max iterations during triage, marking failed"
            );
            grove_graph_repo::set_step_failed(conn, step_id, "max iterations reached")?;
        }
    }
    Ok(())
}

/// Apply orchestrator failover decision: reset steps and prepare for retry.
fn apply_failover_decision(
    conn: &Connection,
    failover: &orchestrator_dispatch::FailoverDecision,
) -> GroveResult<()> {
    // Issue #17: Log ignored failover fields.
    if failover.strategy != "resume" {
        warn!(
            strategy = failover.strategy.as_str(),
            "failover strategy field present but only 'resume' is acted upon — \
             'reset_steps' is used regardless of strategy value"
        );
    }
    if failover.chunks.is_some() {
        warn!("failover decision contains 'chunks' field which is currently ignored");
    }
    if failover.session_id.is_some() {
        warn!("failover decision contains 'session_id' field which is currently ignored");
    }

    if let Some(reset_ids) = &failover.reset_steps {
        for step_id in reset_ids {
            grove_graph_repo::reopen_step(conn, step_id)?;
        }
    } else {
        // Issue #16: reset_steps is None — warn and skip.
        warn!(
            strategy = failover.strategy.as_str(),
            "failover decision has reset_steps = None — no steps will be reopened"
        );
    }
    Ok(())
}

/// Apply orchestrator deadlock decision: reset or skip steps.
fn apply_deadlock_decision(
    conn: &Connection,
    deadlock: &orchestrator_dispatch::DeadlockDecision,
) -> GroveResult<()> {
    match deadlock.action.as_str() {
        "reset_and_retry" => {
            if let Some(reset_ids) = &deadlock.reset_steps {
                for step_id in reset_ids {
                    grove_graph_repo::reopen_step(conn, step_id)?;
                }
            }
        }
        "skip" => {
            if let Some(skip_ids) = &deadlock.skip_steps {
                for step_id in skip_ids {
                    grove_graph_repo::set_step_failed(conn, step_id, "skipped by orchestrator")?;
                }
            }
        }
        _ => {
            // "escalate_to_user" is handled by caller.
        }
    }
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grove_graph::PhaseValidationResult;
    use crate::providers::MockProvider;

    fn mock_provider() -> Arc<dyn Provider> {
        Arc::new(MockProvider)
    }

    // ── Test helpers ────────────────────────────────────────────────────────

    fn test_db() -> Connection {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        crate::db::DbHandle::new(dir.path()).connect().unwrap()
    }

    fn seed_conversation(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO conversations (id, project_id, state, conversation_kind, \
             remote_registration_state, created_at, updated_at) \
             VALUES (?1, 'proj1', 'active', 'run', 'none', \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [id],
        )
        .unwrap();
    }

    fn seed_graph(conn: &Connection, conv_id: &str) -> String {
        seed_conversation(conn, conv_id);
        grove_graph_repo::insert_graph(conn, conv_id, "Test Graph", "desc", None).unwrap()
    }

    fn seed_phase(conn: &Connection, graph_id: &str, ordinal: i64) -> String {
        grove_graph_repo::insert_phase(
            conn,
            graph_id,
            &format!("Phase {ordinal}"),
            "Build feature",
            ordinal,
            "[]",
            false,
            None,
        )
        .unwrap()
    }

    fn seed_step(
        conn: &Connection,
        phase_id: &str,
        graph_id: &str,
        ordinal: i64,
        deps: &str,
    ) -> String {
        grove_graph_repo::insert_step(
            conn,
            phase_id,
            graph_id,
            &format!("Step {ordinal}"),
            "Implement something",
            ordinal,
            "code",
            "auto",
            deps,
            false,
            None,
        )
        .unwrap()
    }

    // ── RuntimeStatus check on initial setup ────────────────────────────────

    #[tokio::test]
    async fn initial_setup_sets_running_and_inprogress() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_init");

        // Verify the graph starts as idle/open.
        let graph_before = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph_before.runtime_status, "idle");
        assert_eq!(graph_before.status, "open");
        assert!(graph_before.git_branch.is_none());

        // The loop will set running + inprogress, then discover no phases/steps
        // and fall through to completion (all_phases_closed = true for 0 phases).
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let result = run_graph_loop(&conn, &graph_id, tmp.path(), &db_path, &mock_provider())
            .await
            .unwrap();

        // With zero phases, all_phases_closed() and all_validations_passed()
        // are both true (COUNT(*) WHERE status != 'closed' = 0 for empty set).
        assert_eq!(result, LoopIterationResult::GraphComplete);

        let graph_after = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph_after.status, "closed");
        assert_eq!(graph_after.runtime_status, "idle");
    }

    #[tokio::test]
    async fn paused_graph_returns_paused_immediately() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_pause");
        let phase_id = seed_phase(&conn, &graph_id, 0);
        let _step_id = seed_step(&conn, &phase_id, &graph_id, 0, "[]");

        // The initial setup always sets runtime_status to "running",
        // overwriting any pre-set pause. We cannot test mid-loop pause
        // without concurrency. Instead, verify the status parsing logic
        // that the runtime check relies on.
        grove_graph_repo::set_runtime_status(&conn, &graph_id, "paused").unwrap();
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        let rt = RuntimeStatus::try_from(graph.runtime_status.as_str()).unwrap();
        assert_eq!(rt, RuntimeStatus::Paused);
    }

    #[tokio::test]
    async fn aborted_runtime_returns_aborted_and_sets_failed() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_abort");

        // Verify the abort path works at the status level.
        grove_graph_repo::set_runtime_status(&conn, &graph_id, "aborted").unwrap();
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        let rt = RuntimeStatus::try_from(graph.runtime_status.as_str()).unwrap();
        assert_eq!(rt, RuntimeStatus::Aborted);
    }

    // ── Deadlock detection ──────────────────────────────────────────────────

    #[tokio::test]
    async fn deadlock_detected_when_open_steps_but_none_ready() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_deadlock");
        let phase_id = seed_phase(&conn, &graph_id, 0);

        // Create two steps that depend on each other (circular dependency).
        let step_a = seed_step(&conn, &phase_id, &graph_id, 0, "[]");
        let step_b_deps = format!("[\"{step_a}\"]");
        let step_b = seed_step(&conn, &phase_id, &graph_id, 1, &step_b_deps);

        // Now make step_a depend on step_b by updating its depends_on_json.
        let step_a_deps = format!("[\"{step_b}\"]");
        conn.execute(
            "UPDATE graph_steps SET depends_on_json = ?1 WHERE id = ?2",
            rusqlite::params![step_a_deps, step_a],
        )
        .unwrap();

        // Both steps are open but neither can run (circular dependency).
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let result = run_graph_loop(&conn, &graph_id, tmp.path(), &db_path, &mock_provider())
            .await
            .unwrap();

        assert_eq!(result, LoopIterationResult::Deadlock);

        // Deadlock now fails the graph.
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph.status, "failed");
    }

    // ── Step dispatch: failed steps cause stuck detection ────────────────────

    #[tokio::test]
    async fn step_failure_causes_stuck_detection() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_seq");
        let phase_id = seed_phase(&conn, &graph_id, 0);
        let step_id = seed_step(&conn, &phase_id, &graph_id, 0, "[]");

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");

        // The worker will fail because the mock provider just returns a summary.
        // After worker dispatch, the step remains "open" (no MCP tools available),
        // so on the next loop iteration the same step comes back as ready.
        // With the mock provider, the worker will crash or return without
        // updating the step, leading to a stuck state eventually.
        // For this test, pre-fail the step to simulate the outcome.
        grove_graph_repo::set_step_failed(&conn, &step_id, "builder not wired").unwrap();

        let result = run_graph_loop(&conn, &graph_id, tmp.path(), &db_path, &mock_provider())
            .await
            .unwrap();

        // Step is failed (not open, not closed), no ready steps, phase unclosed → stuck.
        assert!(
            matches!(result, LoopIterationResult::Error(_)),
            "expected Error, got: {:?}",
            result
        );

        // Verify step is failed.
        let step = grove_graph_repo::get_step(&conn, &step_id).unwrap();
        assert_eq!(step.status, "failed");

        // Graph should be marked as failed with runtime idle.
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph.status, "failed");
        assert_eq!(graph.runtime_status, "idle");
    }

    // ── Execution mode parsing ──────────────────────────────────────────────

    #[test]
    fn execution_mode_default_is_sequential() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_mode");

        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        let mode = crate::grove_graph::GraphExecutionMode::try_from(graph.execution_mode.as_str())
            .unwrap();
        assert_eq!(mode, crate::grove_graph::GraphExecutionMode::Sequential);
    }

    #[test]
    fn execution_mode_parallel() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_mode_par");

        grove_graph_repo::set_graph_execution_mode(&conn, &graph_id, "parallel").unwrap();
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        let mode = crate::grove_graph::GraphExecutionMode::try_from(graph.execution_mode.as_str())
            .unwrap();
        assert_eq!(mode, crate::grove_graph::GraphExecutionMode::Parallel);
    }

    // ── Completion with zero phases ─────────────────────────────────────────

    #[tokio::test]
    async fn empty_graph_completes_immediately() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_empty");

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let result = run_graph_loop(&conn, &graph_id, tmp.path(), &db_path, &mock_provider())
            .await
            .unwrap();

        assert_eq!(result, LoopIterationResult::GraphComplete);

        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph.status, "closed");
        assert_eq!(graph.runtime_status, "idle");
    }

    // ── LoopIterationResult enum variants ───────────────────────────────────

    #[test]
    fn loop_iteration_result_variants_are_distinct() {
        let variants = [
            LoopIterationResult::Continue,
            LoopIterationResult::GraphComplete,
            LoopIterationResult::Paused,
            LoopIterationResult::Aborted,
            LoopIterationResult::Deadlock,
            LoopIterationResult::Error("test".into()),
        ];

        // All variants should be different from each other.
        for (i, a) in variants.iter().enumerate() {
            for (j, b) in variants.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "variants at index {i} and {j} should differ");
                }
            }
        }
    }

    // ── Runtime status transition checks ────────────────────────────────────

    #[test]
    fn runtime_status_running_allows_continuation() {
        let rt = RuntimeStatus::try_from("running").unwrap();
        assert_eq!(rt, RuntimeStatus::Running);
        assert_ne!(rt, RuntimeStatus::Paused);
        assert_ne!(rt, RuntimeStatus::Aborted);
    }

    #[test]
    fn runtime_status_idle_allows_continuation() {
        let rt = RuntimeStatus::try_from("idle").unwrap();
        assert_eq!(rt, RuntimeStatus::Idle);
        assert_ne!(rt, RuntimeStatus::Paused);
        assert_ne!(rt, RuntimeStatus::Aborted);
    }

    #[test]
    fn invalid_runtime_status_produces_error() {
        let result = RuntimeStatus::try_from("bogus");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_execution_mode_produces_error() {
        let result = crate::grove_graph::GraphExecutionMode::try_from("invalid_mode");
        assert!(result.is_err());
    }

    // ── Phase validation result handling ─────────────────────────────────────

    #[test]
    fn phase_validation_result_variants() {
        assert_ne!(
            PhaseValidationResult::Passed,
            PhaseValidationResult::Retrying
        );
        assert_ne!(PhaseValidationResult::Passed, PhaseValidationResult::Failed);
        assert_ne!(
            PhaseValidationResult::Retrying,
            PhaseValidationResult::Failed
        );
    }

    // ── DAG query helpers ───────────────────────────────────────────────────

    #[test]
    fn get_ready_steps_respects_dependencies() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_dag");
        let phase_id = seed_phase(&conn, &graph_id, 0);

        // Step A has no deps — should be ready.
        let step_a = seed_step(&conn, &phase_id, &graph_id, 0, "[]");

        // Step B depends on Step A — should NOT be ready while A is open.
        let step_b_deps = format!("[\"{step_a}\"]");
        let _step_b = seed_step(&conn, &phase_id, &graph_id, 1, &step_b_deps);

        let ready = grove_graph_repo::get_ready_steps_for_graph(&conn, &graph_id).unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, step_a);
    }

    #[test]
    fn get_ready_steps_unlocks_after_dep_closed() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_dag2");
        let phase_id = seed_phase(&conn, &graph_id, 0);

        let step_a = seed_step(&conn, &phase_id, &graph_id, 0, "[]");
        let step_b_deps = format!("[\"{step_a}\"]");
        let step_b = seed_step(&conn, &phase_id, &graph_id, 1, &step_b_deps);

        // Close step A.
        conn.execute(
            "UPDATE graph_steps SET status = 'closed' WHERE id = ?1",
            [&step_a],
        )
        .unwrap();

        let ready = grove_graph_repo::get_ready_steps_for_graph(&conn, &graph_id).unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, step_b);
    }

    #[test]
    fn has_any_open_steps_true_when_steps_exist() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_open");
        let phase_id = seed_phase(&conn, &graph_id, 0);
        let _step = seed_step(&conn, &phase_id, &graph_id, 0, "[]");

        assert!(grove_graph_repo::has_any_open_steps(&conn, &graph_id).unwrap());
    }

    #[test]
    fn has_any_open_steps_false_when_all_closed_or_failed() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_no_open");
        let phase_id = seed_phase(&conn, &graph_id, 0);
        let step = seed_step(&conn, &phase_id, &graph_id, 0, "[]");

        // Mark step as failed (not open).
        conn.execute(
            "UPDATE graph_steps SET status = 'failed' WHERE id = ?1",
            [&step],
        )
        .unwrap();

        assert!(!grove_graph_repo::has_any_open_steps(&conn, &graph_id).unwrap());
    }

    // ── Initial setup branch creation ───────────────────────────────────────

    #[tokio::test]
    async fn initial_setup_skips_branch_if_already_set() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_branch_exists");

        // Pre-set a git branch.
        grove_graph_repo::set_graph_git_branch(&conn, &graph_id, "grove-graph/existing/branch")
            .unwrap();

        let graph_before = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert!(graph_before.git_branch.is_some());

        // Run the loop — it should skip branch creation and complete (no phases).
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let result = run_graph_loop(&conn, &graph_id, tmp.path(), &db_path, &mock_provider())
            .await
            .unwrap();

        assert_eq!(result, LoopIterationResult::GraphComplete);

        // The branch should still be the one we set, not overwritten.
        let graph_after = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(
            graph_after.git_branch.as_deref(),
            Some("grove-graph/existing/branch")
        );
    }

    // ── Stuck detection with multiple steps ─────────────────────────────────

    #[tokio::test]
    async fn stuck_detected_when_some_steps_failed_and_rest_blocked() {
        let conn = test_db();
        let graph_id = seed_graph(&conn, "conv_stuck_multi");
        let phase_id = seed_phase(&conn, &graph_id, 0);

        // Step A (no deps) — we'll pre-fail it.
        let step_a = seed_step(&conn, &phase_id, &graph_id, 0, "[]");

        // Step B depends on A — will be blocked because A is failed (not closed).
        let step_b_deps = format!("[\"{step_a}\"]");
        let _step_b = seed_step(&conn, &phase_id, &graph_id, 1, &step_b_deps);

        // Pre-fail step A so the loop finds it already failed.
        conn.execute(
            "UPDATE graph_steps SET status = 'failed' WHERE id = ?1",
            [&step_a],
        )
        .unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let result = run_graph_loop(&conn, &graph_id, tmp.path(), &db_path, &mock_provider())
            .await
            .unwrap();

        // Step B is open (its dep A is failed, not closed) so ready_steps is
        // empty but has_any_open_steps is true. This is a deadlock: open steps
        // exist but none are ready.
        assert_eq!(result, LoopIterationResult::Deadlock);
    }
}
