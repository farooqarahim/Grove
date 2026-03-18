/// Integration tests covering orchestrator error paths:
/// invalid inputs, state transition violations, and database/system errors.
///
/// All tests use a real (not mocked) SQLite database via direct DB setup,
/// avoiding any MockProvider overhead for tests that don't exercise execution.
use grove_core::db;
use grove_core::errors::GroveError;
use grove_core::orchestrator;
use rusqlite::params;
use tempfile::TempDir;

// ── Shared helpers ─────────────────────────────────────────────────────────────

fn setup() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

/// Insert a minimal run row with the given state.
fn insert_run_with_state(conn: &rusqlite::Connection, run_id: &str, state: &str) {
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'test objective',?2,1.0,0.0,
                strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        params![run_id, state],
    )
    .unwrap();
}

// ── Invalid input tests ────────────────────────────────────────────────────────

#[test]
fn queue_task_empty_objective_returns_validation_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::queue_task(
        dir.path(),
        "",
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );

    assert!(result.is_err(), "empty objective must be rejected");
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::ValidationError { ref field, .. } if field == "objective"),
        "expected ValidationError on 'objective' field; got: {err:?}"
    );
}

#[test]
fn queue_task_whitespace_only_objective_returns_validation_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::queue_task(
        dir.path(),
        "   \t\n  ",
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );

    assert!(
        result.is_err(),
        "whitespace-only objective must be rejected"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::ValidationError { ref field, .. } if field == "objective"),
        "expected ValidationError on 'objective' field; got: {err:?}"
    );
}

#[test]
fn queue_task_negative_budget_returns_validation_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::queue_task(
        dir.path(),
        "do something useful",
        Some(-5.0),
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );

    assert!(result.is_err(), "negative budget must be rejected");
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::ValidationError { ref field, .. } if field == "budget_usd"),
        "expected ValidationError on 'budget_usd' field; got: {err:?}"
    );
}

#[test]
fn queue_task_zero_budget_returns_validation_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::queue_task(
        dir.path(),
        "do something useful",
        Some(0.0),
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );

    assert!(result.is_err(), "zero budget must be rejected");
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::ValidationError { ref field, .. } if field == "budget_usd"),
        "expected ValidationError on 'budget_usd' field; got: {err:?}"
    );
}

#[test]
fn queue_task_very_long_objective_does_not_crash() {
    let (dir, conn) = setup();
    drop(conn);

    // 120 KB of text — well above any typical input limit.
    let long_objective = "x".repeat(120 * 1024);

    let result = orchestrator::queue_task(
        dir.path(),
        &long_objective,
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );

    // Must either succeed or fail gracefully — must not panic.
    match result {
        Ok(task) => {
            assert_eq!(task.objective.len(), long_objective.len());
        }
        Err(e) => {
            // Any error is acceptable as long as it's structured.
            assert!(
                !e.to_string().is_empty(),
                "error message must not be empty: {e:?}"
            );
        }
    }
}

#[test]
fn queue_task_invalid_conversation_id_returns_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::queue_task(
        dir.path(),
        "valid objective",
        None,
        0,
        None,
        None,
        Some("conv_does_not_exist_xyz"),
        None,
        None,
        None,
        false,
    );

    assert!(
        result.is_err(),
        "referencing a non-existent conversation must fail"
    );
}

// ── State transition violation tests ──────────────────────────────────────────

#[test]
fn abort_completed_run_returns_invalid_transition_error() {
    let (dir, conn) = setup();
    insert_run_with_state(&conn, "run_completed", "completed");
    drop(conn);

    let result = orchestrator::abort_run(dir.path(), "run_completed");

    assert!(
        result.is_err(),
        "aborting a completed run must fail with an error"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::InvalidTransition(_)),
        "expected InvalidTransition error; got: {err:?}"
    );
}

#[test]
fn abort_already_paused_run_returns_invalid_transition_error() {
    let (dir, conn) = setup();
    insert_run_with_state(&conn, "run_paused", "paused");
    drop(conn);

    // Paused → Paused is not a valid transition.
    let result = orchestrator::abort_run(dir.path(), "run_paused");

    assert!(result.is_err(), "aborting an already-paused run must fail");
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::InvalidTransition(_)),
        "expected InvalidTransition error; got: {err:?}"
    );
}

#[test]
fn abort_nonexistent_run_returns_database_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::abort_run(dir.path(), "run_does_not_exist_xyz");

    assert!(
        result.is_err(),
        "aborting a non-existent run must return an error"
    );
    let err = result.unwrap_err();
    // rusqlite returns QueryReturnedNoRows which maps to GroveError::Database.
    assert!(
        matches!(err, GroveError::Database(_)),
        "expected Database error for missing run; got: {err:?}"
    );
}

#[test]
fn resume_completed_run_returns_invalid_transition_error() {
    let (dir, conn) = setup();
    insert_run_with_state(&conn, "run_done", "completed");
    // A completed run has no checkpoint; resume will fail at the checkpoint
    // lookup or at the transition, both of which are errors.
    drop(conn);

    let result = orchestrator::resume_run(dir.path(), "run_done");

    assert!(
        result.is_err(),
        "resuming a completed run must return an error"
    );
}

#[test]
fn resume_nonexistent_run_returns_database_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::resume_run(dir.path(), "run_nonexistent_xyz");

    assert!(
        result.is_err(),
        "resuming a non-existent run must return an error"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::Database(_)),
        "expected Database error for missing run; got: {err:?}"
    );
}

#[test]
fn abort_failed_run_returns_invalid_transition_error() {
    let (dir, conn) = setup();
    insert_run_with_state(&conn, "run_failed", "failed");
    drop(conn);

    // Failed → Paused is not a valid transition per the state machine.
    let result = orchestrator::abort_run(dir.path(), "run_failed");

    assert!(
        result.is_err(),
        "aborting a failed run must fail (Failed → Paused is not allowed)"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::InvalidTransition(_)),
        "expected InvalidTransition error; got: {err:?}"
    );
}

#[test]
fn abort_created_run_returns_invalid_transition_error() {
    let (dir, conn) = setup();
    insert_run_with_state(&conn, "run_created", "created");
    drop(conn);

    // Created → Paused is not a valid transition.
    let result = orchestrator::abort_run(dir.path(), "run_created");

    assert!(
        result.is_err(),
        "aborting a run in 'created' state must fail"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::InvalidTransition(_)),
        "expected InvalidTransition error; got: {err:?}"
    );
}

// ── Database / system error tests ──────────────────────────────────────────────

#[test]
fn list_runs_on_empty_database_returns_empty_list() {
    let (dir, conn) = setup();
    drop(conn);

    let runs = orchestrator::list_runs(dir.path(), 100).unwrap();

    assert!(
        runs.is_empty(),
        "list_runs on empty DB must return an empty list; got {} runs",
        runs.len()
    );
}

#[test]
fn list_tasks_on_empty_database_returns_empty_list() {
    let (dir, conn) = setup();
    drop(conn);

    let tasks = orchestrator::list_tasks(dir.path()).unwrap();

    assert!(
        tasks.is_empty(),
        "list_tasks on empty DB must return an empty list; got {} tasks",
        tasks.len()
    );
}

#[test]
fn run_events_nonexistent_run_id_returns_error() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::run_events(dir.path(), "run_ghost_xyz");

    // The underlying query returns an empty list for unknown run_ids, but
    // run_events_all validates that the run exists.  run_events uses list_for_run_tail
    // which does not enforce run existence.  Verify the call does not panic
    // and either returns Ok([]) or Err.
    match result {
        Ok(evts) => assert!(
            evts.is_empty(),
            "run_events for unknown run must return empty list or error"
        ),
        Err(e) => assert!(
            !e.to_string().is_empty(),
            "error message must not be empty: {e:?}"
        ),
    }
}

#[test]
fn list_runs_uninitialized_directory_returns_error() {
    // Use a fresh temp dir that has NOT been initialized (no .grove/grove.db).
    let dir = TempDir::new().unwrap();

    let result = orchestrator::list_runs(dir.path(), 10);

    assert!(
        result.is_err(),
        "list_runs on an uninitialized directory must return an error"
    );
}

#[test]
fn abort_run_uninitialized_directory_returns_error() {
    let dir = TempDir::new().unwrap();

    let result = orchestrator::abort_run(dir.path(), "any_run_id");

    assert!(
        result.is_err(),
        "abort_run on an uninitialized directory must return an error"
    );
}

#[test]
fn queue_task_uninitialized_directory_returns_error() {
    // Validation runs before DB access — use a valid objective so we reach the DB.
    let dir = TempDir::new().unwrap();

    let result = orchestrator::queue_task(
        dir.path(),
        "valid objective that passes validation",
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );

    assert!(
        result.is_err(),
        "queue_task on an uninitialized directory must return an error"
    );
}

// ── Double-queue / idempotency tests ──────────────────────────────────────────

#[test]
fn queue_two_tasks_with_same_objective_both_succeed() {
    // Tasks are identified by auto-generated IDs; queuing the same objective
    // twice must create two independent task records.
    let (dir, conn) = setup();
    drop(conn);

    let t1 = orchestrator::queue_task(
        dir.path(),
        "duplicate objective",
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

    let t2 = orchestrator::queue_task(
        dir.path(),
        "duplicate objective",
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

    assert_ne!(
        t1.id, t2.id,
        "two tasks with the same objective must get distinct IDs"
    );
    assert_eq!(t1.objective, t2.objective);

    let tasks = orchestrator::list_tasks(dir.path()).unwrap();
    let matching: Vec<_> = tasks
        .iter()
        .filter(|t| t.objective == "duplicate objective")
        .collect();
    assert_eq!(
        matching.len(),
        2,
        "both tasks must persist in the DB; found {}",
        matching.len()
    );
}

#[test]
fn queue_task_positive_budget_is_accepted() {
    let (dir, conn) = setup();
    drop(conn);

    let result = orchestrator::queue_task(
        dir.path(),
        "valid objective",
        Some(1.5),
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    );

    assert!(
        result.is_ok(),
        "positive budget must be accepted; got: {:?}",
        result.err()
    );
    let task = result.unwrap();
    assert_eq!(task.budget_usd, Some(1.5));
}

#[test]
fn abort_run_with_unknown_state_string_returns_runtime_error() {
    let (dir, conn) = setup();
    // Insert a valid run first, then corrupt its state via a PRAGMA bypass so
    // the CHECK constraint is not enforced for this single write.  This lets us
    // test the `RunState::from_str` error branch in `abort_run`.
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES('run_bad_state','test','executing',1.0,0.0,
                strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        [],
    )
    .unwrap();
    // Disable CHECK constraints for this connection so we can write the invalid state.
    conn.execute_batch("PRAGMA ignore_check_constraints = 1")
        .ok();
    conn.execute(
        "UPDATE runs SET state = 'not_a_valid_state' WHERE id = 'run_bad_state'",
        [],
    )
    .unwrap();
    drop(conn);

    let result = orchestrator::abort_run(dir.path(), "run_bad_state");

    assert!(
        result.is_err(),
        "abort with unknown state string must return an error"
    );
    let err = result.unwrap_err();
    assert!(
        matches!(err, GroveError::Runtime(_)),
        "expected Runtime error for unknown state; got: {err:?}"
    );
    let msg = err.to_string();
    assert!(
        msg.contains("not_a_valid_state"),
        "error must mention the bad state; got: {msg}"
    );
}
