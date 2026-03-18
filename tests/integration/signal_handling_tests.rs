/// Integration tests for signal handling and graceful shutdown.
///
/// Covers:
/// - OS-level SIGINT / SIGTERM sent to child processes (unix-only)
/// - AbortHandle: flag propagation, PID registration, RAII guard cleanup
/// - abort_gracefully: state transition, ownership release, checkpoint creation
/// - Grove inter-agent signal infrastructure: send, check, mark-read, broadcast
/// - Concurrent send + check operations (thread-safety)
///
/// All OS-signal tests are gated on `#[cfg(unix)]`.
/// Slow tests (those that actually wait for OS scheduling) carry `#[ignore]`.
use chrono::Utc;
use grove_core::checkpoint;
use grove_core::db;
use grove_core::db::repositories::ownership_repo;
use grove_core::orchestrator::RunState;
use grove_core::orchestrator::abort::abort_gracefully;
use grove_core::orchestrator::abort_handle::AbortHandle;
use grove_core::signals::{self, GROUP_ALL, SignalPriority, SignalType};
use rusqlite::{Connection, params};
use tempfile::TempDir;

// ── Test helpers ─────────────────────────────────────────────────────────────

fn setup_db() -> (TempDir, Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

fn insert_run(conn: &Connection, run_id: &str) {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
         VALUES (?1, 'test objective', 'executing', 10.0, 0.0, ?2, ?2)",
        params![run_id, now],
    )
    .unwrap();
}

fn insert_running_session(conn: &Connection, session_id: &str, run_id: &str, agent_type: &str) {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, run_id, agent_type, state, worktree_path, started_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'running', '/tmp/wt', ?4, ?4, ?4)",
        params![session_id, run_id, agent_type, now],
    )
    .unwrap();
}

fn insert_ownership_lock(conn: &Connection, run_id: &str, session_id: &str, path: &str) {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO ownership_locks (run_id, path, owner_session_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![run_id, path, session_id, now],
    )
    .unwrap();
}

// ── AbortHandle unit tests ────────────────────────────────────────────────────

/// A freshly created handle is not aborted.
#[test]
fn abort_handle_initial_state_is_not_aborted() {
    let handle = AbortHandle::new();
    assert!(
        !handle.is_aborted(),
        "new AbortHandle must start in non-aborted state"
    );
}

/// Calling abort() sets the flag to true.
#[test]
fn abort_handle_abort_sets_flag() {
    let handle = AbortHandle::new();
    handle.abort();
    assert!(
        handle.is_aborted(),
        "abort() must set the aborted flag to true"
    );
}

/// Clones share the same atomic flag — aborting one view aborts all.
#[test]
fn abort_handle_clone_shares_abort_state() {
    let h1 = AbortHandle::new();
    let h2 = h1.clone();
    let h3 = h1.clone();

    assert!(!h2.is_aborted());
    h1.abort();
    assert!(h2.is_aborted(), "clone must observe abort set on original");
    assert!(h3.is_aborted(), "second clone must also observe abort");
}

/// PidGuard registers a PID while alive and unregisters it on drop.
#[test]
fn abort_handle_pid_guard_raii() {
    let handle = AbortHandle::new();

    {
        let _guard = handle.register_pid(99_999);
        // While the guard is alive the PID is tracked; we cannot directly
        // inspect the pids field from outside the module, but we can verify
        // that a second abort() call doesn't panic with a bad PID list.
        handle.abort();
        // abort() was called while PID 99_999 is registered.
        // The OS likely has no process with that PID, so kill returns an error
        // which abort() silently ignores — we just verify no panic occurs.
    }
    // Guard dropped: PID removed.  A second abort() on a different handle
    // (same Arc state) would also be safe — but we've already set the flag.
    assert!(handle.is_aborted());
}

/// Registering multiple PIDs and dropping them individually leaves correct state.
#[test]
fn abort_handle_multiple_pids_drop_order() {
    let handle = AbortHandle::new();
    let g1 = handle.register_pid(1001);
    let g2 = handle.register_pid(1002);
    let g3 = handle.register_pid(1003);

    // Drop in non-registration order.
    drop(g2);
    drop(g1);
    // g3 still alive; abort won't panic.
    handle.abort();
    drop(g3);
    // Post-drop, handle is still usable.
    assert!(handle.is_aborted());
}

/// Concurrent abort flag reads from multiple threads are consistent.
#[test]
fn abort_handle_concurrent_abort_flag_consistency() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let handle = Arc::new(AbortHandle::new());
    let barrier = Arc::new(Barrier::new(5));

    let readers: Vec<_> = (0..4)
        .map(|_| {
            let h = Arc::clone(&handle);
            let b = Arc::clone(&barrier);
            thread::spawn(move || {
                b.wait();
                // Spin-read for up to 200 iterations.
                for _ in 0..200 {
                    let _ = h.is_aborted();
                }
            })
        })
        .collect();

    // Trigger abort after all reader threads are ready.
    {
        let h = Arc::clone(&handle);
        let b = Arc::clone(&barrier);
        let writer = thread::spawn(move || {
            b.wait();
            h.abort();
        });
        writer.join().unwrap();
    }

    for r in readers {
        r.join().unwrap();
    }

    assert!(
        handle.is_aborted(),
        "flag must be set after concurrent abort"
    );
}

// ── abort_gracefully integration tests ───────────────────────────────────────

/// abort_gracefully transitions run to Paused state.
#[test]
fn abort_gracefully_transitions_run_to_paused() {
    let (_dir, conn) = setup_db();
    let run_id = "run_graceful_1";
    insert_run(&conn, run_id);

    abort_gracefully(
        &conn,
        run_id,
        "build the feature",
        10.0,
        RunState::Executing,
    )
    .unwrap();

    let state: String = conn
        .query_row("SELECT state FROM runs WHERE id = ?1", [run_id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        state, "paused",
        "abort_gracefully must transition run to paused"
    );
}

/// abort_gracefully releases all ownership locks held by the run.
#[test]
fn abort_gracefully_releases_all_ownership_locks() {
    let (_dir, conn) = setup_db();
    let run_id = "run_graceful_2";
    insert_run(&conn, run_id);
    insert_running_session(&conn, "sess_g1", run_id, "builder");
    insert_running_session(&conn, "sess_g2", run_id, "tester");
    insert_ownership_lock(&conn, run_id, "sess_g1", "src/main.rs");
    insert_ownership_lock(&conn, run_id, "sess_g2", "src/lib.rs");

    // Verify locks exist before abort.
    let locks_before = ownership_repo::list_all(&conn).unwrap();
    let run_locks_before: Vec<_> = locks_before.iter().filter(|l| l.run_id == run_id).collect();
    assert_eq!(run_locks_before.len(), 2, "should start with 2 locks");

    abort_gracefully(&conn, run_id, "objective", 10.0, RunState::Executing).unwrap();

    let locks_after = ownership_repo::list_all(&conn).unwrap();
    let run_locks_after: Vec<_> = locks_after.iter().filter(|l| l.run_id == run_id).collect();
    assert!(
        run_locks_after.is_empty(),
        "abort_gracefully must release all ownership locks for the run"
    );
}

/// abort_gracefully creates a checkpoint so the run can be resumed.
#[test]
fn abort_gracefully_creates_resumable_checkpoint() {
    let (_dir, conn) = setup_db();
    let run_id = "run_graceful_3";
    insert_run(&conn, run_id);

    abort_gracefully(
        &conn,
        run_id,
        "implement feature X",
        5.0,
        RunState::Executing,
    )
    .unwrap();

    let checkpoint = checkpoint::latest_for_run(&conn, run_id).unwrap();
    assert!(
        checkpoint.is_some(),
        "abort_gracefully must persist a checkpoint"
    );
    let cp = checkpoint.unwrap();
    assert_eq!(cp.run_id, run_id);
    assert_eq!(cp.stage, "paused");
    assert!(
        cp.pending_tasks
            .contains(&"implement feature X".to_string()),
        "checkpoint pending_tasks must contain the original objective"
    );
}

/// abort_gracefully emits a run_aborted event.
#[test]
fn abort_gracefully_emits_run_aborted_event() {
    let (_dir, conn) = setup_db();
    let run_id = "run_graceful_4";
    insert_run(&conn, run_id);

    abort_gracefully(&conn, run_id, "objective", 5.0, RunState::Executing).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE run_id = ?1 AND type = 'run_aborted'",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "exactly one run_aborted event must be emitted");
}

/// abort_gracefully is idempotent when called twice (Paused → Paused is a no-op
/// transition; the second call should not panic or corrupt state).
#[test]
fn abort_gracefully_is_safe_on_already_paused_run() {
    let (_dir, conn) = setup_db();
    let run_id = "run_graceful_5";
    insert_run(&conn, run_id);

    // First abort: valid transition Executing → Paused.
    abort_gracefully(&conn, run_id, "obj", 5.0, RunState::Executing).unwrap();

    // Second abort: from Paused. The transition layer may return an error
    // (invalid transition), but it must not panic and must not corrupt state.
    let result = abort_gracefully(&conn, run_id, "obj", 5.0, RunState::Paused);
    // Either Ok (idempotent) or Err (invalid transition) is acceptable.
    // Panic is NOT acceptable — we just verify the run is still readable.
    let state: String = conn
        .query_row("SELECT state FROM runs WHERE id = ?1", [run_id], |r| {
            r.get(0)
        })
        .unwrap();
    // State should not be corrupted regardless of whether the second abort
    // succeeded or returned an error.
    assert!(
        state == "paused" || result.is_err(),
        "run state must remain stable after double abort: state={state}"
    );
}

// ── Grove inter-agent signal tests ───────────────────────────────────────────

/// Sending a signal and immediately checking it returns the expected signal.
#[test]
fn signal_send_and_check_roundtrip() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_rt_1";
    insert_run(&conn, run_id);

    let id = signals::send_signal(
        &conn,
        run_id,
        "orchestrator",
        "builder",
        SignalType::Dispatch,
        SignalPriority::High,
        serde_json::json!({"task": "implement login"}),
    )
    .unwrap();

    assert!(id.starts_with("sig_"), "signal ID must have sig_ prefix");

    let received = signals::check_signals(&conn, run_id, "builder").unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].id, id);
    assert_eq!(received[0].from_agent, "orchestrator");
    assert_eq!(received[0].to_agent, "builder");
    assert_eq!(received[0].signal_type, "dispatch");
    assert_eq!(received[0].priority, "high");
    assert!(!received[0].read, "new signal must be unread");
}

/// Marking a signal as read removes it from the unread queue.
#[test]
fn signal_mark_read_hides_from_check() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_mr_1";
    insert_run(&conn, run_id);

    let id = signals::send_signal(
        &conn,
        run_id,
        "tester",
        "orchestrator",
        SignalType::TestResult,
        SignalPriority::Normal,
        serde_json::json!({"passed": true}),
    )
    .unwrap();

    signals::mark_read(&conn, &id).unwrap();

    let unread = signals::check_signals(&conn, run_id, "orchestrator").unwrap();
    assert!(
        unread.is_empty(),
        "read signal must not appear in check_signals"
    );

    // list_for_run still shows it.
    let all = signals::list_for_run(&conn, run_id).unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].read, "signal must be marked read in list_for_run");
}

/// check_signals is scoped to the addressed agent — other agents see nothing.
#[test]
fn signal_check_only_returns_addressed_agent_signals() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_scope_1";
    insert_run(&conn, run_id);

    signals::send_signal(
        &conn,
        run_id,
        "a",
        "builder",
        SignalType::Status,
        SignalPriority::Normal,
        serde_json::json!({}),
    )
    .unwrap();
    signals::send_signal(
        &conn,
        run_id,
        "a",
        "reviewer",
        SignalType::Status,
        SignalPriority::Normal,
        serde_json::json!({}),
    )
    .unwrap();

    let for_builder = signals::check_signals(&conn, run_id, "builder").unwrap();
    let for_reviewer = signals::check_signals(&conn, run_id, "reviewer").unwrap();
    let for_tester = signals::check_signals(&conn, run_id, "tester").unwrap();

    assert_eq!(for_builder.len(), 1, "builder gets exactly its signal");
    assert_eq!(for_reviewer.len(), 1, "reviewer gets exactly its signal");
    assert!(for_tester.is_empty(), "tester gets no signals");
}

/// Multiple signals accumulate in arrival order.
#[test]
fn signal_multiple_signals_ordered_by_creation() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_ord_1";
    insert_run(&conn, run_id);

    for i in 0..5_u32 {
        signals::send_signal(
            &conn,
            run_id,
            "producer",
            "consumer",
            SignalType::Status,
            SignalPriority::Normal,
            serde_json::json!({"seq": i}),
        )
        .unwrap();
    }

    let received = signals::check_signals(&conn, run_id, "consumer").unwrap();
    assert_eq!(received.len(), 5, "all 5 signals must be queued");

    for (i, sig) in received.iter().enumerate() {
        let seq = sig.payload["seq"].as_u64().unwrap();
        assert_eq!(seq, i as u64, "signals must be returned in insertion order");
    }
}

/// Broadcast sends to all running agents except the sender.
#[test]
fn signal_broadcast_excludes_sender_reaches_others() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_bc_1";
    insert_run(&conn, run_id);
    insert_running_session(&conn, "sess_arch", run_id, "architect");
    insert_running_session(&conn, "sess_bld1", run_id, "builder");
    insert_running_session(&conn, "sess_bld2", run_id, "builder");
    // Duplicate agent_type — expand_group uses DISTINCT so still one "builder" target.

    let ids = signals::broadcast(
        &conn,
        run_id,
        "architect",
        GROUP_ALL,
        SignalType::DesignReady,
        SignalPriority::High,
        serde_json::json!({"phase": "v1"}),
    )
    .unwrap();

    // architect is excluded; builder is the only other distinct agent_type.
    assert_eq!(ids.len(), 1, "broadcast to @all must exclude sender");
    let builder_signals = signals::check_signals(&conn, run_id, "builder").unwrap();
    assert_eq!(builder_signals.len(), 1);
    assert_eq!(builder_signals[0].signal_type, "design_ready");
}

/// Broadcast to a group with no other agents produces zero signals.
#[test]
fn signal_broadcast_with_only_sender_yields_empty() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_bc_2";
    insert_run(&conn, run_id);
    insert_running_session(&conn, "sess_solo", run_id, "builder");

    let ids = signals::broadcast(
        &conn,
        run_id,
        "builder",
        GROUP_ALL,
        SignalType::WorkerDone,
        SignalPriority::Normal,
        serde_json::json!({}),
    )
    .unwrap();

    assert!(
        ids.is_empty(),
        "sole agent broadcasting to itself must yield no signals"
    );
}

/// list_for_run returns all signals (read and unread) for a run.
#[test]
fn signal_list_for_run_includes_read_and_unread() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_list_1";
    insert_run(&conn, run_id);

    let id1 = signals::send_signal(
        &conn,
        run_id,
        "a",
        "b",
        SignalType::Result,
        SignalPriority::Normal,
        serde_json::json!({}),
    )
    .unwrap();
    let _id2 = signals::send_signal(
        &conn,
        run_id,
        "a",
        "b",
        SignalType::Error,
        SignalPriority::Urgent,
        serde_json::json!({}),
    )
    .unwrap();

    // Mark the first one read.
    signals::mark_read(&conn, &id1).unwrap();

    let all = signals::list_for_run(&conn, run_id).unwrap();
    assert_eq!(all.len(), 2, "list_for_run must return all signals");
    let read_count = all.iter().filter(|s| s.read).count();
    let unread_count = all.iter().filter(|s| !s.read).count();
    assert_eq!(read_count, 1);
    assert_eq!(unread_count, 1);
}

/// SignalType as_str / parse roundtrip is exhaustive.
#[test]
fn signal_type_roundtrip_all_variants() {
    let variants = [
        (SignalType::Status, "status"),
        (SignalType::Question, "question"),
        (SignalType::Result, "result"),
        (SignalType::Error, "error"),
        (SignalType::WorkerDone, "worker_done"),
        (SignalType::MergeReady, "merge_ready"),
        (SignalType::Escalation, "escalation"),
        (SignalType::Dispatch, "dispatch"),
        (SignalType::BudgetWarning, "budget_warning"),
        (SignalType::DesignReady, "design_ready"),
        (SignalType::TestResult, "test_result"),
        (SignalType::ReviewResult, "review_result"),
    ];

    for (variant, expected_str) in &variants {
        assert_eq!(
            variant.as_str(),
            *expected_str,
            "{variant:?}.as_str() must equal {expected_str}"
        );
        assert_eq!(
            SignalType::parse(expected_str),
            Some(*variant),
            "SignalType::parse({expected_str}) must recover {variant:?}"
        );
    }
    assert_eq!(
        SignalType::parse("not_a_signal"),
        None,
        "unknown string must parse to None"
    );
}

/// SignalPriority as_str / parse roundtrip.
#[test]
fn signal_priority_roundtrip_all_variants() {
    use grove_core::signals::SignalPriority;

    let cases = [
        (SignalPriority::Low, "low"),
        (SignalPriority::Normal, "normal"),
        (SignalPriority::High, "high"),
        (SignalPriority::Urgent, "urgent"),
    ];
    for (variant, s) in &cases {
        assert_eq!(variant.as_str(), *s);
        assert_eq!(SignalPriority::parse(s), *variant);
    }
    // Unknown strings fall back to Normal.
    assert_eq!(SignalPriority::parse("whatever"), SignalPriority::Normal);
}

// ── Concurrent signal send + check (thread safety) ───────────────────────────

/// Many threads send signals concurrently; all must be persisted without error.
#[test]
fn signal_concurrent_sends_all_persisted() {
    use std::sync::{Arc, Mutex};
    use std::thread;

    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let db_path = Arc::new(dir.path().to_path_buf());
    let run_id = "run_sig_conc_1";

    // Insert run on the main thread.
    {
        let conn = db::DbHandle::new(&*db_path).connect().unwrap();
        insert_run(&conn, run_id);
    }

    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let thread_count = 8;
    let signals_per_thread = 5;

    let handles: Vec<_> = (0..thread_count)
        .map(|t| {
            let path = Arc::clone(&db_path);
            let errs = Arc::clone(&errors);
            thread::spawn(move || {
                let conn = db::DbHandle::new(&*path).connect().unwrap();
                for i in 0..signals_per_thread {
                    let from = format!("producer_{t}");
                    let result = signals::send_signal(
                        &conn,
                        run_id,
                        &from,
                        "consumer",
                        SignalType::Status,
                        SignalPriority::Normal,
                        serde_json::json!({"thread": t, "seq": i}),
                    );
                    if let Err(e) = result {
                        errs.lock().unwrap().push(format!("t={t} i={i}: {e}"));
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("worker thread must not panic");
    }

    let errs = errors.lock().unwrap();
    assert!(
        errs.is_empty(),
        "concurrent sends must not produce errors: {errs:?}"
    );

    // Verify all signals landed.
    let conn = db::DbHandle::new(&*db_path).connect().unwrap();
    let all = signals::check_signals(&conn, run_id, "consumer").unwrap();
    assert_eq!(
        all.len(),
        thread_count * signals_per_thread,
        "all concurrent signals must be persisted"
    );
}

// ── OS-level signal tests (unix only) ────────────────────────────────────────

/// Spawning a sleep process and sending SIGINT causes it to terminate.
///
/// This test is marked `#[ignore]` because it introduces actual OS scheduling
/// latency. Run with: `cargo test --test signal_handling_tests -- --ignored`
#[cfg(unix)]
#[test]
#[ignore]
fn os_sigint_terminates_child_process() {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let mut child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("failed to spawn sleep process");

    let pid = child.id();

    // Give the process time to start.
    thread::sleep(Duration::from_millis(100));

    // Send SIGINT.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGINT);
    }

    // Wait with timeout — the process should have exited.
    let status = child.wait().expect("failed to wait on child");
    assert!(
        !status.success(),
        "SIGINT must cause child process to exit non-zero"
    );

    // Verify the process is no longer alive (kill -0 returns error).
    let still_alive = unsafe { libc::kill(pid as libc::pid_t, 0) == 0 };
    assert!(
        !still_alive,
        "process must not be running after SIGINT + wait"
    );
}

/// Spawning a sleep process and sending SIGTERM causes graceful termination.
///
/// Marked `#[ignore]` — introduces OS scheduling latency.
#[cfg(unix)]
#[test]
#[ignore]
fn os_sigterm_terminates_child_process() {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let mut child = Command::new("sleep")
        .arg("60")
        .spawn()
        .expect("failed to spawn sleep process");

    let pid = child.id();

    thread::sleep(Duration::from_millis(100));

    // Send SIGTERM.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }

    let status = child.wait().expect("failed to wait on child");
    assert!(
        !status.success(),
        "SIGTERM must cause child process to exit non-zero"
    );

    let still_alive = unsafe { libc::kill(pid as libc::pid_t, 0) == 0 };
    assert!(
        !still_alive,
        "process must not be alive after SIGTERM + wait"
    );
}

/// AbortHandle.abort() sends SIGTERM to a real registered child process and
/// verifies the process is no longer alive after the grace period.
///
/// Marked `#[ignore]` because abort() includes a 5-second sleep for the grace
/// period.  Run explicitly with: `cargo test -- --ignored`
#[cfg(unix)]
#[test]
#[ignore]
fn abort_handle_kills_registered_child_process() {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let mut child = Command::new("sleep")
        .arg("300")
        .spawn()
        .expect("failed to spawn sleep process");

    let pid = child.id();
    let handle = AbortHandle::new();
    let _guard = handle.register_pid(pid);

    thread::sleep(Duration::from_millis(100));

    // abort() sends SIGTERM, waits 5 s, then SIGKILLs survivors.
    handle.abort();

    // After abort() the process should be gone — reap it.
    let _ = child.try_wait();

    let still_alive = unsafe { libc::kill(pid as libc::pid_t, 0) == 0 };
    assert!(
        !still_alive,
        "child process must be dead after AbortHandle.abort()"
    );
}

/// Verifies that a process killed via SIGKILL does not become a zombie when
/// properly waited on.
///
/// Marked `#[ignore]` — involves OS-level process management.
#[cfg(unix)]
#[test]
#[ignore]
fn killed_child_process_does_not_become_zombie() {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let mut child = Command::new("sleep")
        .arg("300")
        .spawn()
        .expect("failed to spawn sleep process");

    let pid = child.id();

    thread::sleep(Duration::from_millis(100));

    // Send SIGKILL directly.
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }

    // Reap the child — this prevents zombie state.
    let status = child.wait().expect("wait must succeed after SIGKILL");
    assert!(
        !status.success(),
        "SIGKILLed process must not exit successfully"
    );

    // Verify no zombie: kill -0 must fail (process no longer exists).
    let still_alive = unsafe { libc::kill(pid as libc::pid_t, 0) == 0 };
    assert!(!still_alive, "reaped process must not appear as zombie");
}

/// DB remains queryable (not corrupted) after abort_gracefully simulates a
/// SIGTERM-triggered shutdown mid-operation.
#[test]
fn db_integrity_preserved_after_graceful_abort() {
    let (_dir, conn) = setup_db();
    let run_id = "run_integrity_1";
    insert_run(&conn, run_id);
    insert_running_session(&conn, "sess_int1", run_id, "builder");
    insert_ownership_lock(&conn, run_id, "sess_int1", "src/core.rs");

    // Simulate signal-triggered graceful shutdown.
    abort_gracefully(
        &conn,
        run_id,
        "build core feature",
        20.0,
        RunState::Executing,
    )
    .unwrap();

    // DB must still be queryable — verify key tables are intact.
    let run_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM runs WHERE id = ?1", [run_id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(run_count, 1, "runs table must remain intact after abort");

    let event_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE run_id = ?1 AND type = 'run_aborted'",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        event_count > 0,
        "events table must contain entries after abort"
    );

    let checkpoint_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM checkpoints WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        checkpoint_count, 1,
        "checkpoints table must contain the abort checkpoint"
    );
}
