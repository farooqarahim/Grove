/// Agent instruction builder for the 5-agent pipeline.
///
/// Each agent gets a focused system prompt describing its role, what artifacts
/// to read, and what artifact to produce. Artifacts are written to a dedicated
/// `.grove/artifacts/{conversation_id}/{run_id}/` directory — never the worktree root.
use std::path::Path;

use crate::agents::AgentType;

/// Build the full instruction prompt for an agent in the pipeline.
///
/// - `agent` — which of the 5 agents is being prompted
/// - `objective` — the user's objective for this run
/// - `run_id` — used to generate artifact filenames (first 8 chars)
/// - `artifacts_dir` — absolute path to the artifacts directory for this run
/// - `handoff_context` — optional diff context from the previous agent
pub fn build_agent_instructions(
    agent: AgentType,
    objective: &str,
    run_id: &str,
    artifacts_dir: &Path,
    handoff_context: Option<&str>,
) -> String {
    let short_id = if run_id.len() >= 8 {
        &run_id[..8]
    } else {
        run_id
    };

    let artifacts_path = artifacts_dir.display();
    let upstream = upstream_artifacts_context(agent, short_id, artifacts_dir);
    let handoff = handoff_context.unwrap_or("");

    let role = role_prompt(agent, objective, short_id, &artifacts_path.to_string());

    let mut instructions = String::with_capacity(role.len() + upstream.len() + handoff.len() + 256);

    instructions.push_str(&role);

    if !upstream.is_empty() {
        instructions.push_str("\n\n");
        instructions.push_str(&upstream);
    }

    if !handoff.is_empty() {
        instructions.push_str("\n\n");
        instructions.push_str(handoff);
    }

    instructions
}

/// The role-specific prompt for each of the 5 agents.
fn role_prompt(agent: AgentType, objective: &str, short_id: &str, artifacts_dir: &str) -> String {
    match agent {
        AgentType::BuildPrd => format!(
            "You are the BUILD PRD agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read all existing code and documentation to understand current capabilities\n\
             2. Write `{artifacts_dir}/GROVE_PRD_{short_id}.md` — a production-grade Product Requirements Document with:\n\n\
                ## Overview\n\
                (one paragraph: what this product/feature does and why)\n\n\
                ## Goals\n\
                (bullet list: specific measurable outcomes)\n\n\
                ## Non-Goals\n\
                (what is explicitly out of scope)\n\n\
                ## User Stories\n\
                (As a [role], I want [capability] so that [value])\n\n\
                ## Acceptance Criteria\n\
                (numbered list of testable conditions that define done)\n\n\
                ## Constraints\n\
                (technical, legal, performance, compatibility constraints)\n\n\
                ## Open Questions\n\
                (unresolved decisions that need answers before implementation)\n\n\
             3. This document becomes the source of truth for all downstream agents\n\n\
             Be precise and unambiguous. Avoid vague requirements like 'should be fast'.\n\
             Do NOT write any code — only the requirements document.\n\
             Do NOT write any files to the working directory — only to the artifacts directory shown above."
        ),

        AgentType::PlanSystemDesign => format!(
            "You are the PLAN SYSTEM DESIGN agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read `{artifacts_dir}/GROVE_PRD_{short_id}.md` if it exists — it defines what to build\n\
             2. Read all existing source code to understand current patterns, naming conventions, and APIs\n\
             3. Write `{artifacts_dir}/GROVE_DESIGN_{short_id}.md` — a technical system design covering:\n\n\
                ## Architecture Overview\n\
                (module boundaries, data flow diagram in text form, key design decisions)\n\n\
                ## Data Models\n\
                (every new struct/type/schema with field names, types, constraints, and validation rules)\n\n\
                ## API Contracts\n\
                (every new public function/endpoint: inputs, outputs, error cases, side effects)\n\n\
                ## Implementation Plan\n\
                (ordered list of files to create/modify, with specific TODOs for each file)\n\n\
                ## Error Handling Strategy\n\
                (how errors propagate, what errors are returned to callers vs logged internally)\n\n\
                ## Testing Strategy\n\
                (what test types are required, which cases must be covered)\n\n\
             4. This document becomes the implementation contract for the Builder\n\n\
             Be specific. Every interface you define must be implementable without follow-up questions.\n\
             Do NOT write implementation code — only the design document.\n\
             Do NOT write any files to the working directory — only to the artifacts directory shown above."
        ),

        AgentType::Builder => format!(
            "You are the BUILDER agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read `{artifacts_dir}/GROVE_DESIGN_{short_id}.md` if it exists — it defines the architecture and TODOs\n\
             2. Read `{artifacts_dir}/GROVE_PRD_{short_id}.md` if it exists — it defines acceptance criteria\n\
             3. Implement every item in the design document — write complete, working, production-quality code:\n\
                - No stubs, no `// TODO` left behind, no placeholder returns\n\
                - Follow the existing code style and patterns exactly\n\
                - Handle all error paths explicitly\n\
             4. Run the test suite after implementation and fix any failures in source code\n\
             5. Write tests for new functionality — cover happy paths, error paths, and edge cases\n\
             6. Do NOT modify the design or PRD documents\n\n\
             Work in the current directory. The Reviewer reads your output next."
        ),

        AgentType::Reviewer => format!(
            "You are the REVIEWER agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read `{artifacts_dir}/GROVE_DESIGN_{short_id}.md` and `{artifacts_dir}/GROVE_PRD_{short_id}.md` if they exist\n\
             2. Read all source files changed since the run started\n\
             3. Evaluate the code across these dimensions:\n\
                - CORRECTNESS: does the code do what the objective asked?\n\
                - BUGS: logic errors, null handling, off-by-one, race conditions\n\
                - STYLE: consistency with existing codebase patterns\n\
                - SECURITY: obvious vulnerabilities (injection, auth, secrets)\n\
                - PERFORMANCE: N+1 queries, unnecessary loops, memory issues\n\
                - COMPLETENESS: are there gaps? Did the builder miss part of the task?\n\
             4. Fix any CRITICAL or HIGH severity issues directly in the source files\n\
             5. Run the test suite to verify changes still pass\n\
             6. Write `{artifacts_dir}/GROVE_REVIEW_{short_id}.md` with this exact structure:\n\n\
                ## Summary\n\
                (one paragraph describing the changes reviewed)\n\n\
                ## Issues Found\n\
                (list each issue as: [CRITICAL|HIGH|MEDIUM|LOW] file:line — description)\n\n\
                ## What Was Done Well\n\
                (brief note on positives)\n\n\
                ## VERDICT: PASS\n\
                or\n\
                ## VERDICT: FAIL\n\
                (if FAIL, include: \"Builder must fix: \" followed by specific instructions)\n\n\
             VERDICT rules:\n\
             - PASS if: no critical bugs, objective is met, code is production-ready\n\
             - FAIL if: critical bugs remain, objective is not met, or major anti-patterns exist\n\
             - Do NOT fail for stylistic preferences alone"
        ),

        AgentType::Judge => format!(
            "You are the JUDGE agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read ALL artifacts produced in this run:\n\
                - `{artifacts_dir}/GROVE_PRD_{short_id}.md` (requirements)\n\
                - `{artifacts_dir}/GROVE_DESIGN_{short_id}.md` (architecture)\n\
                - `{artifacts_dir}/GROVE_REVIEW_{short_id}.md` (code review)\n\
                - All source files changed during this run\n\
             2. Evaluate the overall pipeline output holistically:\n\
                - Did the Builder fully implement what the Design specified?\n\
                - Do tests prove correctness? Are they strong enough?\n\
                - Did the Reviewer find real issues vs noise?\n\
                - Are there cross-cutting concerns no single agent caught?\n\
                - Does the output meet the original objective at a high bar?\n\
             3. Run key smoke tests or spot-checks if the test suite is available\n\
             4. Write `{artifacts_dir}/GROVE_VERDICT_{short_id}.md` with:\n\n\
                ## Overall Assessment\n\
                (one paragraph summary of quality)\n\n\
                ## Agent-by-Agent Evaluation\n\
                (for each agent that produced artifacts: what they got right and wrong)\n\n\
                ## Cross-cutting Issues\n\
                (problems that span multiple agents' output)\n\n\
                ## VERDICT: APPROVED\n\
                or ## VERDICT: NEEDS_WORK\n\
                or ## VERDICT: REJECTED\n\n\
             VERDICT rules:\n\
             - APPROVED: objective met, no critical gaps, ready to merge\n\
             - NEEDS_WORK: good progress but specific rework required (list what)\n\
             - REJECTED: fundamental problems — output does not meet the objective"
        ),

        AgentType::PrePlanner => format!(
            "You are the PRE PLANNER agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read the codebase to understand existing structure and patterns\n\
             2. Generate any missing foundational documents (PRD, System Design, Guidelines)\n\
             3. Write `{artifacts_dir}/PREPLAN_{short_id}.md` containing the consolidated pre-planning output\n\
             4. Ensure all downstream agents have the context they need\n\n\
             Do NOT write implementation code — only planning documents.\n\
             Do NOT write any files to the working directory — only to the artifacts directory shown above."
        ),

        AgentType::GraphCreator => format!(
            "You are the GRAPH CREATOR agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read the pre-planning output and any existing design documents\n\
             2. Decompose the objective into phases and steps using MCP tools\n\
             3. Write `{artifacts_dir}/GRAPH_SPEC_{short_id}.json` — a structured graph specification\n\
             4. Each phase should contain ordered steps with clear dependencies\n\n\
             Do NOT write implementation code — only the graph specification.\n\
             Do NOT write any files to the working directory — only to the artifacts directory shown above."
        ),

        AgentType::Verdict => format!(
            "You are the VERDICT agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Review the Builder's output for the current step\n\
             2. Run tests, lints, and checks to validate correctness\n\
             3. Write `{artifacts_dir}/VERDICT_{short_id}.json` with pass/fail assessment\n\
             4. Do NOT modify any source files — you are read-only\n\n\
             Focus on objective correctness, not style preferences."
        ),

        AgentType::PhaseValidator => format!(
            "You are the PHASE VALIDATOR agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Review all steps completed in the current phase\n\
             2. Run integration tests to verify cross-step compatibility\n\
             3. Write `{artifacts_dir}/PHASE_VAL_{short_id}.json` with validation results\n\
             4. Do NOT modify any source files — you are read-only\n\n\
             Focus on integration correctness across steps."
        ),

        AgentType::PhaseJudge => format!(
            "You are the PHASE JUDGE agent.\n\n\
             Objective: {objective}\n\n\
             Your tasks:\n\
             1. Read all artifacts produced in the current phase\n\
             2. Evaluate the phase holistically for quality and completeness\n\
             3. Write `{artifacts_dir}/PHASE_JUDGE_{short_id}.json` with your assessment\n\
             4. Do NOT modify any files or run any commands — you are read-only\n\n\
             Grade: PASS, NEEDS_WORK, or FAIL."
        ),
    }
}

/// Build context about which upstream artifacts this agent should read.
///
/// Checks whether each artifact file exists in the artifacts directory and tells
/// the agent to read it if present.
fn upstream_artifacts_context(agent: AgentType, short_id: &str, artifacts_dir: &Path) -> String {
    let artifacts: Vec<(&str, String)> = match agent {
        AgentType::BuildPrd => {
            // BuildPrd is first — no upstream artifacts.
            vec![]
        }
        AgentType::PlanSystemDesign => {
            vec![("PRD", format!("GROVE_PRD_{short_id}.md"))]
        }
        AgentType::Builder => {
            vec![
                ("PRD", format!("GROVE_PRD_{short_id}.md")),
                ("Design", format!("GROVE_DESIGN_{short_id}.md")),
            ]
        }
        AgentType::Reviewer => {
            vec![
                ("PRD", format!("GROVE_PRD_{short_id}.md")),
                ("Design", format!("GROVE_DESIGN_{short_id}.md")),
            ]
        }
        AgentType::Judge => {
            vec![
                ("PRD", format!("GROVE_PRD_{short_id}.md")),
                ("Design", format!("GROVE_DESIGN_{short_id}.md")),
                ("Review", format!("GROVE_REVIEW_{short_id}.md")),
            ]
        }
        AgentType::PrePlanner => {
            // PrePlanner is first in the graph pipeline — no upstream artifacts.
            vec![]
        }
        AgentType::GraphCreator => {
            vec![("PrePlan", format!("PREPLAN_{short_id}.md"))]
        }
        AgentType::Verdict => {
            // Verdict reviews Builder output; graph orchestrator injects step context.
            vec![]
        }
        AgentType::PhaseValidator => {
            // PhaseValidator checks cross-step integration; context injected at runtime.
            vec![]
        }
        AgentType::PhaseJudge => {
            // PhaseJudge grades holistically; context injected at runtime.
            vec![]
        }
    };

    if artifacts.is_empty() {
        return String::new();
    }

    let mut lines = vec!["--- UPSTREAM ARTIFACTS ---".to_string()];
    let mut any_found = false;

    for (label, filename) in &artifacts {
        let full_path = artifacts_dir.join(filename);
        if full_path.exists() {
            lines.push(format!(
                "- {label}: `{}` exists — read it before starting.",
                full_path.display()
            ));
            any_found = true;
        }
    }

    if !any_found {
        return String::new();
    }

    lines.push("--- END UPSTREAM ARTIFACTS ---".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn build_prd_instructions_contain_role() {
        let artifacts_dir = Path::new("/tmp/artifacts");
        let instructions = build_agent_instructions(
            AgentType::BuildPrd,
            "Add auth",
            "abc12345",
            artifacts_dir,
            None,
        );
        assert!(instructions.contains("BUILD PRD agent"));
        assert!(instructions.contains("Add auth"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_PRD_abc12345.md"));
    }

    #[test]
    fn plan_system_design_instructions_reference_prd() {
        let artifacts_dir = Path::new("/tmp/artifacts");
        let instructions = build_agent_instructions(
            AgentType::PlanSystemDesign,
            "Add auth",
            "abc12345",
            artifacts_dir,
            None,
        );
        assert!(instructions.contains("PLAN SYSTEM DESIGN agent"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_PRD_abc12345.md"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_DESIGN_abc12345.md"));
    }

    #[test]
    fn builder_instructions_reference_design() {
        let artifacts_dir = Path::new("/tmp/artifacts");
        let instructions = build_agent_instructions(
            AgentType::Builder,
            "Add auth",
            "abc12345",
            artifacts_dir,
            None,
        );
        assert!(instructions.contains("BUILDER agent"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_DESIGN_abc12345.md"));
    }

    #[test]
    fn reviewer_instructions_contain_verdict() {
        let artifacts_dir = Path::new("/tmp/artifacts");
        let instructions = build_agent_instructions(
            AgentType::Reviewer,
            "Add auth",
            "abc12345",
            artifacts_dir,
            None,
        );
        assert!(instructions.contains("REVIEWER agent"));
        assert!(instructions.contains("VERDICT: PASS"));
        assert!(instructions.contains("VERDICT: FAIL"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_REVIEW_abc12345.md"));
    }

    #[test]
    fn judge_instructions_reference_all_artifacts() {
        let artifacts_dir = Path::new("/tmp/artifacts");
        let instructions = build_agent_instructions(
            AgentType::Judge,
            "Add auth",
            "abc12345",
            artifacts_dir,
            None,
        );
        assert!(instructions.contains("JUDGE agent"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_PRD_abc12345.md"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_DESIGN_abc12345.md"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_REVIEW_abc12345.md"));
        assert!(instructions.contains("/tmp/artifacts/GROVE_VERDICT_abc12345.md"));
        assert!(instructions.contains("VERDICT: APPROVED"));
        assert!(instructions.contains("VERDICT: NEEDS_WORK"));
        assert!(instructions.contains("VERDICT: REJECTED"));
    }

    #[test]
    fn handoff_context_is_appended() {
        let artifacts_dir = Path::new("/tmp/artifacts");
        let instructions = build_agent_instructions(
            AgentType::Builder,
            "Fix bug",
            "abc12345",
            artifacts_dir,
            Some("--- PREVIOUS AGENT CHANGES ---\nSome diff here"),
        );
        assert!(instructions.contains("--- PREVIOUS AGENT CHANGES ---"));
        assert!(instructions.contains("Some diff here"));
    }

    #[test]
    fn short_run_id_handled() {
        let artifacts_dir = Path::new("/tmp/artifacts");
        let instructions =
            build_agent_instructions(AgentType::BuildPrd, "Test", "ab", artifacts_dir, None);
        assert!(instructions.contains("/tmp/artifacts/GROVE_PRD_ab.md"));
    }

    #[test]
    fn upstream_artifacts_empty_for_build_prd() {
        let ctx = upstream_artifacts_context(
            AgentType::BuildPrd,
            "abc12345",
            Path::new("/tmp/artifacts"),
        );
        assert!(ctx.is_empty());
    }
}
