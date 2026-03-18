/// Error-path tests for `events::list_for_run` after the [6]-A fix.
///
/// Verifies that corrupt payload_json rows surface as `Err` rather than
/// silently substituting `null`.
use grove_core::db;
use grove_core::events;
use tempfile::TempDir;

fn setup_with_run() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES('run_ep','error path test','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    (dir, conn)
}

#[test]
fn list_for_run_returns_ok_for_valid_payload() {
    let (_dir, conn) = setup_with_run();

    events::emit(
        &conn,
        "run_ep",
        None,
        "run_created",
        serde_json::json!({"source": "test"}),
    )
    .unwrap();

    let result = events::list_for_run(&conn, "run_ep");
    assert!(result.is_ok(), "valid payload must not error");
    let rows = result.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].event_type, "run_created");
}

#[test]
fn list_for_run_errors_on_corrupt_payload_json() {
    let (_dir, conn) = setup_with_run();

    // Bypass `events::emit` to inject a corrupt payload directly.
    conn.execute(
        "INSERT INTO events(run_id, session_id, type, payload_json, created_at)
         VALUES('run_ep', NULL, 'corrupt_event', 'not-valid-json', '2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let result = events::list_for_run(&conn, "run_ep");
    assert!(
        result.is_err(),
        "corrupt payload_json must surface as Err, not silently return null"
    );
}

#[test]
fn list_for_run_tail_errors_on_corrupt_payload_json() {
    let (_dir, conn) = setup_with_run();

    conn.execute(
        "INSERT INTO events(run_id, session_id, type, payload_json, created_at)
         VALUES('run_ep', NULL, 'corrupt_tail', '{bad-json', '2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let result = events::list_for_run_tail(&conn, "run_ep", 10);
    assert!(
        result.is_err(),
        "list_for_run_tail must also surface corrupt payload as Err"
    );
}

#[test]
fn redacted_emit_does_not_store_anthropic_key() {
    let (_dir, conn) = setup_with_run();

    events::emit(
        &conn,
        "run_ep",
        None,
        "agent_prompt",
        serde_json::json!({"key": "sk-ant-api03-ABCDEFGHIJ1234567890abcdef"}),
    )
    .unwrap();

    // Read raw payload_json from DB to confirm redaction happened at insert time.
    let raw: String = conn
        .query_row(
            "SELECT payload_json FROM events WHERE run_id='run_ep' AND type='agent_prompt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        !raw.contains("ABCDEFGHIJ1234567890"),
        "Anthropic key must be redacted before storage; got: {raw}"
    );
    assert!(
        raw.contains("***REDACTED***"),
        "redaction marker must be present"
    );
}
