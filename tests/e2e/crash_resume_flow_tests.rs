use assert_cmd::Command;
use grove_core::checkpoint::{self, BudgetSnapshot, CheckpointPayload};
use grove_core::db;
use tempfile::TempDir;

fn grove(dir: &TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_grove"));
    cmd.args(["--project", dir.path().to_str().unwrap()]);
    cmd
}

/// Insert a run directly into the DB in a given state, bypassing the orchestrator.
fn insert_paused_run(dir: &TempDir, run_id: &str) {
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'crash resume test','paused',5.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [run_id],
    )
    .unwrap();

    // Save a checkpoint so `resume_from_checkpoint` can find the objective.
    let payload = CheckpointPayload {
        run_id: run_id.to_string(),
        stage: "executing".to_string(),
        active_sessions: vec![],
        pending_tasks: vec!["crash resume test".to_string()],
        ownership: vec![],
        budget: BudgetSnapshot {
            allocated_usd: 5.0,
            used_usd: 0.0,
        },
    };
    checkpoint::save(&conn, &format!("cp_{run_id}"), &payload).unwrap();
}

#[test]
fn resume_paused_run_reaches_completed() {
    let dir = TempDir::new().unwrap();
    grove(&dir).arg("init").assert().success();

    let run_id = "run_crash_test_001";
    insert_paused_run(&dir, run_id);

    // Resume should succeed and bring the run to completed.
    grove(&dir).args(["resume", run_id]).assert().success();

    // Verify final state via DB.
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let state: String = conn
        .query_row("SELECT state FROM runs WHERE id=?1", [run_id], |r| r.get(0))
        .unwrap();
    assert_eq!(
        state, "completed",
        "resumed run should reach completed state"
    );
}

#[test]
fn resume_nonexistent_run_returns_error() {
    let dir = TempDir::new().unwrap();
    grove(&dir).arg("init").assert().success();

    grove(&dir)
        .args(["resume", "run_does_not_exist"])
        .assert()
        .failure();
}

#[test]
fn abort_then_resume_cycle() {
    let dir = TempDir::new().unwrap();
    grove(&dir).arg("init").assert().success();

    let run_id = "run_abort_resume_002";

    // Insert a run in `executing` state so abort is valid.
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES(?1,'abort resume','executing',5.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [run_id],
    )
    .unwrap();
    drop(conn);

    // Abort transitions the run to `paused`.
    grove(&dir).args(["abort", run_id]).assert().success();

    // Save a checkpoint so resume can find the objective.
    let conn2 = db::DbHandle::new(dir.path()).connect().unwrap();
    let payload = CheckpointPayload {
        run_id: run_id.to_string(),
        stage: "executing".to_string(),
        active_sessions: vec![],
        pending_tasks: vec!["abort resume".to_string()],
        ownership: vec![],
        budget: BudgetSnapshot {
            allocated_usd: 5.0,
            used_usd: 0.0,
        },
    };
    checkpoint::save(&conn2, &format!("cp_{run_id}"), &payload).unwrap();
    drop(conn2);

    // Resume should reach completed.
    grove(&dir).args(["resume", run_id]).assert().success();
}
