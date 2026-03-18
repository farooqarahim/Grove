use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::config::paths;
use crate::errors::GroveResult;

use super::intent::RunIntent;
use super::verdict::{self, JudgeVerdict};

pub fn write_plan_log(
    project_root: &Path,
    conversation_id: &str,
    run_id: &str,
    objective: &str,
    effective_objective: &str,
    run_intent: &RunIntent,
) -> GroveResult<PathBuf> {
    let path = paths::run_plan_log_path(project_root, conversation_id, run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let effective_objective_block = if effective_objective != objective {
        format!(
            "\n## Effective Objective\n\n{}\n",
            effective_objective.trim()
        )
    } else {
        String::new()
    };

    let body = format!(
        "# Run Plan\n\n\
         - Generated: {}\n\
         - Run ID: `{}`\n\
         - Conversation ID: `{}`\n\
         - Intent Label: `{}`\n\
         - Execution Bundle: `{}`\n\n\
         ## Objective\n\n\
         {}\n{}\
         \n## Rationale\n\n\
         {}\n\n\
         ## Micro Plan\n\n\
         {}\n\n\
         ## Checklist\n\n\
         {}\n\n\
         ## Flow\n\n\
         1. {} handles scoped implementation work and validation in the same agent turn.\n\
         2. Judge confirms the checklist is actually complete and the result is acceptable.\n",
        Utc::now().to_rfc3339(),
        run_id,
        conversation_id,
        run_intent.label,
        run_intent.execution_bundle,
        objective.trim(),
        effective_objective_block,
        run_intent.rationale.trim(),
        numbered_list(&run_intent.execution_checklist),
        bulleted_list(&run_intent.execution_checklist),
        run_intent.execution_bundle,
    );

    fs::write(&path, body)?;
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
pub fn write_verdict_log(
    project_root: &Path,
    conversation_id: &str,
    run_id: &str,
    objective: &str,
    run_intent: &RunIntent,
    final_state: &str,
    error: Option<&str>,
    report_path: Option<&Path>,
) -> GroveResult<PathBuf> {
    let path = paths::run_verdict_log_path(project_root, conversation_id, run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let artifacts_dir = paths::run_artifacts_dir(project_root, conversation_id, run_id);
    let (verdict_label, verdict_notes) = match verdict::parse_judge_verdict(&artifacts_dir, run_id)
    {
        Some(JudgeVerdict::Approved) => (
            "APPROVED",
            String::from("Judge accepted the bundled execution result."),
        ),
        Some(JudgeVerdict::NeedsWork { notes }) => ("NEEDS_WORK", notes),
        Some(JudgeVerdict::Rejected { notes }) => ("REJECTED", notes),
        None => fallback_verdict(final_state, error),
    };

    let report_block = report_path
        .map(|path| format!("- Report: `{}`\n", path.display()))
        .unwrap_or_default();
    let error_block = error
        .filter(|msg| !msg.trim().is_empty())
        .map(|msg| format!("\n## Error\n\n{}\n", msg.trim()))
        .unwrap_or_default();

    let body = format!(
        "# Run Verdict\n\n\
         - Generated: {}\n\
         - Run ID: `{}`\n\
         - Conversation ID: `{}`\n\
         - State: `{}`\n\
         - Intent Label: `{}`\n\
         - Execution Bundle: `{}`\n\
         {}\
         \n## Objective\n\n\
         {}\n\n\
         ## Final Verdict\n\n\
         - Judge Verdict: `{}`\n\n\
         {}\n\
         \n## Checklist Confirmed\n\n\
         {}\n{}",
        Utc::now().to_rfc3339(),
        run_id,
        conversation_id,
        final_state,
        run_intent.label,
        run_intent.execution_bundle,
        report_block,
        objective.trim(),
        verdict_label,
        verdict_notes.trim(),
        bulleted_list(&run_intent.execution_checklist),
        error_block,
    );

    fs::write(&path, body)?;
    // Also clean up any legacy judge artifacts that may exist in the worktree
    let worktree_path = crate::worktree::paths::conv_worktree_path(
        &paths::worktrees_dir(project_root),
        conversation_id,
    );
    cleanup_judge_artifacts(&worktree_path, run_id)?;
    cleanup_judge_artifacts(&artifacts_dir, run_id)?;
    Ok(path)
}

fn numbered_list(items: &[String]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(idx, item)| format!("{}. {}", idx + 1, item))
        .collect::<Vec<_>>()
        .join("\n")
}

fn bulleted_list(items: &[String]) -> String {
    items
        .iter()
        .map(|item| format!("- {}", item))
        .collect::<Vec<_>>()
        .join("\n")
}

fn fallback_verdict(final_state: &str, error: Option<&str>) -> (&'static str, String) {
    match final_state {
        "completed" => (
            "COMPLETED",
            "Run completed, but no explicit judge verdict file was found in the conversation worktree."
                .to_string(),
        ),
        "paused" => (
            "PAUSED",
            "Run was paused before final acceptance. Judge confirmation is still pending."
                .to_string(),
        ),
        "failed" => (
            "FAILED",
            error
                .unwrap_or("Run failed before a final judge verdict was recorded.")
                .to_string(),
        ),
        other => (
            "UNKNOWN",
            format!("Run ended in state `{other}` without a parsed judge verdict."),
        ),
    }
}

fn cleanup_judge_artifacts(worktree_path: &Path, run_id: &str) -> GroveResult<()> {
    let short_id = if run_id.len() >= 8 {
        &run_id[..8]
    } else {
        run_id
    };
    for candidate in [
        format!("GROVE_VERDICT_{short_id}.md"),
        format!("JUDGE_VERDICT_{run_id}.md"),
        "GROVE_VERDICT.md".to_string(),
        "judge_verdict.md".to_string(),
    ] {
        let path = worktree_path.join(candidate);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::agents::AgentType;

    use super::*;

    fn sample_intent() -> RunIntent {
        RunIntent {
            label: "build_validate_judge".to_string(),
            rationale: "test rationale".to_string(),
            execution_bundle: "Builder + Validator".to_string(),
            plan: vec![vec![AgentType::Builder], vec![AgentType::Judge]],
            phase_gates: vec![],
            execution_checklist: vec![
                "Implement the requested change.".to_string(),
                "Validate the requested change.".to_string(),
            ],
            shared_context: "shared".to_string(),
            agent_briefs: HashMap::new(),
        }
    }

    #[test]
    fn writes_plan_log_under_singular_log_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = write_plan_log(
            tmp.path(),
            "conv_123",
            "run_456",
            "Implement settings page",
            "Implement settings page",
            &sample_intent(),
        )
        .unwrap();
        assert!(path.ends_with(".grove/log/conv_123/plan/run_456.md"));
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("Execution Bundle: `Builder + Validator`"));
    }

    #[test]
    fn writes_verdict_log_without_root_leakage() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Write verdict artifact to the new artifacts directory
        let artifacts_dir =
            crate::config::paths::run_artifacts_dir(tmp.path(), "conv_123", "run_456");
        fs::create_dir_all(&artifacts_dir).unwrap();
        fs::write(
            artifacts_dir.join("GROVE_VERDICT_run_456.md"),
            "## VERDICT: APPROVED\n",
        )
        .unwrap();
        // Also create worktree dir so cleanup doesn't fail
        let worktree = crate::config::paths::worktrees_dir(tmp.path()).join("conv_123");
        fs::create_dir_all(&worktree).unwrap();
        let path = write_verdict_log(
            tmp.path(),
            "conv_123",
            "run_456",
            "Implement settings page",
            &sample_intent(),
            "completed",
            None,
            None,
        )
        .unwrap();
        assert!(path.ends_with(".grove/log/conv_123/verdict/run_456.md"));
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("Judge Verdict: `APPROVED`"));
        assert!(!artifacts_dir.join("GROVE_VERDICT_run_456.md").exists());
    }
}
