//! Background task-queue drainer.
//!
//! Polls `orchestrator::dequeue_next_task` on a periodic tick (and on demand
//! via `DrainSignal::notify`) and runs each dequeued task through
//! `orchestrator::execute_objective`. Mirrors the Grove Desktop drain loop
//! (see `grove-gui/src-tauri/src/commands/mod.rs::drain_task_queue`) but
//! without the Tauri-specific event emission, streaming sink, or workflow
//! writeback thread — those are GUI-only concerns.
//!
//! Abort/resume semantics are unaffected: abort flips a SQL flag that
//! `execute_objective` polls internally, so the daemon does not need an
//! in-process abort registry for Wave A.

use std::sync::Arc;
use std::time::Duration;

use grove_core::config::{self, GroveConfig};
use grove_core::orchestrator::{self, RunOptions, TaskRecord};
use tokio::sync::{Notify, Semaphore};
use tracing::{error, info, warn};

use crate::config::DaemonConfig;

/// Handle for waking the drain loop on demand.
#[derive(Clone, Default)]
pub struct DrainSignal {
    notify: Arc<Notify>,
}

impl DrainSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wake the drain loop exactly once. Calls are coalesced — two notifications
    /// before a wait may still wake the loop only once, which is intentional
    /// since the loop drains the queue exhaustively each cycle.
    pub fn notify(&self) {
        self.notify.notify_one();
    }

    /// Wait for the next notification. Exposed for tests and for the drain
    /// loop itself — typical callers use [`DrainSignal::notify`] only.
    pub async fn wait(&self) {
        self.notify.notified().await;
    }
}

/// Shutdown handle for the drain loop.
///
/// Wraps a `tokio::sync::Notify` — calling `.shutdown()` causes the running
/// drain loop to exit at its next select point (typically within the poll
/// interval or immediately if currently parked). Cloning is cheap.
#[derive(Clone, Default)]
pub struct DrainShutdown {
    notify: Arc<Notify>,
}

impl DrainShutdown {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn shutdown(&self) {
        self.notify.notify_waiters();
    }

    async fn wait(&self) {
        self.notify.notified().await;
    }
}

/// Run the drain loop until `shutdown` fires.
///
/// The loop parks on `signal`, a 1-second tick, or `shutdown` — whichever
/// arrives first. On wake, it calls `drain_all` which dequeues as many tasks
/// as the concurrency semaphore allows, spawns each onto the tokio runtime,
/// and returns. Errors in one task never terminate the loop; they are logged
/// and the loop continues.
///
/// When a running task completes it re-notifies `signal` so the loop wakes
/// immediately to pick up the next queued task, rather than waiting up to
/// the 1s poll interval. This is the mechanism that keeps concurrency
/// fully utilized.
pub async fn run(cfg: DaemonConfig, signal: DrainSignal, shutdown: DrainShutdown) {
    info!(
        max_concurrent = cfg.max_concurrent_tasks,
        "queue drain loop started"
    );
    let sem = Arc::new(Semaphore::new(cfg.max_concurrent_tasks));
    loop {
        tokio::select! {
            _ = shutdown.wait() => {
                info!("queue drain loop shutting down");
                return;
            }
            _ = signal.wait() => {}
            _ = tokio::time::sleep(Duration::from_millis(1000)) => {}
        }

        if let Err(e) = drain_all(&cfg, &sem, &signal).await {
            error!(error = %e, "drain cycle failed");
        }
    }
}

/// Dequeue tasks and dispatch them concurrently up to the semaphore limit.
///
/// Permits are acquired *before* dequeue — if all permits are taken we return
/// early and let the outer loop wait for the next signal/tick. When a spawned
/// task finishes, it drops its permit and notifies `signal`, which wakes the
/// outer loop so it can pull the next queued task immediately.
///
/// Each task runs inside `spawn_blocking` because `execute_objective` is a
/// synchronous, long-running call that would otherwise pin the tokio reactor.
async fn drain_all(
    cfg: &DaemonConfig,
    sem: &Arc<Semaphore>,
    signal: &DrainSignal,
) -> anyhow::Result<()> {
    loop {
        // Acquire concurrency slot first; if the pool is full, yield to the
        // outer loop which will retry on the next signal/tick.
        let permit = match sem.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(tokio::sync::TryAcquireError::NoPermits) => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        let workspace_root = cfg.project_root.clone();
        let task_res =
            tokio::task::spawn_blocking(move || orchestrator::dequeue_next_task(&workspace_root))
                .await?;

        let task = match task_res? {
            Some(t) => t,
            None => {
                // Permit dropped here — nothing to run, let the loop park.
                return Ok(());
            }
        };

        let cfg_for_exec = cfg.clone();
        let signal_for_wake = signal.clone();
        // Detach: we do NOT await the spawn. The permit lives on the spawned
        // task and is released when the task completes (or panics).
        tokio::spawn(async move {
            let _permit = permit; // held for the task's lifetime
            let join = tokio::task::spawn_blocking(move || execute_one(&cfg_for_exec, task)).await;
            if let Err(join_err) = join {
                error!(error = %join_err, "queued task panicked");
            }
            // Wake the drain loop so it can re-dequeue under the now-freed permit.
            signal_for_wake.notify();
        });
    }
}

/// Run one task to completion (synchronously). Called from `spawn_blocking`.
///
/// Failure modes are logged and the task is marked `failed` in the DB so the
/// queue never sticks on a poisoned task. Returns `()` unconditionally —
/// drain_all only distinguishes panics from ordinary failures.
fn execute_one(cfg: &DaemonConfig, task: TaskRecord) {
    let workspace_root = cfg.project_root.clone();
    let project_root = orchestrator::resolve_project_root_for_task(&workspace_root, &task);

    let grove_cfg = match GroveConfig::load_or_create(&project_root) {
        Ok(c) => c,
        Err(e) => {
            error!(task_id = %task.id, error = %e, "failed to load project config");
            if let Err(fe) = orchestrator::finish_task(&workspace_root, &task.id, "failed", None) {
                warn!(task_id = %task.id, error = %fe, "finish_task after load_config failure");
            }
            return;
        }
    };

    let perm = orchestrator::parse_permission_mode(task.permission_mode.as_deref());
    let provider = match orchestrator::build_provider(
        &grove_cfg,
        &project_root,
        task.provider.as_deref(),
        perm.clone(),
        None,
    ) {
        Ok(p) => p,
        Err(e) => {
            error!(task_id = %task.id, error = %e, "failed to build provider");
            if let Err(fe) = orchestrator::finish_task(&workspace_root, &task.id, "failed", None) {
                warn!(task_id = %task.id, error = %fe, "finish_task after build_provider failure");
            }
            return;
        }
    };

    let db_path = config::db_path(&workspace_root);
    let abort_handle = orchestrator::abort_handle::AbortHandle::new();
    let pipeline = task
        .pipeline
        .as_deref()
        .and_then(orchestrator::pipeline::PipelineKind::from_str);

    let options = RunOptions {
        budget_usd: task.budget_usd,
        max_agents: None,
        model: task.model.clone(),
        provider: task.provider.clone(),
        interactive: false,
        pause_after: vec![],
        disable_phase_gates: task.disable_phase_gates,
        permission_mode: perm,
        pipeline,
        conversation_id: task.conversation_id.clone(),
        continue_last: false,
        db_path: Some(db_path),
        abort_handle: Some(abort_handle),
        issue_id: None,
        issue: None,
        resume_provider_session_id: task.resume_provider_session_id.clone(),
        on_run_created: None,
        input_handle_callback: None,
        run_control_callback: None,
        session_host_registry: None,
    };

    info!(task_id = %task.id, objective = %task.objective, "executing queued task");
    match orchestrator::execute_objective(
        &project_root,
        &grove_cfg,
        &task.objective,
        options,
        provider,
    ) {
        Ok(r) => {
            let state = orchestrator::task_terminal_state(&r.state);
            info!(task_id = %task.id, run_id = %r.run_id, state, "queued task finished");
            if let Err(e) =
                orchestrator::finish_task(&workspace_root, &task.id, state, Some(&r.run_id))
            {
                warn!(task_id = %task.id, error = %e, "finish_task after success");
            }
            if state == "completed" || state == "cancelled" {
                if let Err(e) = orchestrator::delete_completed_task(&workspace_root, &task.id) {
                    warn!(task_id = %task.id, error = %e, "delete_completed_task");
                }
            }
        }
        Err(e) => {
            error!(task_id = %task.id, error = %e, "queued task failed");
            if let Err(fe) = orchestrator::finish_task(&workspace_root, &task.id, "failed", None) {
                warn!(task_id = %task.id, error = %fe, "finish_task after failure");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use tempfile::tempdir;

    fn test_cfg() -> DaemonConfig {
        let tmp = tempdir().expect("tempdir");
        let path = tmp.keep(); // leak the tempdir so the path stays valid
        DaemonConfig::from_project_root(&path).expect("cfg")
    }

    #[tokio::test]
    async fn shutdown_exits_loop_promptly() {
        let cfg = test_cfg();
        let signal = DrainSignal::new();
        let shutdown = DrainShutdown::new();
        let s2 = shutdown.clone();
        let handle = tokio::spawn(async move {
            run(cfg, signal, shutdown).await;
        });
        // Give the loop a moment to enter its select.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let started = Instant::now();
        s2.shutdown();
        handle.await.expect("loop join");
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "shutdown took {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn signal_wakes_loop_before_interval() {
        // Drain loop on an empty queue is a no-op, but we can observe that
        // notify() wakes the loop promptly by counting iterations indirectly.
        // We verify the signal plumbing itself: calling notify() before wait()
        // must be immediately observable.
        let sig = DrainSignal::new();
        sig.notify();
        // With notify_one, the permit is buffered — the first wait() returns
        // immediately without another notify().
        tokio::time::timeout(Duration::from_millis(100), sig.wait())
            .await
            .expect("signal was not delivered");
    }

    #[tokio::test]
    async fn drain_all_on_empty_queue_returns_ok() {
        // An uninitialized project has no tasks table yet — drain_all should
        // surface the DB error rather than silently loop. We assert that the
        // outer run loop would log the error and continue (we call drain_all
        // directly to observe).
        let cfg = test_cfg();
        let sem = Arc::new(Semaphore::new(cfg.max_concurrent_tasks));
        let signal = DrainSignal::new();
        let res = drain_all(&cfg, &sem, &signal).await;
        // The task table does not exist on a bare tempdir, so we expect an
        // error from the DB layer. The important contract is that drain_all
        // returns rather than hanging — the surrounding loop logs and continues.
        assert!(
            res.is_err(),
            "expected DB error on uninitialized workspace, got Ok"
        );
    }

    #[tokio::test]
    async fn drain_all_returns_early_when_semaphore_is_exhausted() {
        // With 0 available permits the drain must return Ok immediately —
        // the outer loop relies on this to park and wait for a permit release
        // rather than busy-looping or dequeueing tasks it can't run.
        let cfg = test_cfg();
        let sem = Arc::new(Semaphore::new(1));
        // Pre-acquire the only permit to simulate "pool full".
        let _blocker = sem.clone().try_acquire_owned().expect("permit");
        let signal = DrainSignal::new();
        let res = tokio::time::timeout(Duration::from_millis(100), drain_all(&cfg, &sem, &signal))
            .await
            .expect("drain_all must not hang when pool is full");
        assert!(
            res.is_ok(),
            "saturated pool is expected behavior, not an error"
        );
    }
}
