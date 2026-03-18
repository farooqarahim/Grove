use grove_core::budget::policy::{BudgetStatus, check_budget, record_spend};
use grove_core::db;
use rusqlite::params;
use tempfile::TempDir;

fn setup() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

fn insert_run(conn: &rusqlite::Connection, run_id: &str, budget: f64) {
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'test','executing',?2,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        params![run_id, budget],
    )
    .unwrap();
}

#[test]
fn check_budget_ok_when_under_limit() {
    let (_dir, conn) = setup();
    insert_run(&conn, "run1", 10.0);
    let status = check_budget(&conn, "run1").unwrap();
    assert!(matches!(status, BudgetStatus::Ok { .. }));
}

#[test]
fn check_budget_warning_at_80_percent() {
    let (_dir, conn) = setup();
    insert_run(&conn, "run2", 10.0);
    // 8.5 / 10.0 = 85% — above warning (80%) but below hard stop (100%)
    conn.execute("UPDATE runs SET cost_used_usd=8.5 WHERE id='run2'", [])
        .unwrap();
    let status = check_budget(&conn, "run2").unwrap();
    assert!(
        matches!(status, BudgetStatus::Warning { .. }),
        "expected Warning, got {:?}",
        status
    );
}

#[test]
fn check_budget_exceeded_at_limit() {
    let (_dir, conn) = setup();
    insert_run(&conn, "run3", 1.0);
    conn.execute("UPDATE runs SET cost_used_usd=1.0 WHERE id='run3'", [])
        .unwrap();
    let status = check_budget(&conn, "run3").unwrap();
    assert!(matches!(status, BudgetStatus::Exceeded { .. }));
}

#[test]
fn check_budget_exceeded_over_limit() {
    let (_dir, conn) = setup();
    insert_run(&conn, "run4", 1.0);
    conn.execute("UPDATE runs SET cost_used_usd=2.0 WHERE id='run4'", [])
        .unwrap();
    let status = check_budget(&conn, "run4").unwrap();
    assert!(matches!(status, BudgetStatus::Exceeded { .. }));
}

#[test]
fn record_spend_accumulates_correctly() {
    let (_dir, conn) = setup();
    insert_run(&conn, "run5", 100.0);
    record_spend(&conn, "run5", 1.5).unwrap();
    record_spend(&conn, "run5", 2.5).unwrap();

    let cost: f64 = conn
        .query_row("SELECT cost_used_usd FROM runs WHERE id='run5'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!((cost - 4.0).abs() < 0.0001, "expected 4.0, got {cost}");
}

#[test]
fn record_spend_is_capped_at_budget() {
    let (_dir, conn) = setup();
    insert_run(&conn, "run6", 5.0);
    // Spend more than the budget — should be capped at 5.0
    record_spend(&conn, "run6", 10.0).unwrap();

    let cost: f64 = conn
        .query_row("SELECT cost_used_usd FROM runs WHERE id='run6'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(cost <= 5.0, "cost {cost} should not exceed budget 5.0");
}
