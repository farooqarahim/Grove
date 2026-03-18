// Run lifecycle
pub const RUN_CREATED: &str = "run_created";
pub const RUN_COMPLETED: &str = "run_completed";
pub const RUN_FAILED: &str = "run_failed";

// Planning
pub const PLAN_GENERATED: &str = "plan_generated";

// Session lifecycle
pub const SESSION_SPAWNED: &str = "session_spawned";
pub const SESSION_STATE_CHANGED: &str = "session_state_changed";

// Checkpoints
pub const CHECKPOINT_SAVED: &str = "checkpoint_saved";

// Merge
pub const MERGE_QUEUED: &str = "merge_queued";
pub const MERGE_STARTED: &str = "merge_started";
pub const MERGE_COMPLETED: &str = "merge_completed";
pub const MERGE_FAILED: &str = "merge_failed";
pub const MERGE_CONFLICT: &str = "merge_conflict";
pub const CONV_MERGED: &str = "conv_merged";
pub const CONV_REBASED: &str = "conv_rebased";

// Pre-run merge (sync conversation branch with main)
pub const PRE_RUN_MERGE_CLEAN: &str = "pre_run_merge_clean";
pub const PRE_RUN_MERGE_CONFLICT: &str = "pre_run_merge_conflict";
pub const PRE_RUN_CONFLICT_RESOLVED: &str = "pre_run_conflict_resolved";
pub const PRE_RUN_CONFLICT_FAILED: &str = "pre_run_conflict_failed";

// Pre-publish pull (sync conv branch with remote before push)
pub const PRE_PUBLISH_PULL_CLEAN: &str = "pre_publish_pull_clean";
pub const PRE_PUBLISH_PULL_CONFLICT: &str = "pre_publish_pull_conflict";
pub const PRE_PUBLISH_PULL_RESOLVED: &str = "pre_publish_pull_resolved";
pub const PRE_PUBLISH_PULL_FAILED: &str = "pre_publish_pull_failed";
pub const PRE_PUBLISH_PULL_SKIPPED: &str = "pre_publish_pull_skipped";

// Push recovery
pub const GIT_PUSH_RECOVERY_STARTED: &str = "git_push_recovery_started";
pub const GIT_PUSH_RECOVERY_COMPLETED: &str = "git_push_recovery_completed";
pub const GIT_PUSH_RECOVERY_FAILED: &str = "git_push_recovery_failed";

// Security / capability
pub const GUARD_VIOLATION: &str = "guard_violation";

// Ownership
pub const LOCK_ACQUIRED: &str = "lock_acquired";
pub const LOCK_RELEASED: &str = "lock_released";

// Budget
pub const BUDGET_WARNING: &str = "budget_warning";
pub const BUDGET_EXCEEDED: &str = "budget_exceeded";

// Recovery
pub const CRASH_RECOVERY: &str = "crash_recovery";

// Watchdog
pub const WATCHDOG_STALLED: &str = "watchdog_stalled";
pub const WATCHDOG_ZOMBIE: &str = "watchdog_zombie";
pub const WATCHDOG_BOOT_TIMEOUT: &str = "watchdog_boot_timeout";
pub const WATCHDOG_LIFETIME_EXCEEDED: &str = "watchdog_lifetime_exceeded";
pub const WATCHDOG_RUN_LIFETIME_EXCEEDED: &str = "watchdog_run_lifetime_exceeded";

// Signals
pub const SIGNAL_SENT: &str = "signal_sent";
pub const SIGNAL_BROADCAST: &str = "signal_broadcast";
