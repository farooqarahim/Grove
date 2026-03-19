pub mod chunking;
pub mod execution;
pub mod git_ops;
pub mod loop_orchestrator;
pub mod orchestrator_dispatch;
pub mod planning;
pub mod skill_loader;
pub mod worker_dispatch;

use serde::{Deserialize, Serialize};

// ── Serializable domain enums ────────────────────────────────────────────────
//
// Each enum maps 1-to-1 with a CHECK constraint in the grove_graph DB migration.
// All variants round-trip through `as_str()` / `TryFrom<&str>`.

// ── GraphStatus ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphStatus {
    Open,
    Inprogress,
    Closed,
    Failed,
}

impl GraphStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Inprogress => "inprogress",
            Self::Closed => "closed",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for GraphStatus {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "open" => Ok(Self::Open),
            "inprogress" => Ok(Self::Inprogress),
            "closed" => Ok(Self::Closed),
            "failed" => Ok(Self::Failed),
            other => Err(format!("invalid GraphStatus: '{other}'")),
        }
    }
}

// ── RuntimeStatus ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStatus {
    Idle,
    Queued,
    Running,
    Paused,
    Aborted,
}

impl RuntimeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Aborted => "aborted",
        }
    }
}

impl TryFrom<&str> for RuntimeStatus {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "idle" => Ok(Self::Idle),
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "aborted" => Ok(Self::Aborted),
            other => Err(format!("invalid RuntimeStatus: '{other}'")),
        }
    }
}

// ── ParsingStatus ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParsingStatus {
    Pending,
    Generating,
    DraftReady,
    Planning,
    Parsing,
    Complete,
    Error,
}

impl ParsingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Generating => "generating",
            Self::DraftReady => "draft_ready",
            Self::Planning => "planning",
            Self::Parsing => "parsing",
            Self::Complete => "complete",
            Self::Error => "error",
        }
    }
}

impl TryFrom<&str> for ParsingStatus {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, <Self as TryFrom<&str>>::Error> {
        match s {
            "pending" => Ok(Self::Pending),
            "generating" => Ok(Self::Generating),
            "draft_ready" => Ok(Self::DraftReady),
            "planning" => Ok(Self::Planning),
            "parsing" => Ok(Self::Parsing),
            "complete" => Ok(Self::Complete),
            "error" => Ok(Self::Error),
            other => Err(format!("invalid ParsingStatus: '{other}'")),
        }
    }
}

// ── ValidationStatus ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Pending,
    Validating,
    Passed,
    Failed,
    Fixing,
}

impl ValidationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Validating => "validating",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Fixing => "fixing",
        }
    }
}

impl TryFrom<&str> for ValidationStatus {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "pending" => Ok(Self::Pending),
            "validating" => Ok(Self::Validating),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            "fixing" => Ok(Self::Fixing),
            other => Err(format!("invalid ValidationStatus: '{other}'")),
        }
    }
}

// ── ExecutionMode ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Auto,
    Manual,
}

impl ExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }
}

impl TryFrom<&str> for ExecutionMode {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            other => Err(format!("invalid ExecutionMode: '{other}'")),
        }
    }
}

// ── GraphExecutionMode ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphExecutionMode {
    Sequential,
    Parallel,
}

impl GraphExecutionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sequential => "sequential",
            Self::Parallel => "parallel",
        }
    }
}

impl TryFrom<&str> for GraphExecutionMode {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "sequential" => Ok(Self::Sequential),
            "parallel" => Ok(Self::Parallel),
            other => Err(format!("invalid GraphExecutionMode: '{other}'")),
        }
    }
}

// ── StepType ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepType {
    Code,
    Config,
    Docs,
    Infra,
    Test,
}

impl StepType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Config => "config",
            Self::Docs => "docs",
            Self::Infra => "infra",
            Self::Test => "test",
        }
    }
}

impl TryFrom<&str> for StepType {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "code" => Ok(Self::Code),
            "config" => Ok(Self::Config),
            "docs" => Ok(Self::Docs),
            "infra" => Ok(Self::Infra),
            "test" => Ok(Self::Test),
            other => Err(format!("invalid StepType: '{other}'")),
        }
    }
}

// ── GitMergeStatus ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitMergeStatus {
    Pending,
    Merged,
    Failed,
}

impl GitMergeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Merged => "merged",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for GitMergeStatus {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "pending" => Ok(Self::Pending),
            "merged" => Ok(Self::Merged),
            "failed" => Ok(Self::Failed),
            other => Err(format!("invalid GitMergeStatus: '{other}'")),
        }
    }
}

// ── StepPipelineStage ────────────────────────────────────────────────────────
//
// Computed at runtime from step state — not stored in the database.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepPipelineStage {
    Pending,
    Building,
    Verdict,
    Judging,
    Done,
    Failed,
}

impl StepPipelineStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Building => "building",
            Self::Verdict => "verdict",
            Self::Judging => "judging",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

impl TryFrom<&str> for StepPipelineStage {
    type Error = String;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s {
            "pending" => Ok(Self::Pending),
            "building" => Ok(Self::Building),
            "verdict" => Ok(Self::Verdict),
            "judging" => Ok(Self::Judging),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            other => Err(format!("invalid StepPipelineStage: '{other}'")),
        }
    }
}

// ── GraphConfig ──────────────────────────────────────────────────────────────
//
// Persisted as key-value pairs in `grove_graph_config`. Each boolean field
// corresponds to one row with `config_value` = "true" / "false".

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphConfig {
    pub doc_prd: bool,
    pub doc_system_design: bool,
    pub doc_guidelines: bool,
    pub platform_frontend: bool,
    pub platform_backend: bool,
    pub platform_desktop: bool,
    pub platform_mobile: bool,
    pub arch_tech_stack: bool,
    pub arch_saas: bool,
    pub arch_multiuser: bool,
    pub arch_dlib: bool,
    /// Whether to push the graph branch to origin on finalize.
    pub git_push: bool,
    /// Whether to create a PR via `gh` on finalize (requires git_push).
    pub git_create_pr: bool,
}

impl GraphConfig {
    /// Build a `GraphConfig` from DB key-value pairs.
    ///
    /// Keys that are absent or have non-"true" values default to `false`.
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        let lookup = |key: &str| -> bool { pairs.iter().any(|(k, v)| k == key && v == "true") };
        Self {
            doc_prd: lookup("doc_prd"),
            doc_system_design: lookup("doc_system_design"),
            doc_guidelines: lookup("doc_guidelines"),
            platform_frontend: lookup("platform_frontend"),
            platform_backend: lookup("platform_backend"),
            platform_desktop: lookup("platform_desktop"),
            platform_mobile: lookup("platform_mobile"),
            arch_tech_stack: lookup("arch_tech_stack"),
            arch_saas: lookup("arch_saas"),
            arch_multiuser: lookup("arch_multiuser"),
            arch_dlib: lookup("arch_dlib"),
            git_push: lookup("git_push"),
            git_create_pr: lookup("git_create_pr"),
        }
    }

    /// Serialize every field as a `(key, value)` pair suitable for DB insertion.
    pub fn to_config_pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("doc_prd", self.doc_prd.to_string()),
            ("doc_system_design", self.doc_system_design.to_string()),
            ("doc_guidelines", self.doc_guidelines.to_string()),
            ("platform_frontend", self.platform_frontend.to_string()),
            ("platform_backend", self.platform_backend.to_string()),
            ("platform_desktop", self.platform_desktop.to_string()),
            ("platform_mobile", self.platform_mobile.to_string()),
            ("arch_tech_stack", self.arch_tech_stack.to_string()),
            ("arch_saas", self.arch_saas.to_string()),
            ("arch_multiuser", self.arch_multiuser.to_string()),
            ("arch_dlib", self.arch_dlib.to_string()),
            ("git_push", self.git_push.to_string()),
            ("git_create_pr", self.git_create_pr.to_string()),
        ]
    }
}

// ── Loop result types (internal only — not serialized over IPC) ──────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseValidationResult {
    Passed,
    Retrying,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopIterationResult {
    Continue,
    GraphComplete,
    Paused,
    Aborted,
    Deadlock,
    Error(String),
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify every DB-facing enum round-trips through `as_str` / `TryFrom`.
    macro_rules! assert_round_trip {
        ($ty:ty, $($variant:expr => $str:literal),+ $(,)?) => {
            $(
                assert_eq!($variant.as_str(), $str);
                assert_eq!(<$ty>::try_from($str).unwrap(), $variant);
            )+
            // Unknown values must fail.
            assert!(<$ty>::try_from("__bogus__").is_err());
        };
    }

    #[test]
    fn graph_status_round_trip() {
        assert_round_trip!(GraphStatus,
            GraphStatus::Open => "open",
            GraphStatus::Inprogress => "inprogress",
            GraphStatus::Closed => "closed",
            GraphStatus::Failed => "failed",
        );
    }

    #[test]
    fn runtime_status_round_trip() {
        assert_round_trip!(RuntimeStatus,
            RuntimeStatus::Idle => "idle",
            RuntimeStatus::Queued => "queued",
            RuntimeStatus::Running => "running",
            RuntimeStatus::Paused => "paused",
            RuntimeStatus::Aborted => "aborted",
        );
    }

    #[test]
    fn parsing_status_round_trip() {
        assert_round_trip!(ParsingStatus,
            ParsingStatus::Pending => "pending",
            ParsingStatus::Generating => "generating",
            ParsingStatus::DraftReady => "draft_ready",
            ParsingStatus::Planning => "planning",
            ParsingStatus::Parsing => "parsing",
            ParsingStatus::Complete => "complete",
            ParsingStatus::Error => "error",
        );
    }

    #[test]
    fn validation_status_round_trip() {
        assert_round_trip!(ValidationStatus,
            ValidationStatus::Pending => "pending",
            ValidationStatus::Validating => "validating",
            ValidationStatus::Passed => "passed",
            ValidationStatus::Failed => "failed",
            ValidationStatus::Fixing => "fixing",
        );
    }

    #[test]
    fn execution_mode_round_trip() {
        assert_round_trip!(ExecutionMode,
            ExecutionMode::Auto => "auto",
            ExecutionMode::Manual => "manual",
        );
    }

    #[test]
    fn graph_execution_mode_round_trip() {
        assert_round_trip!(GraphExecutionMode,
            GraphExecutionMode::Sequential => "sequential",
            GraphExecutionMode::Parallel => "parallel",
        );
    }

    #[test]
    fn step_type_round_trip() {
        assert_round_trip!(StepType,
            StepType::Code => "code",
            StepType::Config => "config",
            StepType::Docs => "docs",
            StepType::Infra => "infra",
            StepType::Test => "test",
        );
    }

    #[test]
    fn git_merge_status_round_trip() {
        assert_round_trip!(GitMergeStatus,
            GitMergeStatus::Pending => "pending",
            GitMergeStatus::Merged => "merged",
            GitMergeStatus::Failed => "failed",
        );
    }

    #[test]
    fn step_pipeline_stage_round_trip() {
        assert_round_trip!(StepPipelineStage,
            StepPipelineStage::Pending => "pending",
            StepPipelineStage::Building => "building",
            StepPipelineStage::Verdict => "verdict",
            StepPipelineStage::Judging => "judging",
            StepPipelineStage::Done => "done",
            StepPipelineStage::Failed => "failed",
        );
    }

    #[test]
    fn graph_config_from_pairs_defaults_to_false() {
        let config = GraphConfig::from_pairs(&[]);
        assert!(!config.doc_prd);
        assert!(!config.platform_frontend);
        assert!(!config.arch_saas);
    }

    #[test]
    fn graph_config_from_pairs_picks_up_true() {
        let pairs = vec![
            ("doc_prd".to_string(), "true".to_string()),
            ("arch_saas".to_string(), "true".to_string()),
            ("platform_backend".to_string(), "false".to_string()),
        ];
        let config = GraphConfig::from_pairs(&pairs);
        assert!(config.doc_prd);
        assert!(config.arch_saas);
        assert!(!config.platform_backend);
        assert!(!config.doc_system_design);
    }

    #[test]
    fn graph_config_to_pairs_round_trip() {
        let config = GraphConfig {
            doc_prd: true,
            platform_mobile: true,
            ..Default::default()
        };
        let pairs: Vec<(String, String)> = config
            .to_config_pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let restored = GraphConfig::from_pairs(&pairs);
        assert!(restored.doc_prd);
        assert!(restored.platform_mobile);
        assert!(!restored.arch_dlib);
    }
}
