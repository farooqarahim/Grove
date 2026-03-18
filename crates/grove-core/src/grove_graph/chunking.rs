//! DAG-aware chunking for graph step execution.
//!
//! Classifies phases by dependency complexity and creates ordered chunks
//! of steps for worker agents to execute.

use crate::db::repositories::grove_graph_repo::{self, GraphStepRow};
use crate::errors::GroveResult;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet, VecDeque};

/// Maximum number of steps per chunk (configurable default).
pub const DEFAULT_MAX_CHUNK_SIZE: usize = 5;

/// Classification of a phase's dependency structure.
#[derive(Debug, Clone, PartialEq)]
pub enum PhaseComplexity {
    /// No intra-step dependencies — all steps are independent.
    Simple,
    /// Linear chain dependencies (A→B→C).
    Linear,
    /// DAG with branches and/or merges — needs orchestrator reasoning.
    Complex,
}

/// An ordered chunk of step IDs ready for worker execution.
#[derive(Debug, Clone)]
pub struct StepChunk {
    pub step_ids: Vec<String>,
    pub steps: Vec<GraphStepRow>,
}

/// Classify the dependency structure of a phase's open steps.
pub fn classify_phase(steps: &[GraphStepRow]) -> PhaseComplexity {
    let open_steps: Vec<&GraphStepRow> = steps.iter().filter(|s| s.status == "open").collect();

    if open_steps.is_empty() {
        return PhaseComplexity::Simple;
    }

    // Parse all dependency lists.
    let mut has_deps = false;
    let mut max_dep_count = 0usize;
    let step_ids: HashSet<&str> = open_steps.iter().map(|s| s.id.as_str()).collect();

    // Count how many open steps depend on each step.
    let mut dependent_count: HashMap<String, usize> = HashMap::new();

    for step in &open_steps {
        let deps = parse_depends_on(&step.depends_on_json);
        // Only count deps within the open set.
        let internal_count = deps
            .iter()
            .filter(|d| step_ids.contains(d.as_str()))
            .count();
        if internal_count > 0 {
            has_deps = true;
            max_dep_count = max_dep_count.max(internal_count);
            for dep in deps.iter().filter(|d| step_ids.contains(d.as_str())) {
                *dependent_count.entry(dep.clone()).or_insert(0) += 1;
            }
        }
    }

    if !has_deps {
        return PhaseComplexity::Simple;
    }

    let max_dependents = dependent_count.values().copied().max().unwrap_or(0);

    // Linear: every step has at most 1 dep and at most 1 dependent.
    if max_dep_count <= 1 && max_dependents <= 1 {
        PhaseComplexity::Linear
    } else {
        PhaseComplexity::Complex
    }
}

/// Create chunks for a simple or linear phase using topological sort.
///
/// For complex phases, returns `None` — caller should use orchestrator agent.
pub fn create_chunks(
    conn: &Connection,
    phase_id: &str,
    max_chunk_size: usize,
) -> GroveResult<Option<Vec<StepChunk>>> {
    let all_steps = grove_graph_repo::list_steps(conn, phase_id)?;
    let open_steps: Vec<GraphStepRow> = all_steps
        .into_iter()
        .filter(|s| s.status == "open")
        .collect();

    if open_steps.is_empty() {
        return Ok(Some(vec![]));
    }

    let complexity = classify_phase(&open_steps);
    if complexity == PhaseComplexity::Complex {
        return Ok(None); // Caller must use orchestrator.
    }

    let sorted = topological_sort(&open_steps)?;
    let chunks = sorted
        .chunks(max_chunk_size)
        .map(|chunk| StepChunk {
            step_ids: chunk.iter().map(|s| s.id.clone()).collect(),
            steps: chunk.to_vec(),
        })
        .collect();

    Ok(Some(chunks))
}

/// Topological sort of steps respecting dependency order.
///
/// Steps with no dependencies come first. Among steps at the same
/// depth, ordering is by ordinal.
pub fn topological_sort(steps: &[GraphStepRow]) -> GroveResult<Vec<GraphStepRow>> {
    let step_map: HashMap<&str, &GraphStepRow> = steps.iter().map(|s| (s.id.as_str(), s)).collect();
    let step_ids: HashSet<&str> = step_map.keys().copied().collect();

    // Build adjacency: step_id → set of steps it depends on (within this set).
    let mut in_deps: HashMap<&str, HashSet<String>> = HashMap::new();
    for step in steps {
        let deps = parse_depends_on(&step.depends_on_json);
        let internal: HashSet<String> = deps
            .into_iter()
            .filter(|d| step_ids.contains(d.as_str()))
            .collect();
        in_deps.insert(step.id.as_str(), internal);
    }

    let mut result = Vec::with_capacity(steps.len());
    let mut ready: VecDeque<&str> = VecDeque::new();

    // Find roots (no internal deps).
    let mut roots: Vec<&str> = in_deps
        .iter()
        .filter(|(_, deps)| deps.is_empty())
        .map(|(&id, _)| id)
        .collect();
    roots.sort_by_key(|id| step_map[id].ordinal);
    ready.extend(roots);

    let mut visited = HashSet::new();
    while let Some(id) = ready.pop_front() {
        if !visited.insert(id) {
            continue;
        }
        result.push(step_map[id].clone());

        // Find steps whose deps are now all visited.
        let mut newly_ready: Vec<&str> = Vec::new();
        for (&sid, deps) in &in_deps {
            if visited.contains(sid) {
                continue;
            }
            if deps.iter().all(|d| visited.contains(d.as_str())) {
                newly_ready.push(sid);
            }
        }
        newly_ready.sort_by_key(|id| step_map[id].ordinal);
        ready.extend(newly_ready);
    }

    if result.len() != steps.len() {
        return Err(crate::errors::GroveError::Runtime(
            "circular dependency detected in step DAG".into(),
        ));
    }

    Ok(result)
}

/// Parse depends_on_json (a String field, not Option) into a list of step IDs.
pub fn parse_depends_on(json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(json).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_step(id: &str, ordinal: i64, deps: Option<&str>) -> GraphStepRow {
        GraphStepRow {
            id: id.to_string(),
            phase_id: "phase1".into(),
            graph_id: "graph1".into(),
            task_name: format!("Step {id}"),
            task_objective: format!("Do {id}"),
            step_type: "code".into(),
            outcome: None,
            ai_comments: None,
            grade: None,
            reference_doc_path: None,
            ref_required: false,
            status: "open".into(),
            ordinal,
            execution_mode: "auto".into(),
            depends_on_json: deps.unwrap_or("[]").to_string(),
            run_iteration: 0,
            max_iterations: 3,
            judge_feedback_json: "[]".to_string(),
            builder_run_id: None,
            verdict_run_id: None,
            judge_run_id: None,
            conversation_id: None,
            created_run_id: None,
            executed_run_id: None,
            execution_agent: None,
            created_at: "".into(),
            updated_at: "".into(),
        }
    }

    #[test]
    fn test_classify_simple() {
        let steps = vec![
            make_step("s1", 1, None),
            make_step("s2", 2, None),
            make_step("s3", 3, None),
        ];
        assert_eq!(classify_phase(&steps), PhaseComplexity::Simple);
    }

    #[test]
    fn test_classify_linear() {
        let steps = vec![
            make_step("s1", 1, None),
            make_step("s2", 2, Some(r#"["s1"]"#)),
            make_step("s3", 3, Some(r#"["s2"]"#)),
        ];
        assert_eq!(classify_phase(&steps), PhaseComplexity::Linear);
    }

    #[test]
    fn test_classify_complex() {
        // s3 depends on both s1 and s2 (merge point) = complex
        let steps = vec![
            make_step("s1", 1, None),
            make_step("s2", 2, None),
            make_step("s3", 3, Some(r#"["s1","s2"]"#)),
        ];
        assert_eq!(classify_phase(&steps), PhaseComplexity::Complex);
    }

    #[test]
    fn test_topological_sort_simple() {
        let steps = vec![
            make_step("s3", 3, None),
            make_step("s1", 1, None),
            make_step("s2", 2, None),
        ];
        let sorted = topological_sort(&steps).unwrap();
        // Should be ordered by ordinal since no deps.
        assert_eq!(sorted[0].id, "s1");
        assert_eq!(sorted[1].id, "s2");
        assert_eq!(sorted[2].id, "s3");
    }

    #[test]
    fn test_topological_sort_linear() {
        let steps = vec![
            make_step("s2", 2, Some(r#"["s1"]"#)),
            make_step("s1", 1, None),
            make_step("s3", 3, Some(r#"["s2"]"#)),
        ];
        let sorted = topological_sort(&steps).unwrap();
        assert_eq!(sorted[0].id, "s1");
        assert_eq!(sorted[1].id, "s2");
        assert_eq!(sorted[2].id, "s3");
    }

    #[test]
    fn test_topological_sort_circular_dependency() {
        let steps = vec![
            make_step("s1", 1, Some(r#"["s2"]"#)),
            make_step("s2", 2, Some(r#"["s1"]"#)),
        ];
        assert!(topological_sort(&steps).is_err());
    }

    #[test]
    fn test_parse_depends_on_empty() {
        assert!(parse_depends_on("[]").is_empty());
        assert!(parse_depends_on("").is_empty());
    }

    #[test]
    fn test_parse_depends_on_valid() {
        assert_eq!(parse_depends_on(r#"["s1","s2"]"#), vec!["s1", "s2"]);
    }
}
