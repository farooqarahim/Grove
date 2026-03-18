/// Integration tests for the new orchestrator public API wrappers added in the
/// CLI-commands plan:
///   - cancel_task
///   - list_sessions
///   - list_ownership_locks
///   - list_merge_queue
///   - run_events_all
///   - ownership_repo::list_for_run (underlying targeted query)
use grove_core::db;
use grove_core::db::repositories::ownership_repo;
use grove_core::events;
use grove_core::orchestrator;
use rusqlite::params;
use serde_json::json;
use tempfile::TempDir;

// ── shared helpers ─────────────────────────────────────────────────────────────

fn setup() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

fn insert_run(conn: &rusqlite::Connection, run_id: &str) {
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'test','completed',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [run_id],
    )
    .unwrap();
}

fn insert_session(conn: &rusqlite::Connection, session_id: &str, run_id: &str) {
    conn.execute(
        "INSERT INTO sessions(id,run_id,agent_type,state,worktree_path,created_at,updated_at)
         VALUES(?1,?2,'builder','completed','/tmp/wt','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        params![session_id, run_id],
    )
    .unwrap();
}

fn insert_ownership_lock(conn: &rusqlite::Connection, run_id: &str, path: &str, session_id: &str) {
    // ownership_locks.owner_session_id FK → sessions.id; ensure the session exists first.
    conn.execute(
        "INSERT OR IGNORE INTO sessions(id,run_id,agent_type,state,worktree_path,created_at,updated_at)
         VALUES(?1,?2,'builder','completed','/tmp/wt','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        params![session_id, run_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO ownership_locks(run_id,path,owner_session_id,created_at)
         VALUES(?1,?2,?3,'2024-01-01T00:00:00Z')",
        params![run_id, path, session_id],
    )
    .unwrap();
}

// ── cancel_task ───────────────────────────────────────────────────────────────

#[test]
fn cancel_task_queued_sets_state_to_cancelled() {
    let (dir, conn) = setup();
    drop(conn);
    let task = orchestrator::queue_task(
        dir.path(),
        "do something",
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    )
    .unwrap();

    orchestrator::cancel_task(dir.path(), &task.id).unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let state: String = conn
        .query_row("SELECT state FROM tasks WHERE id=?1", [&task.id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(state, "cancelled");
}

#[test]
fn cancel_task_nonexistent_id_returns_error() {
    let (dir, conn) = setup();
    drop(conn);
    let result = orchestrator::cancel_task(dir.path(), "task_nonexistent_xyz");
    assert!(result.is_err(), "expected error for nonexistent task");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not found") || msg.contains("queued"),
        "unexpected error message: {msg}"
    );
}

#[test]
fn cancel_task_running_task_returns_error() {
    let (dir, conn) = setup();
    conn.execute(
        "INSERT INTO tasks(id,objective,state,priority,queued_at)
         VALUES('task_already_running','test','running',0,'2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    drop(conn);

    let result = orchestrator::cancel_task(dir.path(), "task_already_running");
    assert!(
        result.is_err(),
        "expected error when cancelling a running task"
    );
}

#[test]
fn cancel_task_completed_task_returns_error() {
    let (dir, conn) = setup();
    conn.execute(
        "INSERT INTO tasks(id,objective,state,priority,queued_at)
         VALUES('task_done','test','completed',0,'2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    drop(conn);

    let result = orchestrator::cancel_task(dir.path(), "task_done");
    assert!(
        result.is_err(),
        "expected error when cancelling a completed task"
    );
}

// ── list_sessions ─────────────────────────────────────────────────────────────

#[test]
fn list_sessions_nonexistent_run_returns_error() {
    let (dir, conn) = setup();
    drop(conn);
    let result = orchestrator::list_sessions(dir.path(), "run_does_not_exist");
    assert!(result.is_err(), "expected error for nonexistent run");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not found"),
        "expected 'not found' in error message, got: {msg}"
    );
}

#[test]
fn list_sessions_run_with_no_sessions_returns_empty_vec() {
    let (dir, conn) = setup();
    insert_run(&conn, "run_empty");
    drop(conn);

    let sessions = orchestrator::list_sessions(dir.path(), "run_empty").unwrap();
    assert!(
        sessions.is_empty(),
        "expected empty sessions list for new run"
    );
}

#[test]
fn list_sessions_returns_all_sessions_for_run() {
    let (dir, conn) = setup();
    insert_run(&conn, "run_with_sessions");
    insert_session(&conn, "sess_a", "run_with_sessions");
    insert_session(&conn, "sess_b", "run_with_sessions");
    drop(conn);

    let sessions = orchestrator::list_sessions(dir.path(), "run_with_sessions").unwrap();
    assert_eq!(sessions.len(), 2, "expected 2 sessions");
    let ids: Vec<&str> = sessions.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"sess_a"), "expected sess_a");
    assert!(ids.contains(&"sess_b"), "expected sess_b");
}

#[test]
fn list_sessions_does_not_return_sessions_from_other_run() {
    let (dir, conn) = setup();
    insert_run(&conn, "run_target");
    insert_run(&conn, "run_other");
    insert_session(&conn, "sess_target", "run_target");
    insert_session(&conn, "sess_other", "run_other");
    drop(conn);

    let sessions = orchestrator::list_sessions(dir.path(), "run_target").unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "sess_target");
}

// ── list_merge_queue ──────────────────────────────────────────────────────────

#[test]
fn list_merge_queue_conversation_with_no_entries_returns_empty() {
    let (dir, conn) = setup();
    // Create a conversation to query
    conn.execute(
        "INSERT INTO conversations(id,project_id,state,created_at,updated_at)
         VALUES('conv_no_merges','proj1','active','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    drop(conn);

    let entries = orchestrator::list_merge_queue(dir.path(), "conv_no_merges").unwrap();
    assert!(
        entries.is_empty(),
        "expected empty merge queue for conversation with no merges"
    );
}

#[test]
fn list_merge_queue_returns_entries_for_conversation_only() {
    let (dir, mut conn) = setup();
    // Create two conversations
    conn.execute(
        "INSERT INTO conversations(id,project_id,state,created_at,updated_at)
         VALUES('conv_a','proj1','active','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO conversations(id,project_id,state,created_at,updated_at)
         VALUES('conv_b','proj1','active','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    grove_core::db::repositories::merge_queue_repo::enqueue(
        &mut conn,
        "conv_a",
        "grove/branch-a",
        "main",
        "direct",
        "2024-01-01T00:00:00Z",
    )
    .unwrap();
    grove_core::db::repositories::merge_queue_repo::enqueue(
        &mut conn,
        "conv_b",
        "grove/branch-b",
        "main",
        "direct",
        "2024-01-01T00:00:00Z",
    )
    .unwrap();
    drop(conn);

    let entries = orchestrator::list_merge_queue(dir.path(), "conv_a").unwrap();
    assert_eq!(entries.len(), 1, "expected exactly 1 entry for conv_a");
    assert_eq!(entries[0].branch_name, "grove/branch-a");
    assert_eq!(entries[0].conversation_id, "conv_a");
}

// ── list_ownership_locks ──────────────────────────────────────────────────────

#[test]
fn list_ownership_locks_without_filter_returns_all_runs() {
    let (dir, conn) = setup();
    insert_run(&conn, "runA");
    insert_run(&conn, "runB");
    insert_ownership_lock(&conn, "runA", "src/a.rs", "sessA");
    insert_ownership_lock(&conn, "runB", "src/b.rs", "sessB");
    drop(conn);

    let all = orchestrator::list_ownership_locks(dir.path(), None).unwrap();
    assert_eq!(all.len(), 2, "expected 2 locks across all runs");
}

#[test]
fn list_ownership_locks_with_run_id_uses_targeted_query() {
    let (dir, conn) = setup();
    insert_run(&conn, "runA");
    insert_run(&conn, "runB");
    insert_ownership_lock(&conn, "runA", "src/a.rs", "sessA");
    insert_ownership_lock(&conn, "runA", "src/c.rs", "sessA");
    insert_ownership_lock(&conn, "runB", "src/b.rs", "sessB");
    drop(conn);

    let filtered = orchestrator::list_ownership_locks(dir.path(), Some("runA")).unwrap();
    assert_eq!(filtered.len(), 2, "expected only 2 locks for runA");
    assert!(
        filtered.iter().all(|l| l.run_id == "runA"),
        "all returned locks must belong to runA"
    );
}

#[test]
fn list_ownership_locks_with_unknown_run_id_returns_empty() {
    let (dir, conn) = setup();
    insert_run(&conn, "runA");
    insert_ownership_lock(&conn, "runA", "src/a.rs", "sessA");
    drop(conn);

    let result = orchestrator::list_ownership_locks(dir.path(), Some("run_nonexistent")).unwrap();
    assert!(
        result.is_empty(),
        "expected empty list for unknown run_id filter"
    );
}

// ── ownership_repo::list_for_run ──────────────────────────────────────────────

#[test]
fn ownership_repo_list_for_run_returns_only_matching_run() {
    let (_dir, conn) = setup();
    insert_run(&conn, "runX");
    insert_run(&conn, "runY");
    insert_ownership_lock(&conn, "runX", "src/x.rs", "sessX");
    insert_ownership_lock(&conn, "runY", "src/y.rs", "sessY");

    let rows = ownership_repo::list_for_run(&conn, "runX").unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].run_id, "runX");
    assert_eq!(rows[0].path, "src/x.rs");
}

#[test]
fn ownership_repo_list_for_run_returns_empty_for_unknown_run() {
    let (_dir, conn) = setup();
    insert_run(&conn, "runZ");
    insert_ownership_lock(&conn, "runZ", "src/z.rs", "sessZ");

    let rows = ownership_repo::list_for_run(&conn, "run_unknown").unwrap();
    assert!(rows.is_empty());
}

// ── run_events_all ────────────────────────────────────────────────────────────

#[test]
fn run_events_all_nonexistent_run_returns_error() {
    let (dir, conn) = setup();
    drop(conn);
    let result = orchestrator::run_events_all(dir.path(), "run_does_not_exist");
    assert!(result.is_err(), "expected error for nonexistent run");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("not found"),
        "expected 'not found' in error message, got: {msg}"
    );
}

#[test]
fn run_events_all_returns_all_events_without_200_cap() {
    let (dir, conn) = setup();
    insert_run(&conn, "run_many_events");

    // Insert 205 events — more than the 200-event tail cap used by run_events.
    for i in 0..205i64 {
        events::emit(
            &conn,
            "run_many_events",
            None,
            "test_event",
            json!({ "index": i }),
        )
        .unwrap();
    }
    drop(conn);

    let all = orchestrator::run_events_all(dir.path(), "run_many_events").unwrap();
    assert_eq!(
        all.len(),
        205,
        "run_events_all must return all 205 events; got {}. \
         If it returns 200 the 200-event cap was not bypassed.",
        all.len()
    );
}

#[test]
fn run_events_all_returns_events_in_insertion_order() {
    let (dir, conn) = setup();
    insert_run(&conn, "run_ordered");
    for i in 0..5i64 {
        events::emit(
            &conn,
            "run_ordered",
            None,
            "ordered_event",
            json!({ "seq": i }),
        )
        .unwrap();
    }
    drop(conn);

    let evts = orchestrator::run_events_all(dir.path(), "run_ordered").unwrap();
    assert_eq!(evts.len(), 5);
    for (i, evt) in evts.iter().enumerate() {
        let seq = evt.payload["seq"].as_i64().unwrap();
        assert_eq!(seq, i as i64, "events must be returned in insertion order");
    }
}
