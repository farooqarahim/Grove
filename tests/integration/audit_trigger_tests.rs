// Migration SQL is embedded at compile time so the test is fully self-contained.
// The consolidated 0001_init.sql now contains all audit triggers.
const SQL_0001: &str = include_str!("../../migrations/0001_init.sql");

fn open_auditable_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    // apply consolidated schema (includes audit triggers)
    conn.execute_batch(SQL_0001).unwrap();
    // seed a run so FK constraints are satisfied
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES('run1','audit test','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

fn insert_event(conn: &rusqlite::Connection) -> i64 {
    conn.execute(
        "INSERT INTO events(run_id,session_id,type,payload_json,created_at)
         VALUES('run1',NULL,'test_event','{}','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn insert_event_succeeds() {
    let conn = open_auditable_db();
    let id = insert_event(&conn);
    assert!(id > 0);
}

#[test]
fn update_event_is_rejected_by_trigger() {
    let conn = open_auditable_db();
    let id = insert_event(&conn);

    let result = conn.execute("UPDATE events SET type='tampered' WHERE id=?1", [id]);
    assert!(
        result.is_err(),
        "UPDATE on events table must be rejected by the audit trigger"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("immutable") || msg.contains("audit") || msg.contains("append-only"),
        "expected immutable/audit/append-only in error, got: {msg}"
    );
}

#[test]
fn delete_event_is_rejected_by_trigger() {
    let conn = open_auditable_db();
    let id = insert_event(&conn);

    let result = conn.execute("DELETE FROM events WHERE id=?1", [id]);
    assert!(
        result.is_err(),
        "DELETE on events table must be rejected by the audit trigger"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("immutable") || msg.contains("audit") || msg.contains("append-only"),
        "expected immutable/audit/append-only in error, got: {msg}"
    );
}

#[test]
fn event_row_is_unchanged_after_rejected_update() {
    let conn = open_auditable_db();
    let id = insert_event(&conn);

    // Attempt — will fail.
    let _ = conn.execute("UPDATE events SET type='tampered' WHERE id=?1", [id]);

    let event_type: String = conn
        .query_row("SELECT type FROM events WHERE id=?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(event_type, "test_event", "original value must be preserved");
}

// ── Migration 0008: audit_log table + state-transition triggers ───────────────

fn open_state_audit_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    // Consolidated schema includes state-transition audit triggers.
    conn.execute_batch(SQL_0001).unwrap();
    // Seed a run in state 'created'.
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES('run-a','state audit','created',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    conn
}

#[test]
fn run_state_transition_writes_audit_log_row() {
    let conn = open_state_audit_db();

    conn.execute(
        "UPDATE runs SET state='executing', updated_at='2024-01-01T00:01:00Z' WHERE id='run-a'",
        [],
    )
    .unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE table_name='runs' AND row_id='run-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "one audit row expected for the state transition");

    let (old, new): (String, String) = conn
        .query_row(
            "SELECT old_state, new_state FROM audit_log WHERE table_name='runs' AND row_id='run-a'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(old, "created");
    assert_eq!(new, "executing");
}

#[test]
fn no_same_state_update_does_not_write_audit_row() {
    let conn = open_state_audit_db();

    // Update a column other than state — trigger fires only on state changes.
    conn.execute(
        "UPDATE runs SET updated_at='2024-01-01T00:02:00Z' WHERE id='run-a'",
        [],
    )
    .unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE table_name='runs' AND row_id='run-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0, "non-state update must not produce an audit row");
}

#[test]
fn multiple_state_transitions_produce_ordered_audit_rows() {
    let conn = open_state_audit_db();

    for state in &["planning", "executing", "verifying", "merging", "completed"] {
        conn.execute(
            &format!(
                "UPDATE runs SET state='{state}', updated_at='2024-01-01T00:00:00Z' WHERE id='run-a'"
            ),
            [],
        )
        .unwrap();
    }

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE table_name='runs' AND row_id='run-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 5,
        "five state transitions must produce five audit rows"
    );
}
