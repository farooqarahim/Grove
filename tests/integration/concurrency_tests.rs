/// Concurrency test suite for grove-core.
///
/// Covers:
/// - Parallel run queuing (thread-safety of queue_task)
/// - Pool exhaustion (size-1 pool blocks and times out correctly)
/// - Writer queue burst (50+ threads writing events without corruption)
/// - Signal race conditions (concurrent send + check without loss or duplication)
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use grove_core::db;
use grove_core::db::DbPool;
use grove_core::events;
use grove_core::events::writer_queue::{PendingEvent, WriterQueue};
use grove_core::orchestrator;
use grove_core::signals::{self, SignalPriority, SignalType};
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn setup_db() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

/// Insert a minimal run row so signals and events can reference it.
fn insert_run(conn: &rusqlite::Connection, run_id: &str) {
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'test','executing',1.0,0.0,
                strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        [run_id],
    )
    .unwrap();
}

// ── Parallel run queuing ──────────────────────────────────────────────────────

/// Five threads each call queue_task on the same project root.
/// All must succeed and every returned task_id must be unique.
#[test]
fn parallel_queue_task_all_succeed() {
    let (dir, conn) = setup_db();
    drop(conn);

    let project_root = dir.path().to_path_buf();
    let results: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    std::thread::scope(|s| {
        for i in 0..5_usize {
            let root = project_root.clone();
            let results = Arc::clone(&results);
            s.spawn(move || {
                let task = orchestrator::queue_task(
                    &root,
                    &format!("parallel objective {i}"),
                    Some(1.0),
                    0,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    false,
                )
                .expect("queue_task must succeed in concurrent scenario");

                results.lock().unwrap().push(task.id);
            });
        }
    });

    let ids = results.lock().unwrap();
    assert_eq!(ids.len(), 5, "all 5 queue_task calls must succeed");

    let unique: HashSet<&String> = ids.iter().collect();
    assert_eq!(
        unique.len(),
        5,
        "all 5 task_ids must be unique; got duplicates: {ids:?}"
    );
}

/// Queue tasks from multiple threads while verifying tasks actually appear in DB.
#[test]
fn parallel_queue_task_tasks_visible_in_db() {
    let (dir, conn) = setup_db();
    drop(conn);

    let project_root = dir.path().to_path_buf();

    std::thread::scope(|s| {
        for i in 0..3_usize {
            let root = project_root.clone();
            s.spawn(move || {
                orchestrator::queue_task(
                    &root,
                    &format!("visibility task {i}"),
                    None,
                    i as i64,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    false,
                )
                .expect("queue_task must succeed");
            });
        }
    });

    // Verify tasks are persisted.
    let tasks = orchestrator::list_tasks(dir.path()).expect("list_tasks must succeed");
    assert_eq!(
        tasks.len(),
        3,
        "all 3 queued tasks must be visible in the database; got {}",
        tasks.len()
    );
}

/// Mixed-priority concurrent queuing preserves all records.
#[test]
fn parallel_queue_task_mixed_priorities_no_loss() {
    let (dir, conn) = setup_db();
    drop(conn);

    let project_root = dir.path().to_path_buf();
    let thread_count = 8_usize;

    std::thread::scope(|s| {
        for i in 0..thread_count {
            let root = project_root.clone();
            let priority = (i % 3) as i64; // 0, 1, 2 cycling
            s.spawn(move || {
                orchestrator::queue_task(
                    &root,
                    &format!("priority task {i}"),
                    Some(0.5),
                    priority,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    false,
                )
                .expect("queue_task with varied priority must succeed");
            });
        }
    });

    let tasks = orchestrator::list_tasks(dir.path()).expect("list_tasks must succeed");
    assert_eq!(
        tasks.len(),
        thread_count,
        "all {thread_count} tasks must be stored; got {}",
        tasks.len()
    );
}

// ── Pool exhaustion ───────────────────────────────────────────────────────────

/// A size-1 pool holds one connection; a second concurrent checkout must fail
/// (not hang forever) because we configure a short 100 ms timeout and hold the
/// first connection for the duration of the test.
#[test]
fn pool_exhaustion_size_one_second_checkout_errors() {
    let dir = TempDir::new().unwrap();
    // Initialize the DB so PRAGMAs applied by PragmaInitializer succeed.
    db::initialize(dir.path()).unwrap();
    let db_path = db::db_path(dir.path());

    // Use a 1 s timeout: long enough for lazy connection establishment under
    // parallel test load, still short enough for exhaustion to surface quickly.
    let pool = DbPool::new(&db_path, 1, 1000).expect("pool creation must succeed");

    // Hold the one available connection for the duration of the test.
    let _held = pool.get().expect("first checkout must succeed");

    // A second checkout on a size-1 pool must timeout and return an error.
    let result = pool.get();
    assert!(
        result.is_err(),
        "second checkout on exhausted size-1 pool must return an error, not hang"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("pool exhausted"),
        "exhaustion error must mention 'pool exhausted', got: {err_msg}"
    );
}

/// A size-2 pool can serve exactly 2 concurrent checkouts without blocking;
/// a third must fail after the timeout.
#[test]
fn pool_exhaustion_size_two_third_checkout_errors() {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let db_path = db::db_path(dir.path());

    // Use a 1 s timeout: long enough for lazy connection establishment under
    // parallel test load, still short enough for exhaustion to surface quickly.
    let pool = DbPool::new(&db_path, 2, 1000).expect("pool creation must succeed");

    let _conn1 = pool.get().expect("first checkout must succeed");
    let _conn2 = pool.get().expect("second checkout must succeed");

    // Third checkout exceeds pool capacity.
    let result = pool.get();
    assert!(
        result.is_err(),
        "third checkout on exhausted size-2 pool must return an error"
    );
}

// ── Writer queue burst ────────────────────────────────────────────────────────

/// 50 threads each flush 10 events through WriterQueue.
/// Total event count must equal 500 with no corruption or data loss.
#[test]
fn writer_queue_burst_50_threads_no_loss() {
    let (dir, conn) = setup_db();
    let run_id = "run_burst_50";
    insert_run(&conn, run_id);
    drop(conn);

    let db_path = db::db_path(dir.path());
    let thread_count = 50_usize;
    let events_per_thread = 10_usize;

    std::thread::scope(|s| {
        for thread_id in 0..thread_count {
            let path = db_path.clone();
            let rid = run_id.to_string();
            s.spawn(move || {
                let mut conn =
                    grove_core::db::connection::open(&path).expect("connection must open");
                let mut wq = WriterQueue::new();
                for event_idx in 0..events_per_thread {
                    wq.push(PendingEvent {
                        run_id: rid.clone(),
                        session_id: None,
                        event_type: format!("burst_t{thread_id}_e{event_idx}"),
                        payload_json: "{}".to_string(),
                        created_at: chrono::Utc::now().to_rfc3339(),
                    });
                }
                wq.flush(&mut conn)
                    .expect("WriterQueue flush must succeed under burst load");
            });
        }
    });

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let all_events = events::list_for_run(&conn, run_id).unwrap();
    let expected = thread_count * events_per_thread;
    assert_eq!(
        all_events.len(),
        expected,
        "expected {expected} events from {thread_count}×{events_per_thread} burst; got {}",
        all_events.len()
    );
}

/// 60 threads emit events directly (not via WriterQueue).
/// Verifies direct emit path is also safe under high concurrency.
#[test]
fn direct_emit_burst_60_threads_no_corruption() {
    let (dir, conn) = setup_db();
    let run_id = "run_direct_60";
    insert_run(&conn, run_id);
    drop(conn);

    let db_path = db::db_path(dir.path());
    let thread_count = 60_usize;
    let events_per_thread = 5_usize;

    std::thread::scope(|s| {
        for thread_id in 0..thread_count {
            let path = db_path.clone();
            let rid = run_id.to_string();
            s.spawn(move || {
                let conn = grove_core::db::connection::open(&path).expect("connection must open");
                for event_idx in 0..events_per_thread {
                    events::emit(
                        &conn,
                        &rid,
                        None,
                        &format!("direct_t{thread_id}_e{event_idx}"),
                        serde_json::json!({"thread": thread_id, "idx": event_idx}),
                    )
                    .expect("direct emit must not fail under burst load");
                }
            });
        }
    });

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let all_events = events::list_for_run(&conn, run_id).unwrap();
    let expected = thread_count * events_per_thread;
    assert_eq!(
        all_events.len(),
        expected,
        "expected {expected} events from {thread_count}×{events_per_thread} direct burst; got {}",
        all_events.len()
    );
}

// ── Signal race conditions ────────────────────────────────────────────────────

/// 20 threads each send 5 signals concurrently to the same recipient.
/// After all threads complete, check_signals must return all 100 signals —
/// none lost and none duplicated.
#[test]
fn concurrent_signal_send_no_loss_no_duplication() {
    let (dir, conn) = setup_db();
    let run_id = "run_sig_race";
    insert_run(&conn, run_id);
    drop(conn);

    let db_path = db::db_path(dir.path());
    let sender_count = 20_usize;
    let signals_per_sender = 5_usize;

    std::thread::scope(|s| {
        for sender_idx in 0..sender_count {
            let path = db_path.clone();
            let rid = run_id.to_string();
            s.spawn(move || {
                let conn = grove_core::db::connection::open(&path).expect("connection must open");
                for sig_idx in 0..signals_per_sender {
                    signals::send_signal(
                        &conn,
                        &rid,
                        &format!("agent_{sender_idx}"),
                        "coordinator",
                        SignalType::Status,
                        SignalPriority::Normal,
                        serde_json::json!({
                            "sender": sender_idx,
                            "signal": sig_idx
                        }),
                    )
                    .expect("send_signal must not fail under concurrent load");
                }
            });
        }
    });

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let received = signals::check_signals(&conn, run_id, "coordinator").unwrap();
    let expected = sender_count * signals_per_sender;
    assert_eq!(
        received.len(),
        expected,
        "expected {expected} signals from {sender_count}×{signals_per_sender} senders; got {}",
        received.len()
    );

    // All signal IDs must be unique — no duplicates inserted.
    let unique_ids: HashSet<&str> = received.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        unique_ids.len(),
        expected,
        "all {expected} signal IDs must be unique; found duplicates"
    );
}

/// Interleaved concurrent send and check calls on the same run/agent.
/// Verifies that check_signals never panics and returns a consistent subset
/// of the signals sent so far.
#[test]
fn concurrent_send_and_check_no_panic() {
    let (dir, conn) = setup_db();
    let run_id = "run_interleave";
    insert_run(&conn, run_id);
    drop(conn);

    let db_path = db::db_path(dir.path());
    let error_seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    std::thread::scope(|s| {
        // 10 sender threads.
        for sender_idx in 0..10_usize {
            let path = db_path.clone();
            let rid = run_id.to_string();
            let err_flag = Arc::clone(&error_seen);
            s.spawn(move || {
                let conn = match grove_core::db::connection::open(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        *err_flag.lock().unwrap() =
                            Some(format!("open failed in sender {sender_idx}: {e}"));
                        return;
                    }
                };
                for sig_idx in 0..5_usize {
                    if let Err(e) = signals::send_signal(
                        &conn,
                        &rid,
                        &format!("worker_{sender_idx}"),
                        "monitor",
                        SignalType::WorkerDone,
                        SignalPriority::Low,
                        serde_json::json!({"worker": sender_idx, "step": sig_idx}),
                    ) {
                        *err_flag.lock().unwrap() = Some(format!("send_signal error: {e}"));
                    }
                }
            });
        }

        // 5 reader threads that call check_signals while senders are running.
        for reader_idx in 0..5_usize {
            let path = db_path.clone();
            let rid = run_id.to_string();
            let err_flag = Arc::clone(&error_seen);
            s.spawn(move || {
                let conn = match grove_core::db::connection::open(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        *err_flag.lock().unwrap() =
                            Some(format!("open failed in reader {reader_idx}: {e}"));
                        return;
                    }
                };
                // May observe any count [0, 50]; must not panic or error.
                if let Err(e) = signals::check_signals(&conn, &rid, "monitor") {
                    *err_flag.lock().unwrap() =
                        Some(format!("check_signals error in reader {reader_idx}: {e}"));
                }
            });
        }
    });

    // Fail with a descriptive message if any thread recorded an error.
    let guard = error_seen.lock().unwrap();
    assert!(
        guard.is_none(),
        "concurrent send+check must not produce errors: {:?}",
        guard.as_deref()
    );

    // After all threads finish, all 50 signals must be present.
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let all = signals::list_for_run(&conn, run_id).unwrap();
    assert_eq!(
        all.len(),
        50,
        "all 50 signals must be stored after interleaved send+check; got {}",
        all.len()
    );
}
