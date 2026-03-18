/// Pipeline definitions for the 3 Grove execution modes.
///
/// - `Plan`       — BuildPrd → PlanSystemDesign (requirements + design, no code)
/// - `Build`      — Builder → Reviewer → Judge (implementation + quality gates)
/// - `Autonomous` — BuildPrd → PlanSystemDesign → Builder → Reviewer → Judge (full)
use serde::{Deserialize, Serialize};

use crate::agents::AgentType;

/// The 3 pipeline modes available in Grove.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineKind {
    /// Requirements + design only. No code changes.
    Plan,
    /// Implementation + quality gates. Use when you already have a plan.
    Build,
    /// Full end-to-end: PRD → Design → Build → Review → Judge.
    #[default]
    Autonomous,
}

impl PipelineKind {
    /// The ordered sequence of agents for this pipeline.
    pub fn agents(self) -> Vec<AgentType> {
        match self {
            PipelineKind::Plan => vec![AgentType::BuildPrd, AgentType::PlanSystemDesign],
            PipelineKind::Build => vec![AgentType::Builder, AgentType::Reviewer, AgentType::Judge],
            PipelineKind::Autonomous => vec![
                AgentType::BuildPrd,
                AgentType::PlanSystemDesign,
                AgentType::Builder,
                AgentType::Reviewer,
                AgentType::Judge,
            ],
        }
    }

    /// Agents after which execution should pause for user review (gate).
    pub fn gates(self) -> Vec<AgentType> {
        match self {
            PipelineKind::Plan => vec![AgentType::BuildPrd],
            PipelineKind::Build => vec![], // auto-advance through all
            PipelineKind::Autonomous => {
                vec![AgentType::BuildPrd, AgentType::PlanSystemDesign]
            }
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            PipelineKind::Plan => "Plan Mode",
            PipelineKind::Build => "Build Mode",
            PipelineKind::Autonomous => "Autonomous Mode",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PipelineKind::Plan => "plan",
            PipelineKind::Build => "build",
            PipelineKind::Autonomous => "autonomous",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "plan" | "plan-mode" => Some(PipelineKind::Plan),
            "build" | "build-mode" => Some(PipelineKind::Build),
            "autonomous" | "auto" | "full" => Some(PipelineKind::Autonomous),
            // Legacy aliases — map old pipeline names to closest match
            "instant" | "quick" | "prototype" | "bugfix" | "ci-fix" | "ci_fix" => {
                Some(PipelineKind::Build)
            }
            "standard" | "secure" | "hardened" | "enterprise" | "fullstack" | "parallel-build"
            | "parallel_build" | "migration" | "cleanup" => Some(PipelineKind::Autonomous),
            "plan-only" | "plan_only" | "docs" | "investigate" | "review-only" | "review_only"
            | "security-audit" | "security_audit" => Some(PipelineKind::Plan),
            "refactor" | "test-coverage" | "test_coverage" => Some(PipelineKind::Build),
            _ => None,
        }
    }
}


impl std::fmt::Display for PipelineKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_pipeline_has_correct_agents() {
        let agents = PipelineKind::Plan.agents();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0], AgentType::BuildPrd);
        assert_eq!(agents[1], AgentType::PlanSystemDesign);
    }

    #[test]
    fn build_pipeline_has_correct_agents() {
        let agents = PipelineKind::Build.agents();
        assert_eq!(agents.len(), 3);
        assert_eq!(agents[0], AgentType::Builder);
        assert_eq!(agents[1], AgentType::Reviewer);
        assert_eq!(agents[2], AgentType::Judge);
    }

    #[test]
    fn autonomous_pipeline_has_all_five_agents() {
        let agents = PipelineKind::Autonomous.agents();
        assert_eq!(agents.len(), 5);
        assert_eq!(agents[0], AgentType::BuildPrd);
        assert_eq!(agents[4], AgentType::Judge);
    }

    #[test]
    fn gates_plan_mode() {
        let gates = PipelineKind::Plan.gates();
        assert_eq!(gates, vec![AgentType::BuildPrd]);
    }

    #[test]
    fn gates_build_mode_empty() {
        assert!(PipelineKind::Build.gates().is_empty());
    }

    #[test]
    fn gates_autonomous_mode() {
        let gates = PipelineKind::Autonomous.gates();
        assert_eq!(gates.len(), 2);
    }

    #[test]
    fn from_str_round_trips() {
        for kind in [
            PipelineKind::Plan,
            PipelineKind::Build,
            PipelineKind::Autonomous,
        ] {
            assert_eq!(PipelineKind::from_str(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn legacy_aliases_map_correctly() {
        assert_eq!(PipelineKind::from_str("instant"), Some(PipelineKind::Build));
        assert_eq!(
            PipelineKind::from_str("standard"),
            Some(PipelineKind::Autonomous)
        );
        assert_eq!(
            PipelineKind::from_str("plan-only"),
            Some(PipelineKind::Plan)
        );
        assert_eq!(
            PipelineKind::from_str("enterprise"),
            Some(PipelineKind::Autonomous)
        );
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(PipelineKind::from_str("nonexistent"), None);
    }

    #[test]
    fn default_is_autonomous() {
        assert_eq!(PipelineKind::default(), PipelineKind::Autonomous);
    }
}
