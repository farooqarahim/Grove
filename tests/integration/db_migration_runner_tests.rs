/// Integration tests for `db::initialize()`.
///
/// These tests replaced the old `migration_runner` tests after `migration_runner.rs`
/// was deleted ([7]-A). All migration logic now lives in `db::initialize()`, which
/// wraps each migration in a `BEGIN IMMEDIATE` transaction.
use std::sync::Arc;

use grove_core::db;
use grove_core::db::repositories::meta_repo;
use tempfile::TempDir;

fn setup() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

/// Returns the current expected schema version by running initialize() once.
fn expected_version() -> i64 {
    let dir = TempDir::new().unwrap();
    let result = db::initialize(dir.path()).unwrap();
    result.schema_version
}

#[test]
fn initialize_reaches_latest_schema_version() {
    let (_dir, conn) = setup();
    let version = meta_repo::get_schema_version(&conn).unwrap();
    let expected = expected_version();
    assert_eq!(
        version, expected,
        "expected schema_version = {expected} after full migration"
    );
}

#[test]
fn calling_initialize_twice_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let first = db::initialize(dir.path()).unwrap();
    let second = db::initialize(dir.path()).unwrap();
    assert_eq!(first.schema_version, second.schema_version);
    assert!(
        second.schema_version >= 1,
        "schema_version must be at least 1"
    );
}

#[test]
fn calling_initialize_ten_times_is_stable() {
    let dir = TempDir::new().unwrap();
    let expected = expected_version();
    for _ in 0..10 {
        let result = db::initialize(dir.path()).unwrap();
        assert_eq!(result.schema_version, expected);
    }
}

#[test]
fn all_expected_tables_exist_after_initialize() {
    let (_dir, conn) = setup();
    for table in &[
        "meta",
        "runs",
        "sessions",
        "events",
        "tasks",
        "plan_steps",
        "subtasks",
        "checkpoints",
        "audit_log",
        "conversations",
        "messages",
        "workspaces",
        "projects",
        "users",
        "issues",
        "issue_comments",
        "issue_events",
        "issue_sync_state",
    ] {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get(0),
            )
            .unwrap_or(0);
        assert_eq!(count, 1, "table '{table}' must exist after initialize()");
    }
}

#[test]
fn meta_schema_version_row_matches_result() {
    let (_dir, conn) = setup();
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let expected = expected_version();
    assert_eq!(version, expected.to_string());
}

#[test]
fn journal_mode_is_wal_after_initialize() {
    let (_dir, conn) = setup();
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_lowercase(), "wal");
}

#[test]
fn concurrent_initialize_converges() {
    // Validate the double-checked-lock fix in `apply_migration_if_needed`:
    // four threads race to call `initialize()` on the same DB path.
    // All must succeed and converge to the same schema version.
    let dir = TempDir::new().unwrap();
    let path = Arc::new(dir.path().to_path_buf());
    let expected = expected_version();

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let p = Arc::clone(&path);
            std::thread::spawn(move || {
                db::initialize(&p).expect("concurrent initialize must not fail")
            })
        })
        .collect();

    for handle in handles {
        let result = handle.join().expect("thread must not panic");
        assert_eq!(
            result.schema_version, expected,
            "schema_version must be {expected} regardless of which thread applied the migrations"
        );
    }

    // Final sanity-check via a fresh connection.
    let conn = db::DbHandle::new(&path).connect().unwrap();
    let version = meta_repo::get_schema_version(&conn).unwrap();
    assert_eq!(version, expected);
}

#[test]
fn initialize_succeeds_when_column_already_exists() {
    // Simulate the scenario that previously triggered the "column already exists"
    // warning: a column from a later migration is added manually to a database
    // whose schema_version hasn't caught up yet.
    let dir = TempDir::new().unwrap();

    // Run initialize to get a fully migrated DB.
    db::initialize(dir.path()).unwrap();

    // Roll the schema_version back so migration 22 will be re-attempted,
    // but the `base_ref` column already exists on the `projects` table.
    {
        let conn = db::DbHandle::new(dir.path()).connect().unwrap();
        conn.execute(
            "UPDATE meta SET value = '21' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
        let v: i64 = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(v, 21, "schema_version should be rolled back to 21");
    }

    // Re-initialize — migration 22 will try ADD COLUMN base_ref but it already
    // exists. The idempotent runner must skip it cleanly (no error, no warning).
    let result = db::initialize(dir.path())
        .expect("initialize must succeed even when column already exists");
    assert_eq!(result.schema_version, expected_version());
}
