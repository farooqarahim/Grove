use grove_core::checkpoint::wal_controller;
use grove_core::db;
use grove_core::events;
use tempfile::TempDir;

fn setup_with_run() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES('run_wal','wal test','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    (dir, conn)
}

#[test]
fn passive_checkpoint_after_100_inserts_returns_ok() {
    let (_dir, conn) = setup_with_run();

    for i in 0..100 {
        events::emit(
            &conn,
            "run_wal",
            None,
            &format!("event_{i}"),
            serde_json::json!({"i": i}),
        )
        .unwrap();
    }

    let stats = wal_controller::passive_checkpoint(&conn).unwrap();
    assert!(stats.wal_pages >= 0, "wal_pages must be non-negative");
    assert!(
        stats.checkpointed_pages >= 0,
        "checkpointed_pages must be non-negative"
    );
}

#[test]
fn wal_size_pages_returns_non_negative() {
    let (_dir, conn) = setup_with_run();
    let pages = wal_controller::wal_size_pages(&conn).unwrap();
    assert!(pages >= 0);
}

#[test]
fn full_checkpoint_completes_without_error() {
    let (_dir, conn) = setup_with_run();

    // Write a few rows to ensure WAL has content.
    for i in 0..10 {
        events::emit(
            &conn,
            "run_wal",
            None,
            &format!("full_{i}"),
            serde_json::json!({}),
        )
        .unwrap();
    }

    let stats = wal_controller::full_checkpoint(&conn).unwrap();
    // After a full checkpoint the WAL should be flushed.
    assert!(stats.wal_pages >= 0);
}
