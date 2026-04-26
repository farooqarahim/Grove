//! Dispatch the orchestrator reasoning agent for complex decisions.
//!
//! The orchestrator is a short-lived agent spawned only when the Rust loop
//! needs a judgment call (complex chunking, failover, phase judge triage, deadlock).

use crate::db::repositories::grove_graph_repo::{GraphPhaseRow, GraphStepRow};
use crate::errors::{GroveError, GroveResult};
use crate::grove_graph::chunking::StepChunk;
use crate::grove_graph::skill_loader;
use crate::providers::mcp_inject;
use crate::providers::{Provider, ProviderRequest};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tracing::info;
use uuid::Uuid;

/// Decision type for the orchestrator.
#[derive(Debug, Clone)]
pub enum DecisionType {
    /// Phase has complex DAG — need optimal chunk grouping.
    ChunkPlanning {
        phase: GraphPhaseRow,
        open_steps: Vec<GraphStepRow>,
        max_chunk_size: usize,
    },
    /// Worker failed — decide recovery strategy.
    FailoverRecovery {
        phase: GraphPhaseRow,
        completed_steps: Vec<String>,
        failed_steps: Vec<String>,
        remaining_steps: Vec<String>,
        error_context: String,
    },
    /// Phase judge rejected — decide which steps to reopen.
    PhaseValidationFailure {
        phase: GraphPhaseRow,
        judge_grade: i64,
        judge_feedback: String,
        step_outcomes: Vec<(GraphStepRow, String)>,
        failed_step_ids: Vec<String>,
    },
    /// No ready steps but open steps exist — diagnose.
    Deadlock {
        graph_id: String,
        all_steps: Vec<GraphStepRow>,
    },
}

/// Orchestrator decision output — parsed from JSON response.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChunkDecision {
    pub chunks: Vec<Vec<String>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct FailoverDecision {
    pub strategy: String, // "resume" | "fresh_chunk" | "re_approach"
    pub session_id: Option<String>,
    pub chunks: Option<Vec<Vec<String>>>,
    pub reset_steps: Option<Vec<String>>,
    pub context_note: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TriageDecision {
    pub reopen_steps: Vec<String>,
    pub feedback_per_step: HashMap<String, String>,
    pub chunks: Vec<Vec<String>>,
    pub context_note: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DeadlockDecision {
    pub diagnosis: String,
    pub action: String, // "reset_and_retry" | "skip" | "escalate_to_user"
    pub reset_steps: Option<Vec<String>>,
    pub skip_steps: Option<Vec<String>>,
    pub context_note: Option<String>,
}

/// Build the context string for a given decision type.
fn build_context(decision: &DecisionType) -> String {
    match decision {
        DecisionType::ChunkPlanning {
            phase,
            open_steps,
            max_chunk_size,
        } => {
            let mut ctx = "## Decision: Complex Chunk Planning\n\n".to_string();
            ctx.push_str(&format!("Phase: \"{}\"\n", phase.task_objective));
            ctx.push_str(&format!("Max chunk size: {}\n\n", max_chunk_size));
            ctx.push_str("Steps and dependencies:\n");
            for step in open_steps {
                let deps = if step.depends_on_json.is_empty() || step.depends_on_json == "[]" {
                    "none"
                } else {
                    &step.depends_on_json
                };
                ctx.push_str(&format!(
                    "  {}: \"{}\" → deps: {}\n",
                    step.id, step.task_objective, deps
                ));
            }
            ctx
        }
        DecisionType::FailoverRecovery {
            phase,
            completed_steps,
            failed_steps,
            remaining_steps,
            error_context,
        } => {
            let mut ctx = "## Decision: Failover Recovery\n\n".to_string();
            ctx.push_str(&format!("Phase: \"{}\"\n", phase.task_objective));
            ctx.push_str(&format!("Completed: {:?}\n", completed_steps));
            ctx.push_str(&format!("Failed: {:?}\n", failed_steps));
            ctx.push_str(&format!("Remaining: {:?}\n", remaining_steps));
            ctx.push_str(&format!("Error: {}\n", error_context));
            ctx
        }
        DecisionType::PhaseValidationFailure {
            phase,
            judge_grade,
            judge_feedback,
            step_outcomes,
            failed_step_ids,
        } => {
            let mut ctx = "## Decision: Phase Validation Failure Triage\n\n".to_string();
            ctx.push_str(&format!("Phase: \"{}\"\n", phase.task_objective));
            ctx.push_str(&format!("Phase judge grade: {}/10\n", judge_grade));
            ctx.push_str(&format!("Judge feedback: {}\n\n", judge_feedback));
            ctx.push_str("Step outcomes:\n");
            for (step, outcome) in step_outcomes {
                ctx.push_str(&format!(
                    "  {} (grade {}): {}\n",
                    step.id,
                    step.grade.unwrap_or(0),
                    outcome
                ));
            }
            ctx.push_str(&format!(
                "\nFailed step IDs identified by judge: {:?}\n",
                failed_step_ids
            ));
            ctx
        }
        DecisionType::Deadlock {
            graph_id,
            all_steps,
        } => {
            let mut ctx = "## Decision: Deadlock Diagnosis\n\n".to_string();
            ctx.push_str(&format!("Graph: {}\n\n", graph_id));
            ctx.push_str("All step statuses:\n");
            for step in all_steps {
                let deps = if step.depends_on_json.is_empty() || step.depends_on_json == "[]" {
                    "none"
                } else {
                    &step.depends_on_json
                };
                ctx.push_str(&format!(
                    "  {} [{}]: \"{}\" → deps: {}\n",
                    step.id, step.status, step.task_objective, deps
                ));
            }
            ctx
        }
    }
}

/// Build the conversation id used to key the graph-scoped orchestrator
/// host. The orchestrator persona is shared across all phases of a graph
/// (chunk planning, failover, deadlock diagnosis, validation triage), so we
/// keep one warm host per graph.
pub fn orchestrator_conversation_id(graph_id: &str) -> String {
    format!("hive:{graph_id}:orchestrator")
}

/// Successful orchestrator dispatch: the raw JSON-bearing response and the
/// provider-side session id observed on this turn (if any). The caller
/// persists the session id so the next decision on the same graph can
/// cold-resume after registry eviction.
#[derive(Debug, Clone)]
pub struct OrchestratorDispatchResult {
    pub response: String,
    pub provider_session_id: Option<String>,
}

/// Dispatch the orchestrator agent and return the raw JSON response plus
/// the provider session id observed on this turn (if any). Callers persist
/// the session id so the next decision on the same graph can cold-resume
/// after registry eviction.
///
/// NOTE: This is a blocking function (Provider::execute is sync).
/// Callers in async contexts should wrap in `tokio::task::spawn_blocking`.
pub fn dispatch_orchestrator_full(
    provider: &Arc<dyn Provider>,
    decision: &DecisionType,
    graph_id: &str,
    project_root: &Path,
    db_path: &Path,
    log_dir: Option<&str>,
    resume_provider_session_id: Option<&str>,
) -> GroveResult<OrchestratorDispatchResult> {
    let skill_content = skill_loader::load_skill(project_root, "execution-orchestrator");
    let context = build_context(decision);
    let instructions = format!("{}\n\n{}", skill_content, context);

    let decision_label = match decision {
        DecisionType::ChunkPlanning { .. } => "chunk_planning",
        DecisionType::FailoverRecovery { .. } => "failover_recovery",
        DecisionType::PhaseValidationFailure { .. } => "phase_validation_failure",
        DecisionType::Deadlock { .. } => "deadlock_diagnosis",
    };

    let mcp_config: Option<std::path::PathBuf> =
        mcp_inject::prepare_mcp_config_for_role("orchestrator", db_path)?;
    let mcp_path: Option<String> = mcp_config
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned());

    info!(
        decision_type = decision_label,
        graph_id,
        warm_resume = resume_provider_session_id.is_some(),
        "dispatching orchestrator"
    );

    let session_id = format!("orch-{}", &Uuid::new_v4().to_string()[..8]);

    let request = ProviderRequest {
        objective: format!("Decide: {}", decision_label),
        role: "orchestrator".into(),
        worktree_path: project_root.to_string_lossy().into_owned(),
        instructions,
        model: None, // Use default (most capable)
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: resume_provider_session_id.map(|s| s.to_string()),
        log_dir: log_dir.map(|s| s.to_string()),
        grove_session_id: Some(session_id),
        input_handle_callback: None,
        mcp_config_path: mcp_path,
        conversation_id: Some(orchestrator_conversation_id(graph_id)),
    };

    let response = provider.execute(&request)?;

    if let Some(ref path) = mcp_config {
        mcp_inject::cleanup_mcp_config(path);
    }

    Ok(OrchestratorDispatchResult {
        response: response.summary,
        provider_session_id: response.provider_session_id,
    })
}

/// Parse a chunk planning decision from orchestrator response.
pub fn parse_chunk_decision(response: &str) -> GroveResult<ChunkDecision> {
    extract_json_and_parse(response)
}

/// Parse a failover decision from orchestrator response.
pub fn parse_failover_decision(response: &str) -> GroveResult<FailoverDecision> {
    extract_json_and_parse(response)
}

/// Parse a triage decision from orchestrator response.
pub fn parse_triage_decision(response: &str) -> GroveResult<TriageDecision> {
    extract_json_and_parse(response)
}

/// Parse a deadlock decision from orchestrator response.
pub fn parse_deadlock_decision(response: &str) -> GroveResult<DeadlockDecision> {
    extract_json_and_parse(response)
}

/// Extract the first balanced JSON object from text using bracket-depth counting.
///
/// This is more robust than `find`/`rfind` because it correctly handles nested
/// braces and ignores trailing text after the first complete JSON object (Issue #15).
fn extract_json_block(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0;
    for (i, c) in text[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extract JSON from an agent response (may contain surrounding text).
fn extract_json_and_parse<T: serde::de::DeserializeOwned>(response: &str) -> GroveResult<T> {
    // Try direct parse first.
    if let Ok(parsed) = serde_json::from_str::<T>(response) {
        return Ok(parsed);
    }

    // Try to find JSON within markdown code block.
    if let Some(start) = response.find("```json") {
        let json_start = start + 7;
        if let Some(end) = response[json_start..].find("```") {
            let json_str = response[json_start..json_start + end].trim();
            if let Ok(parsed) = serde_json::from_str::<T>(json_str) {
                return Ok(parsed);
            }
        }
    }

    // Try to find the first balanced JSON object using bracket-depth counting.
    if let Some(json_str) = extract_json_block(response) {
        if let Ok(parsed) = serde_json::from_str::<T>(json_str) {
            return Ok(parsed);
        }
    }

    Err(GroveError::Runtime(format!(
        "failed to parse orchestrator JSON response: {}",
        &response[..response.len().min(200)]
    )))
}

/// Validate a chunk decision: ensure all step IDs exist and deps are ordered.
pub fn validate_chunk_decision(
    decision: &ChunkDecision,
    available_step_ids: &HashSet<String>,
    steps: &[GraphStepRow],
) -> Result<(), String> {
    let step_map: HashMap<&str, &GraphStepRow> = steps.iter().map(|s| (s.id.as_str(), s)).collect();

    let mut completed_so_far: HashSet<&str> = HashSet::new();

    for (i, chunk) in decision.chunks.iter().enumerate() {
        // Check all IDs exist.
        for id in chunk {
            if !available_step_ids.contains(id) {
                return Err(format!("chunk {} references unknown step_id '{}'", i, id));
            }
        }
        // Check dependency ordering within the chunk.
        let mut seen_in_chunk: HashSet<&str> = HashSet::new();
        for id in chunk {
            if let Some(step) = step_map.get(id.as_str()) {
                let deps = super::chunking::parse_depends_on(&step.depends_on_json);
                for dep in &deps {
                    if available_step_ids.contains(dep)
                        && !completed_so_far.contains(dep.as_str())
                        && !seen_in_chunk.contains(dep.as_str())
                    {
                        return Err(format!(
                            "chunk {}: step '{}' depends on '{}' which is not completed or earlier in this chunk",
                            i, id, dep
                        ));
                    }
                }
            }
            seen_in_chunk.insert(id.as_str());
        }
        completed_so_far.extend(seen_in_chunk);
    }
    Ok(())
}

/// Convert a validated chunk decision into `StepChunk` objects.
pub fn chunks_from_decision(decision: &ChunkDecision, steps: &[GraphStepRow]) -> Vec<StepChunk> {
    let step_map: HashMap<&str, &GraphStepRow> = steps.iter().map(|s| (s.id.as_str(), s)).collect();

    decision
        .chunks
        .iter()
        .map(|chunk_ids| {
            let chunk_steps: Vec<GraphStepRow> = chunk_ids
                .iter()
                .filter_map(|id| step_map.get(id.as_str()).map(|s| (*s).clone()))
                .collect();
            StepChunk {
                step_ids: chunk_ids.clone(),
                steps: chunk_steps,
            }
        })
        .collect()
}
