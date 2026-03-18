use super::RunState;
use super::state_enums::{
    GraphRuntimeStatus, GraphStatus, MergeQueueStatus, PhaseStatus, SessionState, StepStatus,
};

// ── Run transitions ───────────────────────────────────────────────────────────

/// Return `true` if transitioning from `from` → `to` is a legal move for a
/// `Run`.
///
/// Illegal transitions are rejected by `transitions::apply_transition` before
/// any DB write, so the DB never enters an inconsistent state.
pub fn is_valid_run_transition(from: RunState, to: RunState) -> bool {
    matches!(
        (from, to),
        // Forward path
        (RunState::Created, RunState::Planning)
        | (RunState::Planning, RunState::Executing)
        | (RunState::Executing, RunState::WaitingForGate)
        | (RunState::WaitingForGate, RunState::Executing)
        | (RunState::WaitingForGate, RunState::Failed)
        | (RunState::WaitingForGate, RunState::Paused)
        | (RunState::Executing, RunState::Verifying)
        | (RunState::Executing, RunState::Failed)     // agent failure
        | (RunState::Executing, RunState::Paused)     // abort requested
        | (RunState::Verifying, RunState::Publishing)
        | (RunState::Verifying, RunState::Failed)
        | (RunState::Publishing, RunState::Completed)
        | (RunState::Publishing, RunState::Failed)
        | (RunState::Merging, RunState::Completed)
        | (RunState::Merging, RunState::Failed)
        // Recovery path
        | (RunState::Failed, RunState::Executing)     // resume after failure
        | (RunState::Paused, RunState::Executing) // resume after pause
    )
}

// ── Session transitions ───────────────────────────────────────────────────────

/// Return `true` if transitioning a `Session` from `from` → `to` is legal.
///
/// Forward path: `Queued → Running → Waiting → Running → Completed`
/// Failure/abort paths: any non-terminal → `Failed` or `Killed`
pub fn is_valid_session_transition(from: SessionState, to: SessionState) -> bool {
    matches!(
        (from, to),
        // Forward path
        (SessionState::Queued, SessionState::Running)
        | (SessionState::Running, SessionState::Waiting)
        | (SessionState::Waiting, SessionState::Running)   // resumed from wait
        | (SessionState::Running, SessionState::Completed)
        // Failure paths
        | (SessionState::Queued, SessionState::Failed)
        | (SessionState::Running, SessionState::Failed)
        | (SessionState::Waiting, SessionState::Failed)
        // Killed (external abort) from any non-terminal state
        | (SessionState::Queued, SessionState::Killed)
        | (SessionState::Running, SessionState::Killed)
        | (SessionState::Waiting, SessionState::Killed)
    )
}

// ── Graph status transitions ──────────────────────────────────────────────────

/// Return `true` if transitioning a graph's `status` from `from` → `to` is
/// legal.
///
/// Forward path: `Open → InProgress → Closed`
/// Failure path: `Open | InProgress → Failed`
/// Retry path:   `Failed → Open` (full restart)
pub fn is_valid_graph_status_transition(from: GraphStatus, to: GraphStatus) -> bool {
    matches!(
        (from, to),
        // Forward path
        (GraphStatus::Open, GraphStatus::InProgress)
        | (GraphStatus::InProgress, GraphStatus::Closed)
        // Failure paths
        | (GraphStatus::Open, GraphStatus::Failed)
        | (GraphStatus::InProgress, GraphStatus::Failed)
        // Retry: restart a failed graph
        | (GraphStatus::Failed, GraphStatus::Open)
    )
}

/// Return `true` if transitioning a graph's `runtime_status` from `from` → `to`
/// is legal.
///
/// Forward path: `Idle → Queued → Running → Idle` (after loop completes)
/// Pause/resume: `Running ↔ Paused`
/// Abort:        `Running | Paused | Queued → Aborted`
/// Restart:      `Aborted → Idle`
pub fn is_valid_graph_runtime_transition(from: GraphRuntimeStatus, to: GraphRuntimeStatus) -> bool {
    matches!(
        (from, to),
        // Forward path
        (GraphRuntimeStatus::Idle, GraphRuntimeStatus::Queued)
        | (GraphRuntimeStatus::Idle, GraphRuntimeStatus::Running)   // direct start (no queue)
        | (GraphRuntimeStatus::Queued, GraphRuntimeStatus::Running)
        | (GraphRuntimeStatus::Running, GraphRuntimeStatus::Idle)   // loop completed
        // Pause / resume
        | (GraphRuntimeStatus::Running, GraphRuntimeStatus::Paused)
        | (GraphRuntimeStatus::Paused, GraphRuntimeStatus::Running)
        // Abort from any active state
        | (GraphRuntimeStatus::Queued, GraphRuntimeStatus::Aborted)
        | (GraphRuntimeStatus::Running, GraphRuntimeStatus::Aborted)
        | (GraphRuntimeStatus::Paused, GraphRuntimeStatus::Aborted)
        // Restart after abort
        | (GraphRuntimeStatus::Aborted, GraphRuntimeStatus::Idle)
        | (GraphRuntimeStatus::Aborted, GraphRuntimeStatus::Queued)
    )
}

// ── Phase transitions ─────────────────────────────────────────────────────────

/// Return `true` if transitioning a `Phase` status from `from` → `to` is
/// legal.
///
/// Forward path: `Open → InProgress → Closed`
/// Failure path: `Open | InProgress → Failed`
/// Retry path:   `Failed | Closed → Open` (reopen for rework)
pub fn is_valid_phase_transition(from: PhaseStatus, to: PhaseStatus) -> bool {
    matches!(
        (from, to),
        // Forward path
        (PhaseStatus::Open, PhaseStatus::InProgress)
        | (PhaseStatus::InProgress, PhaseStatus::Closed)
        // Failure paths
        | (PhaseStatus::Open, PhaseStatus::Failed)
        | (PhaseStatus::InProgress, PhaseStatus::Failed)
        // Reopen for rework (validation rejection)
        | (PhaseStatus::Closed, PhaseStatus::Open)
        | (PhaseStatus::Failed, PhaseStatus::Open)
    )
}

// ── Step transitions ──────────────────────────────────────────────────────────

/// Return `true` if transitioning a `Step` status from `from` → `to` is legal.
///
/// Forward path: `Open → InProgress → Closed`
/// Failure path: `Open | InProgress → Failed`
/// Retry path:   `Failed | Closed → Open` (reopen for rework)
pub fn is_valid_step_transition(from: StepStatus, to: StepStatus) -> bool {
    matches!(
        (from, to),
        // Forward path
        (StepStatus::Open, StepStatus::InProgress)
        | (StepStatus::InProgress, StepStatus::Closed)
        // Failure paths
        | (StepStatus::Open, StepStatus::Failed)
        | (StepStatus::InProgress, StepStatus::Failed)
        // Reopen for rework / re-run
        | (StepStatus::Closed, StepStatus::Open)
        | (StepStatus::Failed, StepStatus::Open)
    )
}

// ── Merge queue transitions ───────────────────────────────────────────────────

/// Return `true` if transitioning a merge queue entry from `from` → `to` is
/// legal.
///
/// Forward path: `Queued → Running → Completed`
/// Failure paths: `Running → Failed | Conflict`
/// Retry path:    `Failed | Conflict → Queued`
pub fn is_valid_merge_transition(from: MergeQueueStatus, to: MergeQueueStatus) -> bool {
    matches!(
        (from, to),
        // Forward path
        (MergeQueueStatus::Queued, MergeQueueStatus::Running)
        | (MergeQueueStatus::Running, MergeQueueStatus::Completed)
        // Failure paths
        | (MergeQueueStatus::Running, MergeQueueStatus::Failed)
        | (MergeQueueStatus::Running, MergeQueueStatus::Conflict)
        // Retry: re-queue after failure or conflict resolution
        | (MergeQueueStatus::Failed, MergeQueueStatus::Queued)
        | (MergeQueueStatus::Conflict, MergeQueueStatus::Queued)
    )
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Run ──────────────────────────────────────────────────────────────────

    #[test]
    fn run_valid_happy_path() {
        assert!(is_valid_run_transition(
            RunState::Created,
            RunState::Planning
        ));
        assert!(is_valid_run_transition(
            RunState::Planning,
            RunState::Executing
        ));
        assert!(is_valid_run_transition(
            RunState::Executing,
            RunState::WaitingForGate
        ));
        assert!(is_valid_run_transition(
            RunState::WaitingForGate,
            RunState::Executing
        ));
        assert!(is_valid_run_transition(
            RunState::Executing,
            RunState::Verifying
        ));
        assert!(is_valid_run_transition(
            RunState::Verifying,
            RunState::Publishing
        ));
        assert!(is_valid_run_transition(
            RunState::Publishing,
            RunState::Completed
        ));
    }

    #[test]
    fn run_valid_recovery() {
        assert!(is_valid_run_transition(
            RunState::Failed,
            RunState::Executing
        ));
        assert!(is_valid_run_transition(
            RunState::Paused,
            RunState::Executing
        ));
    }

    #[test]
    fn run_invalid_backwards() {
        assert!(!is_valid_run_transition(
            RunState::Completed,
            RunState::Executing
        ));
        assert!(!is_valid_run_transition(
            RunState::Executing,
            RunState::Created
        ));
    }

    #[test]
    fn run_invalid_skip() {
        assert!(!is_valid_run_transition(
            RunState::Created,
            RunState::Completed
        ));
        assert!(!is_valid_run_transition(
            RunState::Planning,
            RunState::Completed
        ));
        assert!(!is_valid_run_transition(
            RunState::Planning,
            RunState::Merging
        ));
    }

    // ── Session ───────────────────────────────────────────────────────────────

    #[test]
    fn session_valid_forward_path() {
        assert!(is_valid_session_transition(
            SessionState::Queued,
            SessionState::Running
        ));
        assert!(is_valid_session_transition(
            SessionState::Running,
            SessionState::Waiting
        ));
        assert!(is_valid_session_transition(
            SessionState::Waiting,
            SessionState::Running
        ));
        assert!(is_valid_session_transition(
            SessionState::Running,
            SessionState::Completed
        ));
    }

    #[test]
    fn session_valid_failure_paths() {
        assert!(is_valid_session_transition(
            SessionState::Running,
            SessionState::Failed
        ));
        assert!(is_valid_session_transition(
            SessionState::Waiting,
            SessionState::Failed
        ));
        assert!(is_valid_session_transition(
            SessionState::Running,
            SessionState::Killed
        ));
    }

    #[test]
    fn session_invalid_transitions() {
        // Cannot go backwards from Completed
        assert!(!is_valid_session_transition(
            SessionState::Completed,
            SessionState::Running
        ));
        // Cannot skip from Queued to Completed
        assert!(!is_valid_session_transition(
            SessionState::Queued,
            SessionState::Completed
        ));
        // Cannot recover from Killed
        assert!(!is_valid_session_transition(
            SessionState::Killed,
            SessionState::Running
        ));
    }

    // ── Graph status ──────────────────────────────────────────────────────────

    #[test]
    fn graph_status_valid_forward() {
        assert!(is_valid_graph_status_transition(
            GraphStatus::Open,
            GraphStatus::InProgress
        ));
        assert!(is_valid_graph_status_transition(
            GraphStatus::InProgress,
            GraphStatus::Closed
        ));
        assert!(is_valid_graph_status_transition(
            GraphStatus::InProgress,
            GraphStatus::Failed
        ));
        // Restart after failure
        assert!(is_valid_graph_status_transition(
            GraphStatus::Failed,
            GraphStatus::Open
        ));
    }

    #[test]
    fn graph_status_invalid() {
        // Cannot reopen Closed
        assert!(!is_valid_graph_status_transition(
            GraphStatus::Closed,
            GraphStatus::Open
        ));
        // Cannot skip directly to Closed
        assert!(!is_valid_graph_status_transition(
            GraphStatus::Open,
            GraphStatus::Closed
        ));
    }

    // ── Graph runtime status ──────────────────────────────────────────────────

    #[test]
    fn graph_runtime_valid_transitions() {
        assert!(is_valid_graph_runtime_transition(
            GraphRuntimeStatus::Idle,
            GraphRuntimeStatus::Running
        ));
        assert!(is_valid_graph_runtime_transition(
            GraphRuntimeStatus::Running,
            GraphRuntimeStatus::Paused
        ));
        assert!(is_valid_graph_runtime_transition(
            GraphRuntimeStatus::Paused,
            GraphRuntimeStatus::Running
        ));
        assert!(is_valid_graph_runtime_transition(
            GraphRuntimeStatus::Running,
            GraphRuntimeStatus::Aborted
        ));
        assert!(is_valid_graph_runtime_transition(
            GraphRuntimeStatus::Running,
            GraphRuntimeStatus::Idle
        ));
    }

    #[test]
    fn graph_runtime_invalid() {
        // Cannot go from Aborted directly to Running
        assert!(!is_valid_graph_runtime_transition(
            GraphRuntimeStatus::Aborted,
            GraphRuntimeStatus::Running
        ));
        // Cannot go from Idle to Aborted
        assert!(!is_valid_graph_runtime_transition(
            GraphRuntimeStatus::Idle,
            GraphRuntimeStatus::Aborted
        ));
    }

    // ── Phase ─────────────────────────────────────────────────────────────────

    #[test]
    fn phase_valid_forward() {
        assert!(is_valid_phase_transition(
            PhaseStatus::Open,
            PhaseStatus::InProgress
        ));
        assert!(is_valid_phase_transition(
            PhaseStatus::InProgress,
            PhaseStatus::Closed
        ));
        assert!(is_valid_phase_transition(
            PhaseStatus::InProgress,
            PhaseStatus::Failed
        ));
        // Reopen for rework
        assert!(is_valid_phase_transition(
            PhaseStatus::Failed,
            PhaseStatus::Open
        ));
        assert!(is_valid_phase_transition(
            PhaseStatus::Closed,
            PhaseStatus::Open
        ));
    }

    #[test]
    fn phase_invalid() {
        // Cannot skip from Open to Closed
        assert!(!is_valid_phase_transition(
            PhaseStatus::Open,
            PhaseStatus::Closed
        ));
        // Cannot go from Closed to Failed
        assert!(!is_valid_phase_transition(
            PhaseStatus::Closed,
            PhaseStatus::Failed
        ));
    }

    // ── Step ──────────────────────────────────────────────────────────────────

    #[test]
    fn step_valid_forward() {
        assert!(is_valid_step_transition(
            StepStatus::Open,
            StepStatus::InProgress
        ));
        assert!(is_valid_step_transition(
            StepStatus::InProgress,
            StepStatus::Closed
        ));
        assert!(is_valid_step_transition(
            StepStatus::InProgress,
            StepStatus::Failed
        ));
        // Reopen for re-run
        assert!(is_valid_step_transition(
            StepStatus::Failed,
            StepStatus::Open
        ));
        assert!(is_valid_step_transition(
            StepStatus::Closed,
            StepStatus::Open
        ));
    }

    #[test]
    fn step_invalid() {
        // Cannot skip from Open to Closed
        assert!(!is_valid_step_transition(
            StepStatus::Open,
            StepStatus::Closed
        ));
        // Cannot move from Closed directly to Failed
        assert!(!is_valid_step_transition(
            StepStatus::Closed,
            StepStatus::Failed
        ));
    }

    // ── Merge queue ───────────────────────────────────────────────────────────

    #[test]
    fn merge_valid_forward() {
        assert!(is_valid_merge_transition(
            MergeQueueStatus::Queued,
            MergeQueueStatus::Running
        ));
        assert!(is_valid_merge_transition(
            MergeQueueStatus::Running,
            MergeQueueStatus::Completed
        ));
        assert!(is_valid_merge_transition(
            MergeQueueStatus::Running,
            MergeQueueStatus::Failed
        ));
        assert!(is_valid_merge_transition(
            MergeQueueStatus::Running,
            MergeQueueStatus::Conflict
        ));
    }

    #[test]
    fn merge_valid_retry() {
        assert!(is_valid_merge_transition(
            MergeQueueStatus::Failed,
            MergeQueueStatus::Queued
        ));
        assert!(is_valid_merge_transition(
            MergeQueueStatus::Conflict,
            MergeQueueStatus::Queued
        ));
    }

    #[test]
    fn merge_invalid() {
        // Cannot go from Completed back to Running
        assert!(!is_valid_merge_transition(
            MergeQueueStatus::Completed,
            MergeQueueStatus::Running
        ));
        // Cannot skip from Queued to Completed
        assert!(!is_valid_merge_transition(
            MergeQueueStatus::Queued,
            MergeQueueStatus::Completed
        ));
        // Cannot go from Queued to Conflict (conflict only happens during run)
        assert!(!is_valid_merge_transition(
            MergeQueueStatus::Queued,
            MergeQueueStatus::Conflict
        ));
    }
}
