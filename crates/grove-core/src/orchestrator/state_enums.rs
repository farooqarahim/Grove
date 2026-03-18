/// Typed enums for entity lifecycle states.
///
/// Each enum mirrors the CHECK constraints in the SQLite schema so that
/// state values are validated at the type level, not just as raw strings.
/// The `as_str` / `from_str` methods convert to/from the canonical DB
/// representations.
use serde::{Deserialize, Serialize};

// ── Session ───────────────────────────────────────────────────────────────────

/// Session lifecycle states (`sessions.state`).
///
/// DB constraint: `CHECK(state IN ('queued','running','waiting','completed','failed','killed'))`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionState {
    Queued,
    Running,
    Waiting,
    Completed,
    Failed,
    Killed,
}

impl SessionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionState::Queued => "queued",
            SessionState::Running => "running",
            SessionState::Waiting => "waiting",
            SessionState::Completed => "completed",
            SessionState::Failed => "failed",
            SessionState::Killed => "killed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(SessionState::Queued),
            "running" => Some(SessionState::Running),
            "waiting" => Some(SessionState::Waiting),
            "completed" => Some(SessionState::Completed),
            "failed" => Some(SessionState::Failed),
            "killed" => Some(SessionState::Killed),
            _ => None,
        }
    }
}

// ── Grove Graph ───────────────────────────────────────────────────────────────

/// Grove graph high-level completion status (`grove_graphs.status`).
///
/// DB constraint: `CHECK(status IN ('open','inprogress','closed','failed'))`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GraphStatus {
    Open,
    InProgress,
    Closed,
    Failed,
}

impl GraphStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GraphStatus::Open => "open",
            GraphStatus::InProgress => "inprogress",
            GraphStatus::Closed => "closed",
            GraphStatus::Failed => "failed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(GraphStatus::Open),
            "inprogress" => Some(GraphStatus::InProgress),
            "closed" => Some(GraphStatus::Closed),
            "failed" => Some(GraphStatus::Failed),
            _ => None,
        }
    }
}

/// Grove graph runtime execution status (`grove_graphs.runtime_status`).
///
/// DB constraint: `CHECK(runtime_status IN ('idle','running','paused','aborted'))`
///
/// Note: some DB versions also include `'queued'`; both variants are handled in
/// `from_str` for forward-compatibility.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum GraphRuntimeStatus {
    Idle,
    Queued,
    Running,
    Paused,
    Aborted,
}

impl GraphRuntimeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            GraphRuntimeStatus::Idle => "idle",
            GraphRuntimeStatus::Queued => "queued",
            GraphRuntimeStatus::Running => "running",
            GraphRuntimeStatus::Paused => "paused",
            GraphRuntimeStatus::Aborted => "aborted",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "idle" => Some(GraphRuntimeStatus::Idle),
            "queued" => Some(GraphRuntimeStatus::Queued),
            "running" => Some(GraphRuntimeStatus::Running),
            "paused" => Some(GraphRuntimeStatus::Paused),
            "aborted" => Some(GraphRuntimeStatus::Aborted),
            _ => None,
        }
    }
}

// ── Graph Phase ───────────────────────────────────────────────────────────────

/// Graph phase completion status (`graph_phases.status`).
///
/// DB constraint: `CHECK(status IN ('open','inprogress','closed','failed'))`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PhaseStatus {
    Open,
    InProgress,
    Closed,
    Failed,
}

impl PhaseStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PhaseStatus::Open => "open",
            PhaseStatus::InProgress => "inprogress",
            PhaseStatus::Closed => "closed",
            PhaseStatus::Failed => "failed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(PhaseStatus::Open),
            "inprogress" => Some(PhaseStatus::InProgress),
            "closed" => Some(PhaseStatus::Closed),
            "failed" => Some(PhaseStatus::Failed),
            _ => None,
        }
    }
}

// ── Graph Step ────────────────────────────────────────────────────────────────

/// Graph step completion status (`graph_steps.status`).
///
/// DB constraint: `CHECK(status IN ('open','inprogress','closed','failed'))`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum StepStatus {
    Open,
    InProgress,
    Closed,
    Failed,
}

impl StepStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            StepStatus::Open => "open",
            StepStatus::InProgress => "inprogress",
            StepStatus::Closed => "closed",
            StepStatus::Failed => "failed",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(StepStatus::Open),
            "inprogress" => Some(StepStatus::InProgress),
            "closed" => Some(StepStatus::Closed),
            "failed" => Some(StepStatus::Failed),
            _ => None,
        }
    }
}

// ── Merge Queue ───────────────────────────────────────────────────────────────

/// Merge queue entry status (`merge_queue.status`).
///
/// DB constraint: `CHECK(status IN ('queued','running','completed','failed','conflict'))`
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum MergeQueueStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Conflict,
}

impl MergeQueueStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            MergeQueueStatus::Queued => "queued",
            MergeQueueStatus::Running => "running",
            MergeQueueStatus::Completed => "completed",
            MergeQueueStatus::Failed => "failed",
            MergeQueueStatus::Conflict => "conflict",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(MergeQueueStatus::Queued),
            "running" => Some(MergeQueueStatus::Running),
            "completed" => Some(MergeQueueStatus::Completed),
            "failed" => Some(MergeQueueStatus::Failed),
            "conflict" => Some(MergeQueueStatus::Conflict),
            _ => None,
        }
    }
}
