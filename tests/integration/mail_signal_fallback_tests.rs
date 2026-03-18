/// Simulates the "watcher delivers zero events, polling fallback picks up new rows" scenario.
///
/// The events::list_for_run function is the polling fallback: it reads directly
/// from the DB and always returns any rows inserted since the last call.
/// This test verifies that polling reliably detects new rows.
use grove_core::db;
use grove_core::events;
use tempfile::TempDir;

fn setup_with_run() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES('run_poll','poll test','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    (dir, conn)
}

#[test]
fn polling_detects_first_row_after_watcher_delivered_nothing() {
    let (_dir, conn) = setup_with_run();

    // Watcher delivered nothing — poll returns empty.
    let before = events::list_for_run(&conn, "run_poll").unwrap();
    assert!(before.is_empty(), "expected empty before first insert");

    // A writer inserts a row.
    events::emit(
        &conn,
        "run_poll",
        None,
        "run_created",
        serde_json::json!({"source": "test"}),
    )
    .unwrap();

    // Polling now detects the new row.
    let after = events::list_for_run(&conn, "run_poll").unwrap();
    assert_eq!(after.len(), 1, "polling fallback must detect new row");
}

#[test]
fn polling_detects_additional_rows_on_subsequent_calls() {
    let (_dir, conn) = setup_with_run();

    events::emit(&conn, "run_poll", None, "event_1", serde_json::json!({})).unwrap();
    let first_poll = events::list_for_run(&conn, "run_poll").unwrap();
    assert_eq!(first_poll.len(), 1);

    events::emit(&conn, "run_poll", None, "event_2", serde_json::json!({})).unwrap();
    events::emit(&conn, "run_poll", None, "event_3", serde_json::json!({})).unwrap();

    // Second poll — must return all 3 rows, not just the new ones.
    let second_poll = events::list_for_run(&conn, "run_poll").unwrap();
    assert_eq!(
        second_poll.len(),
        3,
        "polling fallback must return all rows"
    );
}

#[test]
fn polling_returns_rows_in_insertion_order() {
    let (_dir, conn) = setup_with_run();

    let types = ["alpha", "beta", "gamma"];
    for t in types {
        events::emit(&conn, "run_poll", None, t, serde_json::json!({})).unwrap();
    }

    let events = events::list_for_run(&conn, "run_poll").unwrap();
    assert_eq!(events.len(), 3);
    let returned_types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    assert_eq!(returned_types, vec!["alpha", "beta", "gamma"]);
}
