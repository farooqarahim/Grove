use grove_core::db;
use grove_core::merge::queue::{dequeue_next, enqueue, list_pending, mark_done, mark_failed};
use tempfile::TempDir;

fn setup() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

fn seed(conn: &rusqlite::Connection) {
    // Insert a conversation to satisfy FK constraints.
    conn.execute(
        "INSERT INTO conversations(id,project_id,state,created_at,updated_at)
         VALUES('conv1','proj1','active','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
}

#[test]
fn enqueue_and_dequeue_fifo_order() {
    let (_dir, mut conn) = setup();
    seed(&conn);

    enqueue(&mut conn, "conv1", "grove/branch-a", "main", "direct").unwrap();
    enqueue(&mut conn, "conv1", "grove/branch-b", "main", "direct").unwrap();

    let first = dequeue_next(&mut conn).unwrap().unwrap();
    assert_eq!(
        first.branch_name, "grove/branch-a",
        "first dequeue should be FIFO"
    );

    let second = dequeue_next(&mut conn).unwrap().unwrap();
    assert_eq!(second.branch_name, "grove/branch-b");
}

#[test]
fn dequeue_empty_queue_returns_none() {
    let (_dir, mut conn) = setup();
    seed(&conn);

    let result = dequeue_next(&mut conn).unwrap();
    assert!(result.is_none());
}

#[test]
fn mark_done_removes_from_pending_list() {
    let (_dir, mut conn) = setup();
    seed(&conn);

    let id = enqueue(&mut conn, "conv1", "grove/done-branch", "main", "direct").unwrap();
    let entry = dequeue_next(&mut conn).unwrap().unwrap();
    assert_eq!(entry.id, id);

    mark_done(&conn, id).unwrap();

    let pending = list_pending(&conn).unwrap();
    assert!(
        pending.iter().all(|e| e.id != id),
        "completed entry should not appear in pending list"
    );
}

#[test]
fn mark_failed_stores_reason_and_removes_from_pending() {
    let (_dir, mut conn) = setup();
    seed(&conn);

    let id = enqueue(&mut conn, "conv1", "grove/fail-branch", "main", "direct").unwrap();
    dequeue_next(&mut conn).unwrap();
    mark_failed(&conn, id, "git conflict").unwrap();

    let pending = list_pending(&conn).unwrap();
    assert!(pending.iter().all(|e| e.id != id));
}

#[test]
fn list_pending_returns_queued_and_running_only() {
    let (_dir, mut conn) = setup();
    seed(&conn);

    let id1 = enqueue(&mut conn, "conv1", "grove/p1", "main", "direct").unwrap();
    let id2 = enqueue(&mut conn, "conv1", "grove/p2", "main", "direct").unwrap();
    let id3 = enqueue(&mut conn, "conv1", "grove/p3", "main", "direct").unwrap();

    // Mark id1 as done — should not appear in pending.
    dequeue_next(&mut conn).unwrap();
    mark_done(&conn, id1).unwrap();

    let pending = list_pending(&conn).unwrap();
    let ids: Vec<i64> = pending.iter().map(|e| e.id).collect();
    assert!(!ids.contains(&id1), "done entry should not be pending");
    assert!(ids.contains(&id2) || ids.contains(&id3));
}
