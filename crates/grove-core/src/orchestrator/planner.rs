/// Plan file parsing for structured GROVE_PLAN files.
///
/// The old AI-based agent planner (31 agents, parallel stages) has been replaced
/// by `PipelineKind::agents()` which returns the fixed agent sequence for each
/// pipeline mode (Plan, Build, Autonomous).
///
/// Markdown pipeline configs (`skills/pipelines/*.md`) are now the primary source
/// of truth via `plan_from_pipeline_config` / `gates_from_pipeline_config`. The
/// hardcoded `PipelineKind` enum serves as a fallback when no Markdown config
/// matches the requested pipeline ID.
///
/// This module retains `read_grove_plan()` for reading structured plan JSON files
/// produced by the PlanSystemDesign agent, which can be used for sub-task tracking.
use std::collections::HashMap;
use std::path::Path;

use crate::agents::AgentType;
use crate::config::agent_config::PipelineConfig;

use super::{AgentPlan, GrovePlanFile, GrovePlanStep};

/// Return the default sequential plan for the given pipeline.
///
/// This replaces the old AI planner and hardcoded_plan functions.
/// Each agent runs in its own sequential stage (no parallel execution).
pub fn plan_from_pipeline(pipeline: &super::pipeline::PipelineKind) -> AgentPlan {
    pipeline.agents().into_iter().map(|a| vec![a]).collect()
}

/// Build a plan from Markdown pipeline config, falling back to the enum.
///
/// Resolution order:
/// 1. Direct ID match in `configs` (e.g. `"build"` → `configs["build"]`)
/// 2. Alias match (e.g. `"instant"` → config with `aliases: ["instant"]`)
/// 3. Fallback: parse `pipeline_id` via `PipelineKind::from_str` enum
pub fn plan_from_pipeline_config(
    pipeline_id: &str,
    configs: Option<&HashMap<String, PipelineConfig>>,
) -> AgentPlan {
    if let Some(configs) = configs {
        // Direct ID match
        if let Some(config) = configs.get(pipeline_id) {
            if let Some(plan) = config_to_plan(config) {
                return plan;
            }
        }
        // Alias match
        for config in configs.values() {
            if config.aliases.iter().any(|a| a == pipeline_id) {
                if let Some(plan) = config_to_plan(config) {
                    return plan;
                }
            }
        }
    }
    // Fallback: parse pipeline_id via enum
    let kind = super::pipeline::PipelineKind::from_str(pipeline_id).unwrap_or_default();
    plan_from_pipeline(&kind)
}

/// Extract gates from Markdown pipeline config, falling back to the enum.
///
/// Same resolution order as `plan_from_pipeline_config`.
pub fn gates_from_pipeline_config(
    pipeline_id: &str,
    configs: Option<&HashMap<String, PipelineConfig>>,
) -> Vec<AgentType> {
    if let Some(configs) = configs {
        // Direct ID match
        if let Some(config) = configs.get(pipeline_id) {
            let gates: Vec<AgentType> = config
                .gates
                .iter()
                .filter_map(|g| AgentType::from_str(g))
                .collect();
            // Return if all gate strings resolved, or if the config has no gates
            if gates.len() == config.gates.len() {
                return gates;
            }
        }
        // Alias match
        for config in configs.values() {
            if config.aliases.iter().any(|a| a == pipeline_id) {
                let gates: Vec<AgentType> = config
                    .gates
                    .iter()
                    .filter_map(|g| AgentType::from_str(g))
                    .collect();
                if gates.len() == config.gates.len() {
                    return gates;
                }
            }
        }
    }
    // Fallback: parse pipeline_id via enum
    let kind = super::pipeline::PipelineKind::from_str(pipeline_id).unwrap_or_default();
    kind.gates()
}

/// Convert a `PipelineConfig` to an `AgentPlan`.
///
/// Returns `None` if any agent ID in the config cannot be resolved to an
/// `AgentType` — the caller should fall back to the enum in that case.
fn config_to_plan(config: &PipelineConfig) -> Option<AgentPlan> {
    let agents: Vec<AgentType> = config
        .agents
        .iter()
        .filter_map(|id| AgentType::from_str(id))
        .collect();
    if agents.len() != config.agents.len() {
        return None; // Some agent IDs were invalid — fall back
    }
    Some(agents.into_iter().map(|a| vec![a]).collect())
}

/// Read and parse GROVE_PLAN_{run_id}.json from the worktree.
///
/// Validates that all `depends_on` IDs reference existing step IDs.
/// Returns `None` on any failure (missing file, invalid JSON, bad refs).
pub fn read_grove_plan(worktree: &Path, run_id: &str) -> Option<Vec<GrovePlanStep>> {
    let short_id = if run_id.len() >= 8 {
        &run_id[..8]
    } else {
        run_id
    };

    // Try new naming convention first, fall back to legacy.
    let new_path = worktree.join(format!("GROVE_PLAN_{short_id}.json"));
    let legacy_path = worktree.join(format!("GROVE_PLAN_{run_id}.json"));

    let content = std::fs::read_to_string(&new_path)
        .or_else(|_| std::fs::read_to_string(&legacy_path))
        .ok()?;

    let plan: GrovePlanFile = serde_json::from_str(&content).ok()?;

    if plan.steps.is_empty() {
        return None;
    }

    // Collect all declared step IDs.
    let known_ids: std::collections::HashSet<&str> =
        plan.steps.iter().map(|s| s.id.as_str()).collect();

    for step in &plan.steps {
        // Validate agent_type is one of our 5 known types.
        if AgentType::from_str(&step.agent_type).is_none() {
            eprintln!(
                "[PLANNER] step '{}' has unknown agent_type '{}'",
                step.id, step.agent_type
            );
            return None;
        }
        // Validate all depends_on references.
        for dep in &step.depends_on {
            if !known_ids.contains(dep.as_str()) {
                eprintln!("[PLANNER] step '{}' depends_on unknown id '{dep}'", step.id);
                return None;
            }
        }
    }

    Some(plan.steps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::pipeline::PipelineKind;

    #[test]
    fn plan_from_pipeline_build_mode() {
        let plan = plan_from_pipeline(&PipelineKind::Build);
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0], vec![AgentType::Builder]);
        assert_eq!(plan[1], vec![AgentType::Reviewer]);
        assert_eq!(plan[2], vec![AgentType::Judge]);
    }

    #[test]
    fn plan_from_pipeline_autonomous_mode() {
        let plan = plan_from_pipeline(&PipelineKind::Autonomous);
        assert_eq!(plan.len(), 5);
        assert_eq!(plan[0], vec![AgentType::BuildPrd]);
        assert_eq!(plan[4], vec![AgentType::Judge]);
    }

    #[test]
    fn plan_from_pipeline_plan_mode() {
        let plan = plan_from_pipeline(&PipelineKind::Plan);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0], vec![AgentType::BuildPrd]);
        assert_eq!(plan[1], vec![AgentType::PlanSystemDesign]);
    }

    #[test]
    fn read_grove_plan_missing_file() {
        assert!(read_grove_plan(Path::new("/nonexistent"), "abc12345").is_none());
    }

    #[test]
    fn plan_from_markdown_pipeline_config() {
        let mut pipelines = HashMap::new();
        pipelines.insert(
            "build".to_string(),
            PipelineConfig {
                id: "build".into(),
                name: "Build Mode".into(),
                description: "test".into(),
                agents: vec!["builder".into(), "reviewer".into(), "judge".into()],
                gates: vec![],
                default: false,
                aliases: vec!["instant".into()],
                content: String::new(),
            },
        );

        let plan = plan_from_pipeline_config("build", Some(&pipelines));
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0], vec![AgentType::Builder]);
        assert_eq!(plan[1], vec![AgentType::Reviewer]);
        assert_eq!(plan[2], vec![AgentType::Judge]);
    }

    #[test]
    fn plan_from_pipeline_config_falls_back_to_enum() {
        let plan = plan_from_pipeline_config("build", None);
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0], vec![AgentType::Builder]);
    }

    #[test]
    fn plan_from_pipeline_config_alias_match() {
        let mut pipelines = HashMap::new();
        pipelines.insert(
            "build".to_string(),
            PipelineConfig {
                id: "build".into(),
                name: "Build Mode".into(),
                description: "test".into(),
                agents: vec!["builder".into(), "reviewer".into(), "judge".into()],
                gates: vec![],
                default: false,
                aliases: vec!["instant".into()],
                content: String::new(),
            },
        );

        // "instant" is an alias for "build"
        let plan = plan_from_pipeline_config("instant", Some(&pipelines));
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0], vec![AgentType::Builder]);
    }

    #[test]
    fn gates_from_pipeline_config_reads_markdown() {
        let mut pipelines = HashMap::new();
        pipelines.insert(
            "autonomous".to_string(),
            PipelineConfig {
                id: "autonomous".into(),
                name: "Autonomous".into(),
                description: "test".into(),
                agents: vec![
                    "build_prd".into(),
                    "plan_system_design".into(),
                    "builder".into(),
                    "reviewer".into(),
                    "judge".into(),
                ],
                gates: vec!["build_prd".into(), "plan_system_design".into()],
                default: true,
                aliases: vec![],
                content: String::new(),
            },
        );

        let gates = gates_from_pipeline_config("autonomous", Some(&pipelines));
        assert_eq!(gates.len(), 2);
        assert_eq!(gates[0], AgentType::BuildPrd);
        assert_eq!(gates[1], AgentType::PlanSystemDesign);
    }

    #[test]
    fn gates_from_pipeline_config_empty_gates() {
        let mut pipelines = HashMap::new();
        pipelines.insert(
            "build".to_string(),
            PipelineConfig {
                id: "build".into(),
                name: "Build".into(),
                description: "test".into(),
                agents: vec!["builder".into(), "reviewer".into(), "judge".into()],
                gates: vec![],
                default: false,
                aliases: vec![],
                content: String::new(),
            },
        );

        let gates = gates_from_pipeline_config("build", Some(&pipelines));
        assert!(gates.is_empty());
    }
}
