//! Pre-Planning Loop & Graph Creator — setup pipeline for new graphs.
//!
//! This module runs before the agentic execution loop. It handles two phases:
//!
//! 1. **Pre-Planning** ([`run_pre_planning_loop`]) — ensures foundational documents
//!    (PRD, system design, guidelines) exist. For each missing doc required by the
//!    graph's config, a PrePlanner agent is spawned to generate it. The loop retries
//!    up to `MAX_PREPLANNING_ITERATIONS` times.
//!
//! 2. **Graph Creation** ([`run_graph_creation`]) — spawns the GraphCreator agent
//!    that reads the source document and generates the full phase/step DAG. Retries
//!    up to `max_reruns` times (from the graph record).

use std::path::Path;
use std::sync::Arc;

use crate::db::repositories::grove_graph_repo;
use crate::errors::{GroveError, GroveResult};
use crate::grove_graph::GraphConfig;
use crate::providers::{Provider, ProviderRequest};
use rusqlite::Connection;
use tracing::{debug, info, warn};

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum number of pre-planning loop iterations before giving up.
const MAX_PREPLANNING_ITERATIONS: u32 = 10;

/// Maximum number of graph creation retries (independent of graph.max_reruns).
const MAX_CREATION_RETRIES: u32 = 3;

// ── Document Type Descriptor ─────────────────────────────────────────────────

/// A required document type and its expected filename within the project root.
struct RequiredDoc {
    /// Human-readable label used in log messages.
    label: &'static str,
    /// Filename expected in the project root directory (e.g. "PRD.md").
    filename: &'static str,
}

/// Determine which documents are required based on the graph config flags.
fn required_docs(config: &GraphConfig) -> Vec<RequiredDoc> {
    let mut docs = Vec::new();
    if config.doc_prd {
        docs.push(RequiredDoc {
            label: "PRD",
            filename: "PRD.md",
        });
    }
    if config.doc_system_design {
        docs.push(RequiredDoc {
            label: "System Design",
            filename: "SYSTEM_DESIGN.md",
        });
    }
    if config.doc_guidelines {
        docs.push(RequiredDoc {
            label: "Guidelines",
            filename: "GUIDELINES.md",
        });
    }
    docs
}

/// Check which required docs are missing from the project root.
fn find_missing_docs<'a>(project_root: &Path, required: &'a [RequiredDoc]) -> Vec<&'a RequiredDoc> {
    required
        .iter()
        .filter(|doc| !project_root.join(doc.filename).exists())
        .collect()
}

// ── Skill Loader ─────────────────────────────────────────────────────────────

/// Load skill instructions from `skills/graph/{skill_dir}/SKILL.md`.
///
/// Resolution order:
/// 1. Project-local: `{project_root}/skills/graph/{skill_dir}/SKILL.md`
/// 2. Repo-root relative to exe: walks up from exe dir to find `skills/graph/`
///    (covers dev builds where exe is in `target/debug/` and skills are in repo root)
/// 3. Adjacent to executable: `{exe_dir}/skills/graph/{skill_dir}/SKILL.md`
///    (covers packaged distributions)
fn load_skill_instructions(project_root: &Path, skill_dir: &str) -> String {
    crate::grove_graph::skill_loader::load_skill(project_root, skill_dir)
}

// ── Agent Spawners ──────────────────────────────────────────────────────────

/// Spawn a PrePlanner agent to generate a missing foundational document.
///
/// The agent receives the source document context and the document type to
/// generate. On success, it writes the file to disk in the project root.
async fn spawn_preplanner_agent(
    provider: &Arc<dyn Provider>,
    _graph_id: &str,
    doc_label: &str,
    doc_filename: &str,
    source_doc_content: &str,
    project_root: &Path,
    db_path: &Path,
) -> GroveResult<()> {
    let skill_content = load_skill_instructions(project_root, "pre-planning");

    let mcp_config_path =
        crate::providers::mcp_inject::prepare_mcp_config_for_role("pre_planner", db_path)?;

    let instructions = format!(
        "{skill_content}\n\n\
         ## Task\n\n\
         Generate the **{doc_label}** document and write it to `{doc_filename}` \
         in the project root directory.\n\n\
         ## Source Specification\n\n\
         {source_doc_content}"
    );

    let request = ProviderRequest {
        objective: format!("Generate the {doc_label} document ({doc_filename}) for this project"),
        role: "pre_planner".to_string(),
        worktree_path: project_root.to_string_lossy().to_string(),
        instructions,
        model: None,
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: None,
        log_dir: None,
        grove_session_id: None,
        input_handle_callback: None,
        mcp_config_path: mcp_config_path.map(|p| p.to_string_lossy().to_string()),
    };

    let response = provider.execute(&request)?;

    debug!(
        doc_label,
        doc_filename,
        summary = response.summary.as_str(),
        "preplanner agent completed"
    );

    // Clean up MCP config temp file.
    if let Some(ref mcp_path) = request.mcp_config_path {
        crate::providers::mcp_inject::cleanup_mcp_config(Path::new(mcp_path));
    }

    Ok(())
}

// ── JSON-based graph plan types ──────────────────────────────────────────────

/// A complete graph plan returned as a single JSON blob from the planner LLM.
/// Rust parses this and inserts phases/steps directly — no MCP round-trips.
#[derive(Debug, serde::Deserialize)]
struct GraphPlan {
    phases: Vec<PhasePlan>,
}

#[derive(Debug, serde::Deserialize)]
struct PhasePlan {
    title: String,
    task_objective: String,
    ordinal: i64,
    #[serde(default)]
    depends_on: Vec<String>,
    steps: Vec<StepPlan>,
}

#[derive(Debug, serde::Deserialize)]
struct StepPlan {
    title: String,
    task_objective: String,
    ordinal: i64,
    #[serde(default = "default_step_type")]
    step_type: String,
    #[serde(default = "default_execution_mode")]
    execution_mode: String,
    #[serde(default)]
    depends_on: Vec<String>,
}

fn default_step_type() -> String {
    "code".to_string()
}
fn default_execution_mode() -> String {
    "auto".to_string()
}

/// Extract the first JSON object from LLM output (strips markdown code fences).
fn extract_json(text: &str) -> Option<&str> {
    // Strip ```json ... ``` or ``` ... ``` fences if present.
    let stripped = if let Some(start) = text.find("```") {
        let after_fence = &text[start + 3..];
        let after_lang = after_fence
            .strip_prefix("json")
            .unwrap_or(after_fence)
            .trim_start_matches('\n');
        if let Some(end) = after_lang.find("```") {
            &after_lang[..end]
        } else {
            after_lang
        }
    } else {
        text
    };

    // Find the outermost { ... } block.
    let start = stripped.find('{')?;
    let slice = &stripped[start..];
    let mut depth = 0i32;
    for (i, ch) in slice.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&slice[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Insert all phases and steps from a `GraphPlan` into the DB.
fn insert_plan_from_json(conn: &Connection, graph_id: &str, plan: &GraphPlan) -> GroveResult<()> {
    use std::collections::HashMap;

    // Title → generated DB ID mappings for resolving depends_on references.
    let mut phase_title_to_id: HashMap<String, String> = HashMap::new();
    let mut step_title_to_id: HashMap<String, String> = HashMap::new();

    // ── Pass 1: Insert all phases and steps with empty depends_on ────────────
    // We insert with "[]" first because the dependency targets may not exist yet.
    #[allow(clippy::type_complexity)]
    let mut phase_step_pairs: Vec<(String, Vec<(String, Vec<String>)>)> = Vec::new();

    for phase in &plan.phases {
        let phase_id = grove_graph_repo::insert_phase(
            conn,
            graph_id,
            &phase.title,
            &phase.task_objective,
            phase.ordinal,
            "[]", // will be resolved in pass 2
            false,
            None,
        )?;

        let now = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let _ = conn.execute(
            "UPDATE grove_graphs \
             SET phases_created_count = phases_created_count + 1, updated_at = ?1 \
             WHERE id = ?2",
            rusqlite::params![now, graph_id],
        );

        phase_title_to_id.insert(phase.title.clone(), phase_id.clone());

        let mut step_deps: Vec<(String, Vec<String>)> = Vec::new();

        for step in &phase.steps {
            let step_id = grove_graph_repo::insert_step(
                conn,
                &phase_id,
                graph_id,
                &step.title,
                &step.task_objective,
                step.ordinal,
                &step.step_type,
                &step.execution_mode,
                "[]", // will be resolved in pass 2
                false,
                None,
            )?;

            let _ = conn.execute(
                "UPDATE grove_graphs \
                 SET steps_created_count = steps_created_count + 1, updated_at = ?1 \
                 WHERE id = ?2",
                rusqlite::params![now, graph_id],
            );

            step_title_to_id.insert(step.title.clone(), step_id.clone());
            step_deps.push((step_id, step.depends_on.clone()));
        }

        phase_step_pairs.push((phase_id, step_deps));
    }

    // ── Pass 2: Resolve title-based depends_on to actual IDs ─────────────────
    // The LLM generates depends_on as title strings. The DAG query in
    // get_ready_steps_for_graph matches against step/phase IDs. Resolve here.
    for (i, phase) in plan.phases.iter().enumerate() {
        let (ref phase_id, ref step_deps) = phase_step_pairs[i];

        // Resolve phase-level dependencies (title → phase ID).
        if !phase.depends_on.is_empty() {
            let resolved: Vec<String> = phase
                .depends_on
                .iter()
                .filter_map(|title| phase_title_to_id.get(title).cloned())
                .collect();
            if !resolved.is_empty() {
                let json = serde_json::to_string(&resolved).unwrap_or_else(|_| "[]".to_string());
                let _ = conn.execute(
                    "UPDATE graph_phases SET depends_on_json = ?1 WHERE id = ?2",
                    rusqlite::params![json, phase_id],
                );
            }
        }

        // Resolve step-level dependencies (title → step ID).
        for (step_id, raw_deps) in step_deps {
            if raw_deps.is_empty() {
                continue;
            }
            let resolved: Vec<String> = raw_deps
                .iter()
                .filter_map(|title| {
                    step_title_to_id
                        .get(title)
                        .or_else(|| phase_title_to_id.get(title))
                        .cloned()
                })
                .collect();
            if !resolved.is_empty() {
                let json = serde_json::to_string(&resolved).unwrap_or_else(|_| "[]".to_string());
                let _ = conn.execute(
                    "UPDATE graph_steps SET depends_on_json = ?1 WHERE id = ?2",
                    rusqlite::params![json, step_id],
                );
            }
            // If none resolved (unrecognized titles), leave as "[]" so the step
            // is never blocked by phantom dependencies.
        }
    }

    Ok(())
}

/// Ask the LLM to output a JSON graph plan in a single call — no MCP, no
/// tool round-trips.  The agent is instructed to return ONLY a JSON object;
/// we extract it from the response and parse it in Rust.
///
/// This replaces the old MCP-based `spawn_graph_creator_agent` approach
/// which required 10-20 tool call round-trips and took ~8 minutes.
/// The JSON approach completes in a single LLM call (~30-60 seconds).
async fn spawn_graph_json_planner(
    provider: &Arc<dyn Provider>,
    graph_id: &str,
    source_doc_content: &str,
    config: &GraphConfig,
    project_root: &Path,
) -> GroveResult<GraphPlan> {
    let config_summary = format!(
        "platforms=[frontend={}, backend={}, desktop={}, mobile={}], \
         arch=[tech_stack={}, saas={}, multiuser={}]",
        config.platform_frontend,
        config.platform_backend,
        config.platform_desktop,
        config.platform_mobile,
        config.arch_tech_stack,
        config.arch_saas,
        config.arch_multiuser,
    );

    let instructions = format!(
        "You are a project decomposition assistant. \
         Read the specification and output an execution plan as a single JSON object.\n\n\
         RULES:\n\
         - Output ONLY valid JSON. No markdown prose, no explanation, no code blocks unless \
           you wrap the JSON in ```json ... ```.\n\
         - Do NOT call any tools or use any MCP functions.\n\
         - Decompose into 1-3 phases. Each phase has 1-3 steps.\n\
         - Prefer fewer, larger steps over many small ones. Each step should represent a \
           meaningful chunk of work (e.g. 'Implement storage layer and CLI commands' not \
           separate steps for each function). Merge related functionality.\n\
         - Keep step objectives specific and actionable (1-3 sentences).\n\
         - Use ordinal integers starting at 1.\n\
         - depends_on arrays reference phase/step titles (use [] if none).\n\n\
         JSON SCHEMA:\n\
         {{\n\
           \"phases\": [\n\
             {{\n\
               \"title\": \"<phase name>\",\n\
               \"task_objective\": \"<what this phase accomplishes>\",\n\
               \"ordinal\": 1,\n\
               \"depends_on\": [],\n\
               \"steps\": [\n\
                 {{\n\
                   \"title\": \"<step name>\",\n\
                   \"task_objective\": \"<exactly what to implement>\",\n\
                   \"ordinal\": 1,\n\
                   \"step_type\": \"code\",\n\
                   \"execution_mode\": \"auto\",\n\
                   \"depends_on\": []\n\
                 }}\n\
               ]\n\
             }}\n\
           ]\n\
         }}\n\n\
         ## Graph Configuration\n\n\
         {config_summary}\n\n\
         ## Source Specification\n\n\
         {source_doc_content}"
    );

    let request = ProviderRequest {
        objective: format!("Decompose specification into JSON execution plan for graph {graph_id}"),
        role: "graph_json_planner".to_string(),
        worktree_path: project_root.to_string_lossy().to_string(),
        instructions,
        model: None,
        allowed_tools: None, // no tools — text output only
        timeout_override: None,
        provider_session_id: None,
        log_dir: None,
        grove_session_id: None,
        input_handle_callback: None,
        mcp_config_path: None, // no MCP — eliminates all tool-call overhead
    };

    let response = provider.execute(&request)?;
    let raw = &response.summary;

    debug!(
        graph_id,
        response_len = raw.len(),
        "graph json planner returned"
    );

    let json_str = extract_json(raw).ok_or_else(|| {
        GroveError::Runtime(format!(
            "graph json planner returned no parseable JSON (len={}). First 200 chars: {}",
            raw.len(),
            &raw[..raw.len().min(200)]
        ))
    })?;

    serde_json::from_str::<GraphPlan>(json_str).map_err(|e| {
        GroveError::Runtime(format!(
            "failed to parse graph plan JSON: {e}. JSON: {}",
            &json_str[..json_str.len().min(400)]
        ))
    })
}

// ── Readiness Check ─────────────────────────────────────────────────────────

/// Result of the readiness check: either the graph is ready to proceed,
/// or it needs clarification from the user.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ReadinessResult {
    /// All required documents exist and no clarification needed.
    Ready,
    /// Missing documents detected — clarification questions generated.
    NeedsClarification {
        missing_docs: Vec<String>,
        questions: Vec<grove_graph_repo::GraphClarification>,
    },
}

/// Run a readiness check for a graph. This examines which documents are
/// required by the graph config, checks if they exist, and generates
/// clarification questions for the user if anything is missing.
///
/// Unlike `run_pre_planning_loop` (which auto-generates docs), this function
/// surfaces the gaps to the user for interactive resolution.
pub fn check_readiness(
    conn: &Connection,
    graph_id: &str,
    project_root: &Path,
) -> GroveResult<ReadinessResult> {
    let config = grove_graph_repo::get_graph_config(conn, graph_id)?;
    let required = required_docs(&config);

    if required.is_empty() {
        return Ok(ReadinessResult::Ready);
    }

    let missing = find_missing_docs(project_root, &required);

    if missing.is_empty() {
        return Ok(ReadinessResult::Ready);
    }

    // Clear any existing clarification questions and generate fresh ones.
    grove_graph_repo::clear_clarifications(conn, graph_id)?;

    let missing_labels: Vec<String> = missing.iter().map(|d| d.label.to_string()).collect();

    for doc in &missing {
        let question = format!(
            "The {} document ({}) is required but missing. \
             Would you like to provide it, or should it be auto-generated?",
            doc.label, doc.filename
        );
        grove_graph_repo::insert_clarification(conn, graph_id, &question)?;
    }

    let questions = grove_graph_repo::list_clarifications(conn, graph_id)?;

    info!(
        graph_id,
        missing_count = missing.len(),
        "readiness check: clarification needed"
    );

    Ok(ReadinessResult::NeedsClarification {
        missing_docs: missing_labels,
        questions,
    })
}

// ── Pre-Planning Loop ────────────────────────────────────────────────────────

/// Run the pre-planning loop that ensures all required foundational documents
/// exist before graph creation begins.
///
/// The loop:
/// 1. Reads the graph config to determine which docs are required.
/// 2. Checks the filesystem for each required doc.
/// 3. For each missing doc, spawns a PrePlanner agent to generate it.
/// 4. Re-verifies all docs after agent runs.
/// 5. Repeats up to [`MAX_PREPLANNING_ITERATIONS`] times.
///
/// On success, sets `parsing_status` to `'parsing'`.
/// On failure (max iterations or agent error), sets `parsing_status` to `'error'`.
pub async fn run_pre_planning_loop(
    conn: &Connection,
    graph_id: &str,
    project_root: &Path,
    db_path: &Path,
    provider: &Arc<dyn Provider>,
) -> GroveResult<()> {
    // ── 1. Read graph config ─────────────────────────────────────────────────
    let config = grove_graph_repo::get_graph_config(conn, graph_id)?;
    let required = required_docs(&config);

    if required.is_empty() {
        info!(
            graph_id,
            "no foundational docs required — skipping pre-planning"
        );
        grove_graph_repo::set_graph_parsing_status(conn, graph_id, "parsing")?;
        return Ok(());
    }

    // ── 2. Set parsing_status to 'planning' ──────────────────────────────────
    grove_graph_repo::set_graph_parsing_status(conn, graph_id, "planning")?;

    // ── 3. Get source document content for agent context ─────────────────────
    let graph = grove_graph_repo::get_graph(conn, graph_id)?;
    let source_doc_content = match &graph.source_document_path {
        Some(path) => {
            let full_path = project_root.join(path);
            if full_path.exists() {
                std::fs::read_to_string(&full_path).unwrap_or_default()
            } else {
                debug!(graph_id, path = %full_path.display(), "source document not found on disk");
                String::new()
            }
        }
        None => {
            debug!(
                graph_id,
                "no source_document_path set — using description as context"
            );
            graph.description.clone().unwrap_or_default()
        }
    };

    // ── 4. Pre-planning loop ─────────────────────────────────────────────────
    for iteration in 1..=MAX_PREPLANNING_ITERATIONS {
        let missing = find_missing_docs(project_root, &required);

        if missing.is_empty() {
            info!(
                graph_id,
                iteration, "all required docs present — pre-planning complete"
            );
            grove_graph_repo::set_graph_parsing_status(conn, graph_id, "parsing")?;
            return Ok(());
        }

        debug!(
            graph_id,
            iteration,
            missing_count = missing.len(),
            "pre-planning iteration — spawning agents for missing docs"
        );

        // Spawn a PrePlanner agent for each missing doc.
        for doc in &missing {
            info!(
                graph_id,
                iteration,
                doc_label = doc.label,
                doc_filename = doc.filename,
                "spawning preplanner agent for missing doc"
            );

            if let Err(e) = spawn_preplanner_agent(
                provider,
                graph_id,
                doc.label,
                doc.filename,
                &source_doc_content,
                project_root,
                db_path,
            )
            .await
            {
                warn!(
                    graph_id,
                    iteration,
                    doc_label = doc.label,
                    error = %e,
                    "preplanner agent failed"
                );
                grove_graph_repo::set_graph_parsing_status(conn, graph_id, "error")?;
                return Err(e);
            }
        }

        // Re-verify after agent runs (next loop iteration checks).
    }

    // ── 5. Max iterations exhausted ──────────────────────────────────────────
    let still_missing = find_missing_docs(project_root, &required);
    if still_missing.is_empty() {
        info!(graph_id, "all required docs present after final check");
        grove_graph_repo::set_graph_parsing_status(conn, graph_id, "parsing")?;
        return Ok(());
    }

    let missing_labels: Vec<&str> = still_missing.iter().map(|d| d.label).collect();
    warn!(
        graph_id,
        iterations = MAX_PREPLANNING_ITERATIONS,
        still_missing = ?missing_labels,
        "pre-planning loop exhausted — required docs still missing"
    );
    grove_graph_repo::set_graph_parsing_status(conn, graph_id, "error")?;
    Err(GroveError::Runtime(format!(
        "pre-planning failed after {} iterations: missing docs: {}",
        MAX_PREPLANNING_ITERATIONS,
        missing_labels.join(", ")
    )))
}

// ── Graph Creation Flow ──────────────────────────────────────────────────────

/// Run the graph creation flow that spawns the GraphCreator agent to parse the
/// source document and generate the full phase/step DAG.
///
/// Retries up to [`MAX_CREATION_RETRIES`] times (bounded by the graph's
/// `max_reruns` field). On success, sets `parsing_status` to `'complete'`.
/// On failure after all retries, sets `parsing_status` to `'error'`.
pub async fn run_graph_creation(
    conn: &Connection,
    graph_id: &str,
    project_root: &Path,
    _db_path: &Path, // no longer used: JSON planner needs no MCP
    provider: &Arc<dyn Provider>,
) -> GroveResult<()> {
    // ── 1. Load graph and config ─────────────────────────────────────────────
    let graph = grove_graph_repo::get_graph(conn, graph_id)?;
    let config = grove_graph_repo::get_graph_config(conn, graph_id)?;

    // Read source document content.
    let source_doc_content = match &graph.source_document_path {
        Some(path) => {
            let full_path = project_root.join(path);
            if full_path.exists() {
                std::fs::read_to_string(&full_path).unwrap_or_default()
            } else {
                debug!(graph_id, path = %full_path.display(), "source document not found");
                String::new()
            }
        }
        None => {
            debug!(graph_id, "no source_document_path — using description");
            graph.description.clone().unwrap_or_default()
        }
    };

    // ── 2. Set parsing_status to 'parsing' ───────────────────────────────────
    grove_graph_repo::set_graph_parsing_status(conn, graph_id, "parsing")?;

    // ── 3. Determine retry limit ─────────────────────────────────────────────
    let max_retries = std::cmp::min(graph.max_reruns as u32, MAX_CREATION_RETRIES);

    // ── 4. Creation loop with retries ────────────────────────────────────────
    for attempt in 0..=max_retries {
        debug!(graph_id, attempt, max_retries, "graph creation attempt");

        // Always clear any phases/steps from a previous (failed) attempt before
        // inserting a fresh plan. This prevents partial data from polluting retries.
        if let Err(e) = grove_graph_repo::clear_graph_plan(conn, graph_id) {
            warn!(graph_id, attempt, error = %e, "failed to clear graph plan before attempt");
        }

        // Ask the LLM for a single JSON blob, then insert phases/steps in Rust.
        // This replaces the old MCP-based approach (10-20 tool round-trips, ~8 min)
        // with a single LLM call (~30-60 s) + microsecond DB inserts.
        let plan_result = spawn_graph_json_planner(
            provider,
            graph_id,
            &source_doc_content,
            &config,
            project_root,
        )
        .await;

        match plan_result.and_then(|plan| insert_plan_from_json(conn, graph_id, &plan)) {
            Ok(()) => {
                info!(graph_id, attempt, "graph creation succeeded");
                grove_graph_repo::set_graph_parsing_status(conn, graph_id, "complete")?;
                return Ok(());
            }
            Err(e) => {
                warn!(
                    graph_id,
                    attempt,
                    max_retries,
                    error = %e,
                    "graph creator agent failed"
                );

                // Check if we can retry.
                if attempt < max_retries {
                    let can_retry = grove_graph_repo::can_rerun(conn, graph_id)?;
                    if can_retry {
                        let new_count = grove_graph_repo::increment_rerun_count(conn, graph_id)?;
                        debug!(
                            graph_id,
                            new_rerun_count = new_count,
                            "incremented rerun count — retrying graph creation"
                        );
                        continue;
                    }
                    warn!(graph_id, "rerun limit reached in DB — cannot retry");
                }

                // Final failure.
                grove_graph_repo::set_graph_parsing_status(conn, graph_id, "error")?;
                return Err(e);
            }
        }
    }

    // Should not reach here (loop covers all attempts), but handle defensively.
    grove_graph_repo::set_graph_parsing_status(conn, graph_id, "error")?;
    Err(GroveError::Runtime(
        "graph creation exhausted all retries".into(),
    ))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::MockProvider;

    fn mock_provider() -> Arc<dyn Provider> {
        Arc::new(MockProvider)
    }

    #[test]
    fn required_docs_empty_when_no_flags() {
        let config = GraphConfig::default();
        assert!(required_docs(&config).is_empty());
    }

    #[test]
    fn required_docs_includes_prd_when_flagged() {
        let config = GraphConfig {
            doc_prd: true,
            ..Default::default()
        };
        let docs = required_docs(&config);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].label, "PRD");
        assert_eq!(docs[0].filename, "PRD.md");
    }

    #[test]
    fn required_docs_includes_all_when_all_flagged() {
        let config = GraphConfig {
            doc_prd: true,
            doc_system_design: true,
            doc_guidelines: true,
            ..Default::default()
        };
        let docs = required_docs(&config);
        assert_eq!(docs.len(), 3);
    }

    #[test]
    fn find_missing_docs_returns_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = GraphConfig {
            doc_prd: true,
            doc_system_design: true,
            ..Default::default()
        };
        let docs = required_docs(&config);

        // Neither doc exists.
        let missing = find_missing_docs(tmp.path(), &docs);
        assert_eq!(missing.len(), 2);

        // Create PRD.md.
        std::fs::write(tmp.path().join("PRD.md"), "# PRD").unwrap();
        let missing = find_missing_docs(tmp.path(), &docs);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].label, "System Design");
    }

    #[test]
    fn find_missing_docs_returns_empty_when_all_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config = GraphConfig {
            doc_prd: true,
            ..Default::default()
        };
        let docs = required_docs(&config);

        std::fs::write(tmp.path().join("PRD.md"), "# PRD").unwrap();
        let missing = find_missing_docs(tmp.path(), &docs);
        assert!(missing.is_empty());
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

    #[tokio::test]
    async fn run_pre_planning_loop_skips_when_no_docs_required() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let graph_id = grove_graph_repo::insert_graph(&conn, "conv1", "G", "desc", None).unwrap();

        // Config with no doc flags — all default to false.
        let config = GraphConfig::default();
        grove_graph_repo::set_graph_config(&conn, &graph_id, &config).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        let result = run_pre_planning_loop(&conn, &graph_id, tmp.path(), &db_path, &provider).await;
        assert!(result.is_ok());

        // parsing_status should be 'parsing' (skipped planning).
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph.parsing_status, "parsing");
    }

    #[tokio::test]
    async fn run_pre_planning_loop_succeeds_when_docs_already_exist() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let graph_id = grove_graph_repo::insert_graph(&conn, "conv1", "G", "desc", None).unwrap();

        let config = GraphConfig {
            doc_prd: true,
            ..Default::default()
        };
        grove_graph_repo::set_graph_config(&conn, &graph_id, &config).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("PRD.md"), "# PRD content").unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        let result = run_pre_planning_loop(&conn, &graph_id, tmp.path(), &db_path, &provider).await;
        assert!(result.is_ok());

        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph.parsing_status, "parsing");
    }

    #[tokio::test]
    async fn run_pre_planning_loop_calls_provider_for_missing_docs() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let graph_id = grove_graph_repo::insert_graph(&conn, "conv1", "G", "desc", None).unwrap();

        let config = GraphConfig {
            doc_prd: true,
            ..Default::default()
        };
        grove_graph_repo::set_graph_config(&conn, &graph_id, &config).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        // PRD.md does NOT exist — provider will be called.
        // MockProvider succeeds but doesn't create the file, so the loop
        // eventually exhausts iterations.
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        let result = run_pre_planning_loop(&conn, &graph_id, tmp.path(), &db_path, &provider).await;
        // MockProvider doesn't create the file, so after MAX_PREPLANNING_ITERATIONS
        // the loop fails because the doc is still missing.
        assert!(result.is_err());

        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph.parsing_status, "error");
    }

    #[tokio::test]
    async fn run_graph_creation_calls_provider() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let graph_id = grove_graph_repo::insert_graph(&conn, "conv1", "G", "desc", None).unwrap();

        let config = GraphConfig::default();
        grove_graph_repo::set_graph_config(&conn, &graph_id, &config).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let provider = mock_provider();

        // MockProvider returns a non-JSON summary, so the JSON planner
        // cannot parse a valid graph plan. After exhausting retries,
        // graph creation fails and sets parsing_status to 'error'.
        let result = run_graph_creation(&conn, &graph_id, tmp.path(), &db_path, &provider).await;
        assert!(result.is_err());

        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert_eq!(graph.parsing_status, "error");
    }

    #[test]
    fn max_preplanning_iterations_is_ten() {
        assert_eq!(MAX_PREPLANNING_ITERATIONS, 10);
    }

    #[test]
    fn max_creation_retries_is_three() {
        assert_eq!(MAX_CREATION_RETRIES, 3);
    }
}
