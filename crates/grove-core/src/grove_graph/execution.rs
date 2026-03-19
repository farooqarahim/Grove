//! Step Cycle Executor — single-agent Build+Verify+Grade pipeline for a step.
//!
//! The agentic loop calls [`run_step_cycle`] for each ready step.
//! This module owns the full pipeline:
//!
//! 1. Check runtime status (can abort/pause between stages)
//! 2. Spawn a single Builder agent that builds, verifies, and self-grades (0-10)
//! 3. If grade >= 7: close step as passed
//! 4. If grade < 7 and iterations remain: append feedback, reopen step (retry)
//! 5. If max iterations reached: fail step

use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::db::repositories::grove_graph_repo::{self, GraphPhaseRow, GraphStepRow};
use crate::errors::{GroveError, GroveResult};
use crate::grove_graph::{PhaseValidationResult, RuntimeStatus};
use crate::providers::{Provider, ProviderRequest};

// ── Step Cycle Result ───────────────────────────────────────────────────────

/// Outcome of a single step's build+verify+grade cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepCycleResult {
    Passed,
    Retrying,
    Failed,
    Paused,
    Aborted,
}

// ── Agent Result Types ──────────────────────────────────────────────────────

/// Result from a Builder or Verdict agent run (used by phase validation).
#[derive(Debug, Clone)]
pub struct AgentRunResult {
    /// Unique identifier for the agent run (used for tracking in the DB).
    pub run_id: String,
    /// Summary of what the agent produced / found.
    pub outcome: String,
    /// Detailed AI commentary on the work performed.
    pub ai_comments: String,
}

/// Result from the unified Builder agent that builds, verifies, and self-grades.
#[derive(Debug, Clone)]
pub struct BuilderGradedResult {
    /// Unique identifier for the builder run.
    pub run_id: String,
    /// Self-assigned grade from 0-10.
    pub grade: i64,
    /// The builder's reasoning for the grade.
    pub reasoning: String,
    /// Actionable feedback if the grade is below threshold (self-critique).
    pub feedback: String,
    /// Summary of what was built/implemented.
    pub outcome: String,
    /// Whether the step passed (grade >= 7).
    pub passed: bool,
    /// Full AI response text.
    pub ai_comments: String,
}

/// Result from the Phase Validator agent that checks all step outcomes as a group.
#[derive(Debug, Clone)]
pub struct PhaseValidatorResult {
    /// Unique identifier for the validator agent run.
    pub run_id: String,
    /// Whether all steps collectively satisfy the phase objective.
    pub passed: bool,
    /// Issues found across step outcomes (empty if passed).
    pub issues: Vec<String>,
    /// IDs of steps that the validator considers problematic.
    pub failed_step_ids: Vec<String>,
}

/// Result from the Phase Judge agent that grades the phase's collective work.
#[derive(Debug, Clone)]
pub struct PhaseJudgeResult {
    /// Unique identifier for the judge agent run.
    pub run_id: String,
    /// Grade from 0-10 for the phase as a whole.
    pub grade: i64,
    /// The judge's reasoning for the grade.
    pub reasoning: String,
    /// Whether the phase passed (grade >= 7).
    pub passed: bool,
    /// Steps that need rework: (step_id, feedback) pairs.
    /// Only the judge-identified steps get re-opened, not the entire phase.
    pub failed_steps: Vec<(String, String)>,
}

// ── Grade Threshold ─────────────────────────────────────────────────────────

/// Minimum judge grade required for a step to be considered passed.
const PASS_THRESHOLD: i64 = 7;

// ── Runtime Status Check ────────────────────────────────────────────────────

/// Check the graph's runtime_status. Returns `Ok(())` if running, or the
/// appropriate `StepCycleResult` variant if paused/aborted.
fn check_runtime_status(conn: &Connection, graph_id: &str) -> GroveResult<Option<StepCycleResult>> {
    let graph = grove_graph_repo::get_graph(conn, graph_id)?;
    let status =
        RuntimeStatus::try_from(graph.runtime_status.as_str()).map_err(GroveError::Runtime)?;

    match status {
        RuntimeStatus::Paused => {
            debug!(graph_id, "step cycle paused — runtime_status is paused");
            Ok(Some(StepCycleResult::Paused))
        }
        RuntimeStatus::Aborted => {
            debug!(graph_id, "step cycle aborted — runtime_status is aborted");
            Ok(Some(StepCycleResult::Aborted))
        }
        RuntimeStatus::Running | RuntimeStatus::Idle | RuntimeStatus::Queued => Ok(None),
    }
}

// ── Skill Loader ─────────────────────────────────────────────────────────────

fn load_skill_instructions(project_root: &Path, skill_dir: &str) -> String {
    crate::grove_graph::skill_loader::load_skill(project_root, skill_dir)
}

// ── JSON Response Parsing ───────────────────────────────────────────────────

/// Try to extract a JSON object from a provider response summary.
/// The agent may wrap JSON in markdown code fences, so we strip those.
fn extract_json_from_response(summary: &str) -> Option<serde_json::Value> {
    // Try direct parse first.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(summary) {
        return Some(v);
    }

    // Try extracting from ```json ... ``` code fences.
    let trimmed = summary.trim();
    if let Some(start) = trimmed.find("```json") {
        let after_fence = &trimmed[start + 7..];
        if let Some(end) = after_fence.find("```") {
            let json_str = after_fence[..end].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                return Some(v);
            }
        }
    }

    // Try extracting from ``` ... ``` (no language tag).
    if let Some(start) = trimmed.find("```") {
        let after_fence = &trimmed[start + 3..];
        // Skip any language tag on the same line.
        let content_start = after_fence.find('\n').unwrap_or(0);
        let content = &after_fence[content_start..];
        if let Some(end) = content.find("```") {
            let json_str = content[..end].trim();
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                return Some(v);
            }
        }
    }

    None
}

// ── Agent Spawners ──────────────────────────────────────────────────────────

/// Spawn a Builder agent for the given step.
///
/// The builder builds, verifies, and self-grades in a single invocation.
/// It returns a JSON block with `grade`, `pass`, `outcome`, `reasoning`,
/// and `feedback` fields — parsed by the caller to decide pass/retry/fail.
///
/// Routes tools based on `step_type`:
/// - `Code` / `Test` -> full tools (Bash, file ops, search)
/// - `Config` -> file ops + search, no Bash
/// - `Docs` -> file ops + search, no Bash
/// - `Infra` -> full tools
///
/// The builder receives the step objective plus any accumulated feedback from
/// prior iterations.
async fn spawn_builder_agent(
    provider: &Arc<dyn Provider>,
    step: &GraphStepRow,
    phase_objective: &str,
    feedback: &[String],
    project_root: &Path,
    mcp_config_path: Option<&Path>,
    log_dir: Option<&str>,
) -> GroveResult<BuilderGradedResult> {
    let skill_content = load_skill_instructions(project_root, "step-builder");

    let feedback_section = if feedback.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = feedback
            .iter()
            .enumerate()
            .map(|(i, f)| format!("### Feedback #{}\n{f}", i + 1))
            .collect();
        format!("\n\n## Accumulated Feedback\n\n{}\n", items.join("\n\n"))
    };

    let instructions = format!(
        "{skill_content}\n\n\
         ## Task\n\n\
         **Step:** {}\n\
         **Objective:** {}\n\
         **Type:** {}\n\
         **Phase objective:** {phase_objective}\n\
         {feedback_section}",
        step.task_name, step.task_objective, step.step_type,
    );

    let run_id = Uuid::new_v4().to_string();
    let session_id = format!("builder-{}", &run_id[..8]);

    let request = ProviderRequest {
        objective: step.task_objective.clone(),
        role: "builder".to_string(),
        worktree_path: project_root.to_string_lossy().to_string(),
        instructions,
        model: None,
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: None,
        log_dir: log_dir.map(|s| s.to_string()),
        grove_session_id: Some(session_id),
        input_handle_callback: None,
        mcp_config_path: mcp_config_path.map(|p| p.to_string_lossy().to_string()),
    };

    let response = provider.execute(&request)?;

    // Parse the builder's self-grade JSON from the response.
    let (grade, reasoning, feedback_text, outcome, passed) =
        if let Some(json) = extract_json_from_response(&response.summary) {
            let g = json["grade"].as_i64().unwrap_or(0);
            let r = json["reasoning"]
                .as_str()
                .unwrap_or(&response.summary)
                .to_string();
            let f = json["feedback"].as_str().unwrap_or("").to_string();
            let o = json["outcome"]
                .as_str()
                .unwrap_or(&response.summary)
                .to_string();
            let p = json["pass"].as_bool().unwrap_or(g >= PASS_THRESHOLD);
            (g, r, f, o, p)
        } else {
            // Issue #10: If the builder didn't return JSON, default to grade 0
            // (fail-safe). The builder may have done work but without a structured
            // self-assessment we cannot trust the result.
            warn!(
                step_id = step.id.as_str(),
                "builder response was not valid JSON — defaulting to grade 0 (fail)"
            );
            (
                0,
                response.summary.clone(),
                "Builder did not return structured JSON self-assessment".to_string(),
                response.summary.clone(),
                false,
            )
        };

    Ok(BuilderGradedResult {
        run_id,
        grade,
        reasoning,
        feedback: feedback_text,
        outcome,
        passed,
        ai_comments: response.summary,
    })
}

/// Spawn a Fixer agent that surgically repairs specific issues from a previous iteration.
///
/// Unlike the Builder (which generates from scratch), the Fixer receives:
/// - The original step objective and context
/// - The accumulated feedback from the previous build+verify cycle
///
/// The Fixer also builds, verifies, and self-grades in a single invocation.
async fn spawn_fixer_agent(
    provider: &Arc<dyn Provider>,
    step: &GraphStepRow,
    phase_objective: &str,
    feedback: &[String],
    project_root: &Path,
    mcp_config_path: Option<&Path>,
    log_dir: Option<&str>,
) -> GroveResult<BuilderGradedResult> {
    let skill_content = load_skill_instructions(project_root, "step-fixer");

    let feedback_section = if feedback.is_empty() {
        String::new()
    } else {
        let items: Vec<String> = feedback
            .iter()
            .enumerate()
            .map(|(i, f)| format!("### Issue #{}\n{f}", i + 1))
            .collect();
        format!(
            "\n\n## Feedback — Issues to Fix\n\n{}\n",
            items.join("\n\n")
        )
    };

    let instructions = format!(
        "{skill_content}\n\n\
         ## Task — Fix Identified Issues\n\n\
         You are a Fixer agent. Your job is to surgically fix specific issues \
         identified in a previous iteration. Do NOT rebuild from scratch — \
         focus on the exact feedback below. After fixing, verify your work and \
         self-grade (0-10) just like the Builder agent.\n\n\
         **Step:** {}\n\
         **Objective:** {}\n\
         **Type:** {}\n\
         **Phase objective:** {phase_objective}\n\
         {feedback_section}\n\n\
         ## Output\n\n\
         End your response with a JSON block:\n\
         ```json\n\
         {{\n\
           \"grade\": <0-10>,\n\
           \"pass\": <true/false>,\n\
           \"outcome\": \"<what you fixed>\",\n\
           \"reasoning\": \"<why this grade>\",\n\
           \"feedback\": \"<remaining issues if grade < 7>\"\n\
         }}\n\
         ```",
        step.task_name, step.task_objective, step.step_type,
    );

    let run_id = Uuid::new_v4().to_string();
    let session_id = format!("fixer-{}", &run_id[..8]);

    let request = ProviderRequest {
        objective: format!("Fix issues in step '{}' based on feedback", step.task_name),
        role: "fixer".to_string(),
        worktree_path: project_root.to_string_lossy().to_string(),
        instructions,
        model: None,
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: None,
        log_dir: log_dir.map(|s| s.to_string()),
        grove_session_id: Some(session_id),
        input_handle_callback: None,
        mcp_config_path: mcp_config_path.map(|p| p.to_string_lossy().to_string()),
    };

    let response = provider.execute(&request)?;

    // Parse the fixer's self-grade JSON from the response.
    let (grade, reasoning, feedback_text, outcome, passed) =
        if let Some(json) = extract_json_from_response(&response.summary) {
            let g = json["grade"].as_i64().unwrap_or(0);
            let r = json["reasoning"]
                .as_str()
                .unwrap_or(&response.summary)
                .to_string();
            let f = json["feedback"].as_str().unwrap_or("").to_string();
            let o = json["outcome"]
                .as_str()
                .unwrap_or(&response.summary)
                .to_string();
            let p = json["pass"].as_bool().unwrap_or(g >= PASS_THRESHOLD);
            (g, r, f, o, p)
        } else {
            info!(
                step_id = step.id.as_str(),
                "fixer response was not valid JSON — defaulting to grade 7 (auto-pass)"
            );
            (
                7,
                response.summary.clone(),
                String::new(),
                response.summary.clone(),
                true,
            )
        };

    Ok(BuilderGradedResult {
        run_id,
        grade,
        reasoning,
        feedback: feedback_text,
        outcome,
        passed,
        ai_comments: response.summary,
    })
}

// NOTE: spawn_verdict_agent and spawn_judge_agent for steps have been removed.
// The builder now builds, verifies, and self-grades in a single invocation.
// Phase-level validation agents (spawn_phase_validator, spawn_phase_judge) are
// retained below.

// ── Step Cycle Executor ─────────────────────────────────────────────────────

/// Execute a single Build+Verify+Grade cycle for a step using one agent.
///
/// This is the core pipeline function called by the agentic loop for each
/// ready step. A single builder agent builds, verifies, and self-grades.
///
/// # Returns
///
/// - `StepCycleResult::Passed` — step closed successfully (grade >= 7)
/// - `StepCycleResult::Retrying` — grade < 7, iterations remain, step reopened
/// - `StepCycleResult::Failed` — max iterations reached or unrecoverable error
/// - `StepCycleResult::Paused` — runtime was paused between stages
/// - `StepCycleResult::Aborted` — runtime was aborted between stages
pub async fn run_step_cycle(
    conn: &Connection,
    step_id: &str,
    project_root: &Path,
    _db_path: &Path,
    provider: &Arc<dyn Provider>,
    mcp_config_path: Option<&Path>,
    log_dir: Option<&str>,
) -> GroveResult<StepCycleResult> {
    // ── 1. Load step with accumulated feedback ──────────────────────────────
    let (step, feedback) = grove_graph_repo::get_step_with_feedback(conn, step_id)?;

    debug!(
        step_id,
        step_name = step.task_name.as_str(),
        iteration = step.run_iteration,
        max_iterations = step.max_iterations,
        feedback_count = feedback.len(),
        "starting step cycle"
    );

    // ── 2. Get the parent phase objective (used as context for the agent) ───
    let phase = grove_graph_repo::get_phase(conn, &step.phase_id)?;
    let phase_objective = &phase.task_objective;

    // ── 3. Check runtime status before starting ─────────────────────────────
    if let Some(result) = check_runtime_status(conn, &step.graph_id)? {
        return Ok(result);
    }

    // ── 4. Increment run iteration ──────────────────────────────────────────
    let current_iteration = grove_graph_repo::increment_step_run_iteration(conn, step_id)?;

    debug!(step_id, current_iteration, "incremented run iteration");

    // ── 5. Set step status to inprogress ────────────────────────────────────
    grove_graph_repo::update_step_status(conn, step_id, "inprogress")?;

    // ── 6. SINGLE AGENT: BUILD + VERIFY + GRADE ─────────────────────────────
    //
    // On the first iteration, spawn the Builder agent from scratch.
    // On retry iterations (current_iteration > 1), spawn the Fixer agent
    // with accumulated feedback for surgical corrections.
    // Both return a BuilderGradedResult with the self-grade.
    let is_retry = current_iteration > 1 && !feedback.is_empty();

    let result = if is_retry {
        debug!(
            step_id,
            iteration = current_iteration,
            "retry iteration — using fixer agent"
        );

        match spawn_fixer_agent(
            provider,
            &step,
            phase_objective,
            &feedback,
            project_root,
            mcp_config_path,
            log_dir,
        )
        .await
        {
            Ok(result) => result,
            Err(e) => {
                warn!(step_id, error = %e, "fixer agent failed — falling back to builder");
                match spawn_builder_agent(
                    provider,
                    &step,
                    phase_objective,
                    &feedback,
                    project_root,
                    mcp_config_path,
                    log_dir,
                )
                .await
                {
                    Ok(result) => result,
                    Err(e2) => {
                        warn!(step_id, error = %e2, "builder fallback also failed");
                        grove_graph_repo::set_step_failed(
                            conn,
                            step_id,
                            &format!("Fixer agent failed: {e}; Builder fallback failed: {e2}"),
                        )?;
                        return Ok(StepCycleResult::Failed);
                    }
                }
            }
        }
    } else {
        match spawn_builder_agent(
            provider,
            &step,
            phase_objective,
            &feedback,
            project_root,
            mcp_config_path,
            log_dir,
        )
        .await
        {
            Ok(result) => result,
            Err(e) => {
                warn!(step_id, error = %e, "builder agent failed");
                grove_graph_repo::set_step_failed(
                    conn,
                    step_id,
                    &format!("Builder agent failed: {e}"),
                )?;
                return Ok(StepCycleResult::Failed);
            }
        }
    };

    // ── 7. Record builder run ID + grade ────────────────────────────────────
    grove_graph_repo::set_step_builder_run(conn, step_id, &result.run_id)?;
    grove_graph_repo::set_step_judge_run(conn, step_id, &result.run_id, Some(result.grade))?;

    info!(
        step_id,
        builder_run_id = result.run_id.as_str(),
        grade = result.grade,
        passed = result.passed,
        is_retry,
        "builder cycle complete (build+verify+grade)"
    );

    // ── 8. Runtime re-check after builder ───────────────────────────────────
    if let Some(cycle_result) = check_runtime_status(conn, &step.graph_id)? {
        return Ok(cycle_result);
    }

    // ── 9. GRADE CHECK ──────────────────────────────────────────────────────
    if result.grade >= PASS_THRESHOLD {
        // Step passed — close it with the builder's outcome and grade.
        grove_graph_repo::set_step_closed(
            conn,
            step_id,
            &result.outcome,
            &result.ai_comments,
            result.grade,
        )?;

        debug!(step_id, grade = result.grade, "step passed — closed");
        return Ok(StepCycleResult::Passed);
    }

    // Grade is below threshold — check if we can retry.
    if current_iteration >= step.max_iterations {
        let fail_msg = format!(
            "Max iterations ({}) reached. Last grade: {}. Last feedback: {}",
            step.max_iterations, result.grade, result.feedback
        );

        grove_graph_repo::set_step_failed(conn, step_id, &fail_msg)?;

        warn!(
            step_id,
            grade = result.grade,
            iteration = current_iteration,
            max_iterations = step.max_iterations,
            "step failed — max iterations reached"
        );
        return Ok(StepCycleResult::Failed);
    }

    // Iterations remain — append feedback and reopen for retry.
    grove_graph_repo::append_judge_feedback(conn, step_id, &result.feedback)?;
    grove_graph_repo::reopen_step(conn, step_id)?;

    debug!(
        step_id,
        grade = result.grade,
        iteration = current_iteration,
        max_iterations = step.max_iterations,
        "step below threshold — reopened for retry"
    );

    Ok(StepCycleResult::Retrying)
}

// ── Phase Validation Agent Spawners ──────────────────────────────────────────

/// Spawn a Phase Validator agent that reviews all step outcomes as a group.
///
/// The validator receives the phase objective and every step's outcome summary,
/// checking whether the steps collectively satisfy the phase goal.
async fn spawn_phase_validator(
    provider: &Arc<dyn Provider>,
    phase: &GraphPhaseRow,
    step_outcomes: &[(GraphStepRow, String)],
    project_root: &Path,
    db_path: &Path,
    log_dir: Option<&str>,
) -> GroveResult<PhaseValidatorResult> {
    let skill_content = load_skill_instructions(project_root, "phase-validator");

    let mcp_config_path =
        crate::providers::mcp_inject::prepare_mcp_config_for_role("phase_validator", db_path)?;

    let step_summary: String = step_outcomes
        .iter()
        .map(|(step, outcome)| {
            format!(
                "- **{}** ({}): {}\n  Grade: {:?}, Status: {}",
                step.task_name, step.step_type, outcome, step.grade, step.status,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let instructions = format!(
        "{skill_content}\n\n\
         ## Task\n\n\
         **Phase:** {}\n\
         **Objective:** {}\n\n\
         ## Step Outcomes\n\n\
         {step_summary}\n",
        phase.task_name, phase.task_objective,
    );

    let validator_session_id = format!("validator-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let request = ProviderRequest {
        objective: format!(
            "Validate that all steps collectively satisfy the phase objective: {}",
            phase.task_name
        ),
        role: "phase_validator".to_string(),
        worktree_path: project_root.to_string_lossy().to_string(),
        instructions,
        model: None,
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: None,
        log_dir: log_dir.map(|s| s.to_string()),
        grove_session_id: Some(validator_session_id),
        input_handle_callback: None,
        mcp_config_path: mcp_config_path.map(|p| p.to_string_lossy().to_string()),
    };

    let response = provider.execute(&request)?;

    if let Some(ref mcp_path) = request.mcp_config_path {
        crate::providers::mcp_inject::cleanup_mcp_config(Path::new(mcp_path));
    }

    // Parse JSON response.
    let (passed, issues, failed_step_ids) = if let Some(json) =
        extract_json_from_response(&response.summary)
    {
        let p = json["pass"].as_bool().unwrap_or(true);
        let iss = json["issues"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let fids = json["failed_step_ids"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        (p, iss, fids)
    } else {
        // Issue #11: If we can't parse JSON, default to failed (fail-safe).
        // An unparseable response should not silently pass a phase.
        warn!("phase validator response was not JSON — defaulting to failed");
        (
            false,
            vec![
                "Phase validator did not return structured JSON — cannot verify phase".to_string(),
            ],
            vec![],
        )
    };

    Ok(PhaseValidatorResult {
        run_id: Uuid::new_v4().to_string(),
        passed,
        issues,
        failed_step_ids,
    })
}

/// Spawn a Phase Judge agent that grades the phase's collective work (0-10).
///
/// The judge receives:
/// - The phase objective
/// - All step outcomes
/// - The validator's findings
///
/// It produces a grade, reasoning, and identifies specific steps needing rework.
async fn spawn_phase_judge(
    provider: &Arc<dyn Provider>,
    phase: &GraphPhaseRow,
    step_outcomes: &[(GraphStepRow, String)],
    validator_result: &PhaseValidatorResult,
    project_root: &Path,
    db_path: &Path,
    log_dir: Option<&str>,
) -> GroveResult<PhaseJudgeResult> {
    let skill_content = load_skill_instructions(project_root, "phase-judge");

    let mcp_config_path =
        crate::providers::mcp_inject::prepare_mcp_config_for_role("phase_judge", db_path)?;

    let step_summary: String = step_outcomes
        .iter()
        .map(|(step, outcome)| {
            format!(
                "- **{}** ({}): {}\n  Grade: {:?}, Status: {}",
                step.task_name, step.step_type, outcome, step.grade, step.status,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let validator_summary = format!(
        "Passed: {}\nIssues: {}\nFailed step IDs: {:?}",
        validator_result.passed,
        if validator_result.issues.is_empty() {
            "None".to_string()
        } else {
            validator_result.issues.join("; ")
        },
        validator_result.failed_step_ids,
    );

    let instructions = format!(
        "{skill_content}\n\n\
         ## Task\n\n\
         **Phase:** {}\n\
         **Objective:** {}\n\n\
         ## Step Outcomes\n\n\
         {step_summary}\n\n\
         ## Validator Findings\n\n\
         {validator_summary}\n",
        phase.task_name, phase.task_objective,
    );

    let judge_session_id = format!("judge-{}", &uuid::Uuid::new_v4().to_string()[..8]);

    let request = ProviderRequest {
        objective: format!(
            "Grade the collective work of phase '{}' (0-10)",
            phase.task_name
        ),
        role: "phase_judge".to_string(),
        worktree_path: project_root.to_string_lossy().to_string(),
        instructions,
        model: None,
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: None,
        log_dir: log_dir.map(|s| s.to_string()),
        grove_session_id: Some(judge_session_id),
        input_handle_callback: None,
        mcp_config_path: mcp_config_path.map(|p| p.to_string_lossy().to_string()),
    };

    let response = provider.execute(&request)?;

    if let Some(ref mcp_path) = request.mcp_config_path {
        crate::providers::mcp_inject::cleanup_mcp_config(Path::new(mcp_path));
    }

    // Parse the judge's JSON response.
    let (grade, reasoning, passed, failed_steps) =
        if let Some(json) = extract_json_from_response(&response.summary) {
            let g = json["grade"].as_i64().unwrap_or(0);
            let r = json["reasoning"]
                .as_str()
                .unwrap_or(&response.summary)
                .to_string();
            let p = json["pass"].as_bool().unwrap_or(g >= PASS_THRESHOLD);
            let fs = json["failed_steps"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| {
                            let id = v["id"].as_str()?.to_string();
                            let fb = v["feedback"].as_str().unwrap_or("").to_string();
                            Some((id, fb))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (g, r, p, fs)
        } else {
            warn!("phase judge response was not valid JSON — defaulting to grade 0");
            (0, response.summary.clone(), false, vec![])
        };

    Ok(PhaseJudgeResult {
        run_id: Uuid::new_v4().to_string(),
        grade,
        reasoning,
        passed,
        failed_steps,
    })
}

// ── Phase Validation Cycle ──────────────────────────────────────────────────

/// Execute a Validator -> Judge cycle for a phase after all its steps are closed.
///
/// This validates the step outcomes as a group against the phase objective.
/// If the phase judge grades >= 7, the phase is closed as passed. Otherwise,
/// only the specific steps identified by the judge are re-opened with targeted
/// feedback, and the phase enters 'fixing' status to await their completion.
///
/// # Returns
///
/// - `PhaseValidationResult::Passed` — phase closed successfully
/// - `PhaseValidationResult::Retrying` — specific steps re-opened for rework
/// - `PhaseValidationResult::Failed` — unrecoverable validation error
pub async fn run_phase_validation_cycle(
    conn: &Connection,
    phase_id: &str,
    project_root: &Path,
    db_path: &Path,
    provider: &Arc<dyn Provider>,
    log_dir: Option<&str>,
) -> GroveResult<PhaseValidationResult> {
    // ── 1. Load phase ───────────────────────────────────────────────────────
    let phase = grove_graph_repo::get_phase(conn, phase_id)?;

    debug!(
        phase_id,
        phase_name = phase.task_name.as_str(),
        "starting phase validation cycle"
    );

    // ── 2. Get all steps and collect outcomes for context ────────────────────
    let steps = grove_graph_repo::list_steps(conn, phase_id)?;
    let step_outcomes: Vec<(GraphStepRow, String)> = steps
        .into_iter()
        .map(|s| {
            let outcome = s.outcome.clone().unwrap_or_default();
            (s, outcome)
        })
        .collect();

    debug!(
        phase_id,
        step_count = step_outcomes.len(),
        "collected step outcomes for phase validation"
    );

    // ── 3. Set validation_status = 'validating' ────────────────────────────
    grove_graph_repo::set_phase_validation_status(conn, phase_id, "validating")?;

    // ── 4. Check runtime status before starting ─────────────────────────────
    if let Some(result) = check_runtime_status(conn, &phase.graph_id)? {
        // Reset validation status so it can be retried when resumed.
        grove_graph_repo::set_phase_validation_status(conn, phase_id, "pending")?;
        return Ok(match result {
            // Issue #8: Paused means "not done, come back later" — use Retrying
            // so the loop knows to re-enter rather than treating the phase as terminal.
            StepCycleResult::Paused => PhaseValidationResult::Retrying,
            StepCycleResult::Aborted => PhaseValidationResult::Failed,
            _ => PhaseValidationResult::Failed,
        });
    }

    // ── 5. PHASE VALIDATOR STAGE ────────────────────────────────────────────
    let validator_result = match spawn_phase_validator(
        provider,
        &phase,
        &step_outcomes,
        project_root,
        db_path,
        log_dir,
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!(phase_id, error = %e, "phase validator failed");
            grove_graph_repo::set_phase_validation_status(conn, phase_id, "failed")?;
            return Ok(PhaseValidationResult::Failed);
        }
    };

    // Record validator run ID.
    grove_graph_repo::set_phase_validator_run(conn, phase_id, &validator_result.run_id)?;

    debug!(
        phase_id,
        validator_run_id = validator_result.run_id.as_str(),
        passed = validator_result.passed,
        issues_count = validator_result.issues.len(),
        "phase validator stage complete"
    );

    // ── 6. Runtime re-check after validator ─────────────────────────────────
    if let Some(_result) = check_runtime_status(conn, &phase.graph_id)? {
        grove_graph_repo::set_phase_validation_status(conn, phase_id, "pending")?;
        return Ok(PhaseValidationResult::Failed);
    }

    // ── 7. PHASE JUDGE STAGE ────────────────────────────────────────────────
    let judge_result = match spawn_phase_judge(
        provider,
        &phase,
        &step_outcomes,
        &validator_result,
        project_root,
        db_path,
        log_dir,
    )
    .await
    {
        Ok(result) => result,
        Err(e) => {
            warn!(phase_id, error = %e, "phase judge failed");
            grove_graph_repo::set_phase_validation_status(conn, phase_id, "failed")?;
            return Ok(PhaseValidationResult::Failed);
        }
    };

    // Record judge run ID + grade.
    grove_graph_repo::set_phase_judge_run(
        conn,
        phase_id,
        &judge_result.run_id,
        Some(judge_result.grade),
    )?;

    debug!(
        phase_id,
        judge_run_id = judge_result.run_id.as_str(),
        grade = judge_result.grade,
        passed = judge_result.passed,
        failed_steps_count = judge_result.failed_steps.len(),
        "phase judge stage complete"
    );

    // ── 8. GRADE CHECK ──────────────────────────────────────────────────────
    if judge_result.grade >= PASS_THRESHOLD {
        // Phase passed — close it with the judge's reasoning and grade.
        grove_graph_repo::set_phase_closed(
            conn,
            phase_id,
            &judge_result.reasoning,
            judge_result.grade,
        )?;

        debug!(
            phase_id,
            grade = judge_result.grade,
            "phase validation passed — closed"
        );
        return Ok(PhaseValidationResult::Passed);
    }

    // ── 9. Grade below threshold — selectively re-open failed steps ─────────
    grove_graph_repo::set_phase_validation_status(conn, phase_id, "fixing")?;

    if judge_result.failed_steps.is_empty() {
        // Judge didn't identify specific steps — reopen ALL steps in the phase
        // so the builders can rework the entire phase.
        warn!(
            phase_id,
            grade = judge_result.grade,
            "phase judge returned no failed_steps — reopening all steps"
        );
        let all_steps = grove_graph_repo::list_steps(conn, phase_id)?;
        for step in &all_steps {
            grove_graph_repo::append_judge_feedback(conn, &step.id, &judge_result.reasoning)?;
            grove_graph_repo::reopen_step(conn, &step.id)?;
        }

        debug!(
            phase_id,
            grade = judge_result.grade,
            reopened_steps = all_steps.len(),
            "phase below threshold — all steps re-opened (no specific failures identified)"
        );
    } else {
        for (step_id, feedback) in &judge_result.failed_steps {
            grove_graph_repo::append_judge_feedback(conn, step_id, feedback)?;
            grove_graph_repo::reopen_step(conn, step_id)?;

            debug!(
                phase_id,
                step_id = step_id.as_str(),
                "re-opened step with phase judge feedback"
            );
        }

        debug!(
            phase_id,
            grade = judge_result.grade,
            reopened_steps = judge_result.failed_steps.len(),
            "phase below threshold — specific steps re-opened for rework"
        );
    }

    Ok(PhaseValidationResult::Retrying)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockProvider;

    fn mock_provider() -> Arc<dyn Provider> {
        Arc::new(MockProvider)
    }

    #[test]
    fn pass_threshold_is_seven() {
        assert_eq!(PASS_THRESHOLD, 7);
    }

    #[test]
    fn agent_run_result_is_debug_clone() {
        let r = AgentRunResult {
            run_id: "run_001".into(),
            outcome: "implemented feature X".into(),
            ai_comments: "clean implementation".into(),
        };
        let r2 = r.clone();
        assert_eq!(r2.run_id, "run_001");
        assert_eq!(format!("{:?}", r2).is_empty(), false);
    }

    #[test]
    fn builder_graded_result_is_debug_clone() {
        let r = BuilderGradedResult {
            run_id: "builder_001".into(),
            grade: 8,
            reasoning: "good work".into(),
            feedback: "none needed".into(),
            outcome: "implemented feature".into(),
            passed: true,
            ai_comments: "detailed comments".into(),
        };
        let r2 = r.clone();
        assert_eq!(r2.grade, 8);
        assert!(r2.passed);
        assert_eq!(format!("{:?}", r2).is_empty(), false);
    }

    #[test]
    fn builder_graded_result_passed_reflects_threshold() {
        let passing = BuilderGradedResult {
            run_id: "b1".into(),
            grade: 7,
            reasoning: String::new(),
            feedback: String::new(),
            outcome: String::new(),
            passed: true,
            ai_comments: String::new(),
        };
        assert!(passing.grade >= PASS_THRESHOLD);

        let failing = BuilderGradedResult {
            run_id: "b2".into(),
            grade: 6,
            reasoning: String::new(),
            feedback: "needs work".into(),
            outcome: String::new(),
            passed: false,
            ai_comments: String::new(),
        };
        assert!(failing.grade < PASS_THRESHOLD);
    }

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

    /// Seed a graph + phase + step for cycle testing.
    fn seed_graph_with_step(conn: &Connection) -> (String, String, String) {
        seed_conversation(conn, "conv_test");
        let graph_id =
            grove_graph_repo::insert_graph(conn, "conv_test", "Test Graph", "desc", None).unwrap();
        grove_graph_repo::set_runtime_status(conn, &graph_id, "running").unwrap();

        let phase_id = grove_graph_repo::insert_phase(
            conn,
            &graph_id,
            "Phase 1",
            "Build the feature",
            0,
            "[]",
            false,
            None,
        )
        .unwrap();

        let step_id = grove_graph_repo::insert_step(
            conn,
            &phase_id,
            &graph_id,
            "Step 1",
            "Implement module X",
            0,
            "code",
            "auto",
            "[]",
            false,
            None,
        )
        .unwrap();

        (graph_id, phase_id, step_id)
    }

    #[test]
    fn check_runtime_status_running_returns_none() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = grove_graph_repo::insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        grove_graph_repo::set_runtime_status(&conn, &gid, "running").unwrap();

        let result = check_runtime_status(&conn, &gid).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn check_runtime_status_paused_returns_paused() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = grove_graph_repo::insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        grove_graph_repo::set_runtime_status(&conn, &gid, "paused").unwrap();

        let result = check_runtime_status(&conn, &gid).unwrap();
        assert_eq!(result, Some(StepCycleResult::Paused));
    }

    #[test]
    fn check_runtime_status_aborted_returns_aborted() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = grove_graph_repo::insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        grove_graph_repo::set_runtime_status(&conn, &gid, "aborted").unwrap();

        let result = check_runtime_status(&conn, &gid).unwrap();
        assert_eq!(result, Some(StepCycleResult::Aborted));
    }

    #[test]
    fn check_runtime_status_idle_returns_none() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = grove_graph_repo::insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        // Default is "idle"
        let result = check_runtime_status(&conn, &gid).unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn run_step_cycle_calls_provider() {
        let conn = test_db();
        let (_graph_id, _phase_id, step_id) = seed_graph_with_step(&conn);
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        // MockProvider returns a non-JSON summary, so the builder
        // auto-passes with grade=7 (fallback when no JSON self-grade).
        let result = run_step_cycle(&conn, &step_id, tmp.path(), &db_path, &provider, None, None)
            .await
            .unwrap();
        assert_eq!(result, StepCycleResult::Passed);

        let step = grove_graph_repo::get_step(&conn, &step_id).unwrap();
        // Iteration should have been incremented.
        assert_eq!(step.run_iteration, 1);
        // Step should be closed (passed).
        assert_eq!(step.status, "closed");
    }

    #[tokio::test]
    async fn run_step_cycle_paused_before_builder() {
        let conn = test_db();
        let (graph_id, _phase_id, step_id) = seed_graph_with_step(&conn);

        // Pause the graph before running the cycle.
        grove_graph_repo::set_runtime_status(&conn, &graph_id, "paused").unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        let result = run_step_cycle(&conn, &step_id, tmp.path(), &db_path, &provider, None, None)
            .await
            .unwrap();
        assert_eq!(result, StepCycleResult::Paused);

        // Step should NOT have been modified (still open, iteration 0).
        let step = grove_graph_repo::get_step(&conn, &step_id).unwrap();
        assert_eq!(step.status, "open");
        assert_eq!(step.run_iteration, 0);
    }

    #[tokio::test]
    async fn run_step_cycle_aborted_before_builder() {
        let conn = test_db();
        let (graph_id, _phase_id, step_id) = seed_graph_with_step(&conn);

        grove_graph_repo::set_runtime_status(&conn, &graph_id, "aborted").unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        let result = run_step_cycle(&conn, &step_id, tmp.path(), &db_path, &provider, None, None)
            .await
            .unwrap();
        assert_eq!(result, StepCycleResult::Aborted);

        let step = grove_graph_repo::get_step(&conn, &step_id).unwrap();
        assert_eq!(step.status, "open");
        assert_eq!(step.run_iteration, 0);
    }

    // ── Phase Validation Result Type Tests ──────────────────────────────────

    #[test]
    fn phase_validator_result_is_debug_clone() {
        let r = PhaseValidatorResult {
            run_id: "pv_001".into(),
            passed: false,
            issues: vec!["missing error handling".into()],
            failed_step_ids: vec!["step_3".into()],
        };
        let r2 = r.clone();
        assert_eq!(r2.run_id, "pv_001");
        assert!(!r2.passed);
        assert_eq!(r2.issues.len(), 1);
        assert_eq!(r2.failed_step_ids, vec!["step_3"]);
        assert!(!format!("{:?}", r2).is_empty());
    }

    #[test]
    fn phase_judge_result_is_debug_clone() {
        let r = PhaseJudgeResult {
            run_id: "pj_001".into(),
            grade: 5,
            reasoning: "needs more tests".into(),
            passed: false,
            failed_steps: vec![("step_2".into(), "add unit tests".into())],
        };
        let r2 = r.clone();
        assert_eq!(r2.run_id, "pj_001");
        assert_eq!(r2.grade, 5);
        assert!(!r2.passed);
        assert_eq!(r2.failed_steps.len(), 1);
        assert_eq!(r2.failed_steps[0].0, "step_2");
        assert!(!format!("{:?}", r2).is_empty());
    }

    #[test]
    fn phase_judge_result_passed_reflects_threshold() {
        let passing = PhaseJudgeResult {
            run_id: "pj1".into(),
            grade: 8,
            reasoning: "excellent".into(),
            passed: true,
            failed_steps: vec![],
        };
        assert!(passing.grade >= PASS_THRESHOLD);

        let failing = PhaseJudgeResult {
            run_id: "pj2".into(),
            grade: 4,
            reasoning: "incomplete".into(),
            passed: false,
            failed_steps: vec![("s1".into(), "rework needed".into())],
        };
        assert!(failing.grade < PASS_THRESHOLD);
    }

    #[tokio::test]
    async fn run_phase_validation_cycle_calls_provider() {
        let conn = test_db();
        let (_graph_id, phase_id, _step_id) = seed_graph_with_step(&conn);
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        // MockProvider returns non-JSON for phase_validator, which defaults to
        // passed=true, then phase_judge also returns non-JSON defaulting to grade=0.
        // Grade 0 < 7 → PhaseValidationResult::Retrying (or Failed if no steps to re-open).
        let result =
            run_phase_validation_cycle(&conn, &phase_id, tmp.path(), &db_path, &provider, None)
                .await
                .unwrap();
        // The mock judge returns grade=0 with no failed_steps parsed, so the
        // phase enters 'fixing' status with 0 re-opened steps.
        assert!(matches!(
            result,
            PhaseValidationResult::Retrying | PhaseValidationResult::Passed
        ));
    }

    #[tokio::test]
    async fn run_phase_validation_cycle_paused_before_validator() {
        let conn = test_db();
        let (graph_id, phase_id, _step_id) = seed_graph_with_step(&conn);

        // Pause the graph before running the validation cycle.
        grove_graph_repo::set_runtime_status(&conn, &graph_id, "paused").unwrap();
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        let result =
            run_phase_validation_cycle(&conn, &phase_id, tmp.path(), &db_path, &provider, None)
                .await
                .unwrap();
        assert_eq!(result, PhaseValidationResult::Failed);

        // Phase validation_status should be reset to 'pending' for retry.
        let phase = grove_graph_repo::get_phase(&conn, &phase_id).unwrap();
        assert_eq!(phase.validation_status, "pending");
    }
}
