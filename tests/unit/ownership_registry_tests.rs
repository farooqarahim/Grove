use grove_core::db;
use grove_core::ownership::registry::OwnershipRegistry;
use rusqlite::params;
use tempfile::TempDir;

fn setup() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

fn insert_run(conn: &rusqlite::Connection, run_id: &str) {
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'test','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [run_id],
    )
    .unwrap();
}

fn insert_session(conn: &rusqlite::Connection, session_id: &str, run_id: &str) {
    conn.execute(
        "INSERT INTO sessions(id,run_id,agent_type,state,worktree_path,created_at,updated_at)
         VALUES(?1,?2,'builder','queued','/tmp','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        params![session_id, run_id],
    )
    .unwrap();
}

#[test]
fn first_acquire_succeeds() {
    let (_dir, mut conn) = setup();
    insert_run(&conn, "run1");
    insert_session(&conn, "sess1", "run1");

    let mut reg = OwnershipRegistry::new(&mut conn);
    reg.try_acquire("run1", "src/lib.rs", "sess1").unwrap();
}

#[test]
fn same_session_acquire_is_idempotent() {
    let (_dir, mut conn) = setup();
    insert_run(&conn, "run1");
    insert_session(&conn, "sess1", "run1");

    let mut reg = OwnershipRegistry::new(&mut conn);
    reg.try_acquire("run1", "src/lib.rs", "sess1").unwrap();
    // Second acquire by same session should succeed (idempotent)
    reg.try_acquire("run1", "src/lib.rs", "sess1").unwrap();
}

#[test]
fn second_acquire_by_different_session_fails_with_conflict() {
    let (_dir, mut conn) = setup();
    insert_run(&conn, "run1");
    insert_session(&conn, "sess1", "run1");
    insert_session(&conn, "sess2", "run1");

    let mut reg = OwnershipRegistry::new(&mut conn);
    reg.try_acquire("run1", "src/lib.rs", "sess1").unwrap();

    let result = reg.try_acquire("run1", "src/lib.rs", "sess2");
    assert!(result.is_err(), "expected ownership conflict error");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("conflict") || msg.contains("locked"),
        "expected conflict message, got: {msg}"
    );
}

#[test]
fn release_then_reacquire_succeeds() {
    let (_dir, mut conn) = setup();
    insert_run(&conn, "run1");
    insert_session(&conn, "sess1", "run1");
    insert_session(&conn, "sess2", "run1");

    let mut reg = OwnershipRegistry::new(&mut conn);
    reg.try_acquire("run1", "src/lib.rs", "sess1").unwrap();
    reg.release("run1", "src/lib.rs", "sess1").unwrap();

    // sess2 can now acquire the released lock
    reg.try_acquire("run1", "src/lib.rs", "sess2").unwrap();
}

#[test]
fn release_all_clears_all_locks_for_session() {
    let (_dir, mut conn) = setup();
    insert_run(&conn, "run1");
    insert_session(&conn, "sess1", "run1");

    let mut reg = OwnershipRegistry::new(&mut conn);
    reg.try_acquire("run1", "src/a.rs", "sess1").unwrap();
    reg.try_acquire("run1", "src/b.rs", "sess1").unwrap();

    let released = reg.release_all("sess1").unwrap();
    assert_eq!(released, 2);
}
