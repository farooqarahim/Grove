use std::collections::HashMap;

use crate::agents::AgentType;
use crate::config::agent_config::PipelineConfig;

use super::{AgentPlan, pipeline::PipelineKind};

#[derive(Debug, Clone)]
pub struct RunIntent {
    pub label: String,
    pub rationale: String,
    pub execution_bundle: String,
    pub plan: AgentPlan,
    pub phase_gates: Vec<AgentType>,
    pub execution_checklist: Vec<String>,
    pub shared_context: String,
    pub agent_briefs: HashMap<AgentType, String>,
}

pub fn select_run_intent(
    objective: &str,
    pipeline_override: Option<PipelineKind>,
    _pipeline_configs: Option<&HashMap<String, PipelineConfig>>,
) -> RunIntent {
    let normalized = objective.to_ascii_lowercase();

    if is_bugfix(&normalized) {
        return fix_validate_judge_intent(objective, pipeline_override.is_some());
    }

    build_validate_judge_intent(objective, pipeline_override.is_some())
}

pub fn resume_run_intent(objective: &str, saved_label: Option<&str>) -> RunIntent {
    match saved_label {
        Some("bugfix") | Some("fix_validate_judge") => fix_validate_judge_intent(objective, false),
        Some("simple") | Some("build") | Some("build_validate_judge") => {
            build_validate_judge_intent(objective, false)
        }
        _ => select_run_intent(objective, None, None),
    }
}

fn build_validate_judge_intent(objective: &str, had_override: bool) -> RunIntent {
    let rationale = if had_override {
        "classic run uses the simplified builder-validator-judge flow".to_string()
    } else {
        "standard execution uses a lean builder-validator-judge flow".to_string()
    };
    let checklist = build_execution_checklist(objective, false);
    RunIntent {
        label: "build_validate_judge".to_string(),
        rationale,
        execution_bundle: "Builder + Validator".to_string(),
        plan: vec![vec![AgentType::Builder], vec![AgentType::Judge]],
        phase_gates: vec![],
        execution_checklist: checklist.clone(),
        shared_context: format!(
            "Run objective:\n{objective}\n\n\
             Execution bundle:\nBuilder + Validator\n\n\
             Micro plan:\n{}\n\n\
             Execution checklist:\n{}\n\n\
             Workflow contract:\n\
             1. Builder + Validator implements the requested change and validates the work against the checklist in the same pass.\n\
             2. Judge makes the final ship/no-ship decision against that same checklist.\n\
             Keep the plan lightweight and act immediately; do not create PRD or system design phases."
            ,
            format_numbered_list(&checklist),
            format_bulleted_list(&checklist)
        ),
        agent_briefs: HashMap::from([
            (
                AgentType::Builder,
                "You are the Builder + Validator bundle. Execute the checklist items directly, keep the scope tight, then validate your own work against every checklist item before you stop. Report exactly what you changed, how you verified it, and any remaining risk. Do not expand the plan beyond the checklist unless strictly necessary to complete the objective.".to_string(),
            ),
            (
                AgentType::Judge,
                "You are the judge agent. Confirm whether the Builder + Validator bundle actually satisfied the checklist, whether the verification is credible, and whether any remaining risk blocks acceptance. Approve only when the scoped objective is done.".to_string(),
            ),
        ]),
    }
}

fn fix_validate_judge_intent(objective: &str, had_override: bool) -> RunIntent {
    let rationale = if had_override {
        "classic run uses the simplified fixer-validator-judge flow".to_string()
    } else {
        "objective looks like a bug or error fix, so run starts with a fixer pass".to_string()
    };
    let checklist = build_execution_checklist(objective, true);
    RunIntent {
        label: "bugfix".to_string(),
        rationale,
        execution_bundle: "Fixer + Validator".to_string(),
        plan: vec![vec![AgentType::Builder], vec![AgentType::Judge]],
        phase_gates: vec![],
        execution_checklist: checklist.clone(),
        shared_context: format!(
            "Run objective:\n{objective}\n\n\
             Execution bundle:\nFixer + Validator\n\n\
             Micro plan:\n{}\n\n\
             Execution checklist:\n{}\n\n\
             Workflow contract:\n\
             1. Fixer + Validator identifies the concrete failure, repairs it, and validates the repair against the checklist in the same pass.\n\
             2. Judge decides whether the fix is complete and safe.\n\
             Keep the loop practical and focused; do not create PRD or system design documents."
            ,
            format_numbered_list(&checklist),
            format_bulleted_list(&checklist)
        ),
        agent_briefs: HashMap::from([
            (
                AgentType::Builder,
                "You are the Fixer + Validator bundle. Work through the checklist in order: identify the concrete failure, make the smallest effective repair, and validate the repair against every checklist item before you stop. Report exactly what was broken, what changed, how you verified it, and any remaining risk.".to_string(),
            ),
            (
                AgentType::Judge,
                "You are the judge agent. Decide whether the Fixer + Validator bundle satisfied the checklist completely, whether the verification is credible, and whether anything still blocks acceptance.".to_string(),
            ),
        ]),
    }
}

fn build_execution_checklist(objective: &str, bugfix_mode: bool) -> Vec<String> {
    let mut items = extract_objective_work_items(objective);
    if bugfix_mode {
        items.insert(
            0,
            "Identify the concrete failure mode, error, or broken behavior.".to_string(),
        );
        items.push(
            "Verify the original failure is resolved with a bounded command or direct inspection."
                .to_string(),
        );
    } else {
        items.insert(
            0,
            "Implement the requested scoped change directly without expanding requirements."
                .to_string(),
        );
        items.push(
            "Verify the requested change works and that no obvious regression was introduced."
                .to_string(),
        );
    }
    dedupe_and_limit(items, 4)
}

fn extract_objective_work_items(objective: &str) -> Vec<String> {
    let normalized = objective
        .replace('\n', " ")
        .split(['.', ';', ','])
        .flat_map(|segment| segment.split(" and "))
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let cleaned = segment
                .trim_start_matches("please ")
                .trim_start_matches("help me ")
                .trim_start_matches("can you ")
                .trim();
            let mut sentence = cleaned.to_string();
            if !sentence.ends_with('.') {
                sentence.push('.');
            }
            capitalize_first(&sentence)
        })
        .collect::<Vec<_>>();

    if normalized.is_empty() {
        vec!["Complete the stated objective within the current codebase scope.".to_string()]
    } else {
        normalized
    }
}

fn dedupe_and_limit(items: Vec<String>, limit: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let key = item.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(item);
        }
        if out.len() >= limit {
            break;
        }
    }
    out
}

fn format_numbered_list(items: &[String]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| format!("{}. {}", idx + 1, item))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_bulleted_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {}", item))
        .collect::<Vec<_>>()
        .join("\n")
}

fn capitalize_first(input: &str) -> String {
    let mut chars = input.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn is_bugfix(normalized: &str) -> bool {
    let strong_bug_terms = [
        "bug",
        "error",
        "failing test",
        "failure",
        "broken",
        "regression",
        "compile",
        "panic",
        "exception",
        "crash",
        "stack trace",
        "issue",
        "hotfix",
    ];
    contains_any(normalized, &strong_bug_terms)
        || (normalized.contains("fix")
            && contains_any(
                normalized,
                &[
                    "test",
                    "build",
                    "compile",
                    "runtime",
                    "crash",
                    "panic",
                    "exception",
                    "bug",
                    "regression",
                ],
            ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_still_uses_simple_build_flow() {
        let intent = select_run_intent("Implement login", Some(PipelineKind::Autonomous), None);
        assert_eq!(intent.label, "build_validate_judge");
        assert_eq!(intent.plan.len(), 2);
        assert_eq!(intent.plan[0], vec![AgentType::Builder]);
    }

    #[test]
    fn review_objective_still_flows_through_builder_bundle() {
        let intent = select_run_intent(
            "Review the current auth flow and summarize the scope",
            None,
            None,
        );
        assert_eq!(intent.label, "build_validate_judge");
        assert_eq!(
            intent.plan,
            vec![vec![AgentType::Builder], vec![AgentType::Judge]]
        );
    }

    #[test]
    fn bugfix_objective_uses_fix_validate_judge() {
        let intent = select_run_intent("Fix the failing test and runtime error", None, None);
        assert_eq!(intent.label, "bugfix");
        assert_eq!(
            intent.plan,
            vec![vec![AgentType::Builder], vec![AgentType::Judge]]
        );
        assert!(
            intent
                .agent_briefs
                .get(&AgentType::Builder)
                .is_some_and(|brief| brief.contains("Fixer + Validator"))
        );
        assert!(!intent.execution_checklist.is_empty());
    }

    #[test]
    fn general_build_objective_uses_build_validate_judge() {
        let intent = select_run_intent("Implement a new settings screen", None, None);
        assert_eq!(intent.label, "build_validate_judge");
        assert_eq!(
            intent.plan,
            vec![vec![AgentType::Builder], vec![AgentType::Judge]]
        );
        assert!(intent.shared_context.contains("Builder + Validator"));
    }

    #[test]
    fn resume_recovers_saved_bugfix_label() {
        let intent = resume_run_intent("Fix compile failure", Some("bugfix"));
        assert_eq!(intent.label, "bugfix");
        assert_eq!(intent.plan.len(), 2);
        assert!(intent.shared_context.contains("Fixer + Validator"));
    }

    #[test]
    fn checklist_extracts_multiple_work_items() {
        let intent = select_run_intent(
            "Rename the API client and update the failing import path",
            None,
            None,
        );
        assert!(intent.execution_checklist.len() >= 3);
        assert!(intent.shared_context.contains("Execution checklist"));
    }
}
