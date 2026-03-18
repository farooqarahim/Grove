use std::fmt;

use serde::{Deserialize, Serialize};

/// Agent types in Grove's pipeline and graph systems.
///
/// **Pipeline agents (original 5):**
/// - `BuildPrd` — writes product requirements from user objective
/// - `PlanSystemDesign` — designs architecture, data models, implementation plan
/// - `Builder` — implements code, runs tests (reused by Graph system)
/// - `Reviewer` — audits changes, PASS/FAIL verdict
/// - `Judge` — final quality arbiter, APPROVED/NEEDS_WORK/REJECTED (reused by Graph system)
///
/// **Graph agents (5 new):**
/// - `PrePlanner` — generates missing foundational docs (PRD, System Design, Guidelines)
/// - `GraphCreator` — decomposes specs into phases/steps via MCP tools
/// - `Verdict` — reviews Builder output, runs tests/lints/checks (read-only, can run commands)
/// - `PhaseValidator` — cross-step integration check, runs integration tests (read-only, can run commands)
/// - `PhaseJudge` — grades phase holistically (read-only, no write, no commands)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    BuildPrd,
    PlanSystemDesign,
    Builder,
    Reviewer,
    Judge,
    PrePlanner,
    GraphCreator,
    Verdict,
    PhaseValidator,
    PhaseJudge,
}

impl AgentType {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentType::BuildPrd => "build_prd",
            AgentType::PlanSystemDesign => "plan_system_design",
            AgentType::Builder => "builder",
            AgentType::Reviewer => "reviewer",
            AgentType::Judge => "judge",
            AgentType::PrePlanner => "pre_planner",
            AgentType::GraphCreator => "graph_creator",
            AgentType::Verdict => "verdict",
            AgentType::PhaseValidator => "phase_validator",
            AgentType::PhaseJudge => "phase_judge",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            AgentType::BuildPrd => "Build PRD",
            AgentType::PlanSystemDesign => "Plan System Design",
            AgentType::Builder => "Builder",
            AgentType::Reviewer => "Reviewer",
            AgentType::Judge => "Judge",
            AgentType::PrePlanner => "Pre Planner",
            AgentType::GraphCreator => "Graph Creator",
            AgentType::Verdict => "Verdict",
            AgentType::PhaseValidator => "Phase Validator",
            AgentType::PhaseJudge => "Phase Judge",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "build_prd" => Some(AgentType::BuildPrd),
            "plan_system_design" => Some(AgentType::PlanSystemDesign),
            "builder" => Some(AgentType::Builder),
            "reviewer" => Some(AgentType::Reviewer),
            "judge" => Some(AgentType::Judge),
            "pre_planner" => Some(AgentType::PrePlanner),
            "graph_creator" => Some(AgentType::GraphCreator),
            "verdict" => Some(AgentType::Verdict),
            "phase_validator" => Some(AgentType::PhaseValidator),
            "phase_judge" => Some(AgentType::PhaseJudge),
            // Legacy aliases for backward compat with existing DB rows
            "architect" | "planner" | "prd" => Some(AgentType::BuildPrd),
            "spec" | "api_designer" => Some(AgentType::PlanSystemDesign),
            "tester" | "debugger" | "refactorer" | "optimizer" | "performance"
            | "data_migrator" | "devops" | "deployer" | "integrator" => Some(AgentType::Builder),
            "security" | "qa" | "compliance" | "accessibility" | "documenter" | "reporter"
            | "validator" => Some(AgentType::Reviewer),
            "project_manager" | "coordinator" | "migration_planner" | "researcher" | "monitor"
            | "dependency_manager" => Some(AgentType::Builder),
            _ => None,
        }
    }

    /// Whether this agent can modify files in the worktree.
    ///
    /// **Deprecated fallback:** Prefer reading `can_write` from the agent's
    /// Markdown config (`skills/agents/<id>.md`). The engine reads Markdown
    /// configs first and falls back to this method only if no config exists.
    pub fn can_write(self) -> bool {
        match self {
            AgentType::BuildPrd
            | AgentType::PlanSystemDesign
            | AgentType::Builder
            | AgentType::Reviewer
            | AgentType::Judge
            | AgentType::PrePlanner
            | AgentType::GraphCreator => true,
            AgentType::Verdict | AgentType::PhaseValidator | AgentType::PhaseJudge => false,
        }
    }

    /// Whether this agent can run shell commands (Bash tool).
    ///
    /// **Deprecated fallback:** Prefer reading `can_run_commands` from the
    /// agent's Markdown config. The engine reads Markdown configs first.
    pub fn can_run_commands(self) -> bool {
        match self {
            AgentType::BuildPrd
            | AgentType::PlanSystemDesign
            | AgentType::Judge
            | AgentType::PrePlanner
            | AgentType::GraphCreator
            | AgentType::PhaseJudge => false,
            AgentType::Builder
            | AgentType::Reviewer
            | AgentType::Verdict
            | AgentType::PhaseValidator => true,
        }
    }

    /// Compute the `--allowedTools` list for this agent.
    ///
    /// Returns `None` when all tools are permitted (no restriction needed).
    /// Returns `Some(tools)` listing only the Claude Code tools this agent may use.
    ///
    /// **Deprecated fallback:** Prefer `allowed_tools` from the agent's Markdown
    /// config. The engine already does this (config-first, enum-fallback).
    pub fn allowed_tools(self) -> Option<Vec<String>> {
        // Graph agents get full tool access; MCP tools are injected at runtime.
        match self {
            AgentType::PrePlanner
            | AgentType::GraphCreator
            | AgentType::Verdict
            | AgentType::PhaseValidator
            | AgentType::PhaseJudge => return None,
            _ => {}
        }

        let mut tools: Vec<String> = vec!["Read".into(), "Glob".into(), "Grep".into(), "LS".into()];

        if self.can_write() {
            tools.push("Edit".into());
            tools.push("Write".into());
            tools.push("MultiEdit".into());
        }
        if self.can_run_commands() {
            tools.push("Bash".into());
        }

        // If all major tools are enabled, return None (no restriction).
        if self.can_write() && self.can_run_commands() {
            return None;
        }

        Some(tools)
    }

    /// The output artifact filename for this agent, if it produces a document.
    ///
    /// **Deprecated fallback:** Prefer `artifact` from the agent's Markdown config.
    pub fn artifact_filename(self, run_id: &str) -> Option<String> {
        let short_id = if run_id.len() >= 8 {
            &run_id[..8]
        } else {
            run_id
        };
        match self {
            AgentType::BuildPrd => Some(format!("GROVE_PRD_{short_id}.md")),
            AgentType::PlanSystemDesign => Some(format!("GROVE_DESIGN_{short_id}.md")),
            AgentType::Builder => None, // produces code, not a doc
            AgentType::Reviewer => Some(format!("GROVE_REVIEW_{short_id}.md")),
            AgentType::Judge => Some(format!("GROVE_VERDICT_{short_id}.md")),
            AgentType::PrePlanner => Some(format!("PREPLAN_{short_id}.md")),
            AgentType::GraphCreator => Some(format!("GRAPH_SPEC_{short_id}.json")),
            AgentType::Verdict => Some(format!("VERDICT_{short_id}.json")),
            AgentType::PhaseValidator => Some(format!("PHASE_VAL_{short_id}.json")),
            AgentType::PhaseJudge => Some(format!("PHASE_JUDGE_{short_id}.json")),
        }
    }
}

impl fmt::Display for AgentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
