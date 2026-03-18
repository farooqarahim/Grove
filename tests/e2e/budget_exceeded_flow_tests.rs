use grove_core::budget::policy::{BudgetStatus, check_budget, record_spend};
use grove_core::db;
use grove_core::events;
use tempfile::TempDir;

fn setup_with_run(budget_usd: f64) -> (TempDir, String) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();

    let run_id = "run_budget_test";
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'budget test','executing',?2,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        rusqlite::params![run_id, budget_usd],
    )
    .unwrap();
    (dir, run_id.to_string())
}

#[test]
fn run_with_tiny_budget_is_exceeded_after_spend() {
    let (dir, run_id) = setup_with_run(0.001);
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();

    // Initially under budget.
    let status = check_budget(&conn, &run_id).unwrap();
    assert!(matches!(status, BudgetStatus::Ok { .. }));

    // Record a $1.00 spend — well over the $0.001 limit.
    record_spend(&conn, &run_id, 1.0).unwrap();

    let status = check_budget(&conn, &run_id).unwrap();
    assert!(
        matches!(status, BudgetStatus::Exceeded { .. }),
        "budget should be Exceeded after $1.00 spend on $0.001 limit"
    );
}

#[test]
fn budget_exceeded_emits_event() {
    let (dir, run_id) = setup_with_run(0.001);
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();

    record_spend(&conn, &run_id, 1.0).unwrap();

    if let BudgetStatus::Exceeded {
        used_usd,
        limit_usd,
    } = check_budget(&conn, &run_id).unwrap()
    {
        // Emit the budget_exceeded event (as the engine would do).
        events::emit(
            &conn,
            &run_id,
            None,
            "budget_exceeded",
            serde_json::json!({ "used_usd": used_usd, "limit_usd": limit_usd }),
        )
        .unwrap();
    }

    let ev = events::list_for_run(&conn, &run_id).unwrap();
    assert!(
        ev.iter().any(|e| e.event_type == "budget_exceeded"),
        "budget_exceeded event must be recorded"
    );
}

#[test]
fn budget_remaining_is_zero_when_exceeded() {
    let (dir, run_id) = setup_with_run(1.0);
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();

    record_spend(&conn, &run_id, 2.0).unwrap();

    if let BudgetStatus::Exceeded {
        used_usd,
        limit_usd,
    } = check_budget(&conn, &run_id).unwrap()
    {
        // record_spend caps cost_used_usd at budget_usd, so used_usd == limit_usd
        // when the spend equals or exceeds the cap. Assert >= to cover both cases.
        assert!(
            used_usd >= limit_usd,
            "used_usd ({used_usd}) must be at or above limit_usd ({limit_usd}) when budget is exceeded"
        );
    } else {
        panic!("expected BudgetStatus::Exceeded");
    }
}

#[test]
fn record_spend_is_capped_at_budget_limit() {
    let (dir, run_id) = setup_with_run(5.0);
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();

    // Spend 100× the budget — cost_used_usd should be capped at 5.0.
    record_spend(&conn, &run_id, 500.0).unwrap();

    let cost: f64 = conn
        .query_row(
            "SELECT cost_used_usd FROM runs WHERE id=?1",
            [run_id.as_str()],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        cost <= 5.0,
        "cost_used_usd {cost} must not exceed budget 5.0"
    );
}
