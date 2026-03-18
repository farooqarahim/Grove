use chrono::Utc;
use rusqlite::{Connection, params};
use serde::Serialize;
use uuid::Uuid;

use crate::errors::GroveResult;

use super::task_decomposer::{TaskSpec, compute_waves};
use super::{GrovePlanStep, PlanStep};

/// Serialize a slice to a JSON array string. Falls back to `"[]"` on error.
fn to_json_array<T: Serialize>(v: &[T]) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "[]".to_string())
}

type RawPlanStepRow = (
    String,         // id
    String,         // run_id
    i64,            // step_index
    i64,            // wave
    String,         // agent_type
    String,         // title
    String,         // description
    String,         // todos_json
    String,         // files_json
    String,         // depends_on_json
    String,         // status
    Option<String>, // session_id
    Option<String>, // result_summary
    String,         // created_at
    String,         // updated_at
);

fn map_plan_step_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawPlanStepRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
        r.get(11)?,
        r.get(12)?,
        r.get(13)?,
        r.get(14)?,
    ))
}

fn deserialize_plan_step(row: RawPlanStepRow) -> GroveResult<PlanStep> {
    let (
        id,
        run_id,
        step_index,
        wave,
        agent_type,
        title,
        description,
        todos_json,
        files_json,
        depends_on_json,
        status,
        session_id,
        result_summary,
        created_at,
        updated_at,
    ) = row;

    let todos: Vec<String> = serde_json::from_str(&todos_json)?;
    let files: Vec<String> = serde_json::from_str(&files_json)?;
    let depends_on: Vec<String> = serde_json::from_str(&depends_on_json)?;

    Ok(PlanStep {
        id,
        run_id,
        step_index,
        wave,
        agent_type,
        title,
        description,
        todos,
        files,
        depends_on,
        status,
        session_id,
        result_summary,
        created_at,
        updated_at,
    })
}

/// Insert a slice of GrovePlanStep rows into the `plan_steps` table.
///
/// Steps with `agent_type == "planner"` are silently skipped.
/// Wave assignment is computed via `compute_waves` using each step's
/// `depends_on` field (same topological-sort logic as subtasks).
///
/// `wave_offset` is added to every computed wave index, allowing dynamically
/// spawned steps to be appended after existing waves (pass `0` for the initial
/// plan, and `max_existing_wave + 1` for spawned steps).
pub fn insert_steps(
    conn: &Connection,
    run_id: &str,
    steps: &[GrovePlanStep],
    wave_offset: i64,
) -> GroveResult<()> {
    // Filter out any planner steps (the planner doesn't plan itself).
    let valid: Vec<&GrovePlanStep> = steps.iter().filter(|s| s.agent_type != "planner").collect();

    if valid.is_empty() {
        return Ok(());
    }

    // Convert to TaskSpec (only id + depends_on matter for wave computation).
    let task_specs: Vec<TaskSpec> = valid
        .iter()
        .map(|s| TaskSpec {
            id: s.id.clone(),
            title: s.title.clone(),
            description: s.description.clone(),
            files: s.files.clone(),
            depends_on: s.depends_on.clone(),
            todos: s.todos.clone(),
        })
        .collect();

    let waves = compute_waves(&task_specs)?;

    // Global step_index = sequential across all waves.
    let mut global_index = 0i64;
    let now = Utc::now().to_rfc3339();

    for (wave_idx, task_indices) in waves.iter().enumerate() {
        for &task_idx in task_indices {
            let step = valid[task_idx];
            let step_id = format!(
                "ps_{}_{}",
                run_id,
                &Uuid::new_v4().simple().to_string()[..8]
            );
            let todos_json = to_json_array(&step.todos);
            let files_json = to_json_array(&step.files);
            let depends_on_json = to_json_array(&step.depends_on);

            conn.execute(
                "INSERT OR IGNORE INTO plan_steps
                 (id, run_id, step_index, wave, agent_type, title, description,
                  todos_json, files_json, depends_on_json, status,
                  session_id, result_summary, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending',
                         NULL, NULL, ?11, ?11)",
                params![
                    step_id,
                    run_id,
                    global_index,
                    wave_idx as i64 + wave_offset,
                    step.agent_type,
                    step.title,
                    step.description,
                    todos_json,
                    files_json,
                    depends_on_json,
                    now,
                ],
            )?;
            global_index += 1;
        }
    }

    Ok(())
}

/// List all plan steps for a run, ordered by wave then step_index.
pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<PlanStep>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, step_index, wave, agent_type, title, description,
                todos_json, files_json, depends_on_json, status,
                session_id, result_summary, created_at, updated_at
         FROM plan_steps
         WHERE run_id = ?1
         ORDER BY wave ASC, step_index ASC",
    )?;

    stmt.query_map([run_id], map_plan_step_row)?
        .map(|r| deserialize_plan_step(r?))
        .collect()
}

/// List all plan steps for a specific wave of a run, ordered by step_index.
pub fn list_for_run_wave(conn: &Connection, run_id: &str, wave: i64) -> GroveResult<Vec<PlanStep>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, step_index, wave, agent_type, title, description,
                todos_json, files_json, depends_on_json, status,
                session_id, result_summary, created_at, updated_at
         FROM plan_steps
         WHERE run_id = ?1 AND wave = ?2
         ORDER BY step_index ASC",
    )?;

    stmt.query_map(params![run_id, wave], map_plan_step_row)?
        .map(|r| deserialize_plan_step(r?))
        .collect()
}

/// Update the status (and optionally session_id / result_summary) of a plan step.
///
/// `session_id` and `summary` use COALESCE so that passing `None` leaves the
/// existing column value unchanged.
pub fn set_status(
    conn: &Connection,
    step_id: &str,
    status: &str,
    session_id: Option<&str>,
    summary: Option<&str>,
) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE plan_steps
         SET status         = ?1,
             session_id     = COALESCE(?2, session_id),
             result_summary = COALESCE(?3, result_summary),
             updated_at     = ?4
         WHERE id = ?5",
        params![status, session_id, summary, now, step_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn open_test_db() -> (rusqlite::Connection, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("tempdir");
        db::initialize(tmp.path()).expect("db init");
        let handle = db::DbHandle::new(tmp.path());
        let conn = handle.connect().expect("connect");
        (conn, tmp)
    }

    fn make_step(id: &str, agent_type: &str) -> GrovePlanStep {
        GrovePlanStep {
            id: id.to_string(),
            agent_type: agent_type.to_string(),
            title: format!("Step {id}"),
            description: String::new(),
            todos: vec![],
            files: vec![],
            depends_on: vec![],
        }
    }

    #[test]
    fn insert_steps_with_wave_offset_stores_correct_wave_values() {
        let (conn, _tmp) = open_test_db();
        let run_id = "run_test_wave_offset";
        // Insert a fake run row so the FK constraint is satisfied.
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'obj', 'executing', 10.0, 0.0, '2024-01-01', '2024-01-01')",
            rusqlite::params![run_id],
        ).expect("insert run");

        let steps = vec![make_step("a", "builder"), make_step("b", "tester")];

        insert_steps(&conn, run_id, &steps, 5).expect("insert_steps");

        let loaded = list_for_run(&conn, run_id).expect("list_for_run");
        assert_eq!(loaded.len(), 2);
        // Both steps have no depends_on so they should be in wave 0 of compute_waves,
        // which is then offset by 5 → stored wave must be 5.
        for ps in &loaded {
            assert_eq!(ps.wave, 5, "wave should be 0 + offset 5");
        }
    }

    #[test]
    fn list_for_run_errors_on_corrupt_todos_json() {
        let (conn, _tmp) = open_test_db();
        let run_id = "run_corrupt_todos";
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'obj', 'executing', 10.0, 0.0, '2024-01-01', '2024-01-01')",
            rusqlite::params![run_id],
        ).expect("insert run");

        // Insert a plan_step with invalid todos_json directly, bypassing insert_steps.
        conn.execute(
            "INSERT INTO plan_steps
             (id, run_id, step_index, wave, agent_type, title, description,
              todos_json, files_json, depends_on_json, status, session_id,
              result_summary, created_at, updated_at)
             VALUES ('ps_corrupt', ?1, 0, 0, 'builder', 'bad step', '',
                     'not-valid-json', '[]', '[]', 'pending', NULL, NULL,
                     '2024-01-01', '2024-01-01')",
            rusqlite::params![run_id],
        )
        .expect("insert corrupt plan_step");

        let result = list_for_run(&conn, run_id);
        assert!(
            result.is_err(),
            "corrupt todos_json must surface as Err, not silently return empty vec"
        );
    }

    #[test]
    fn list_for_run_wave_errors_on_corrupt_files_json() {
        let (conn, _tmp) = open_test_db();
        let run_id = "run_corrupt_files";
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'obj', 'executing', 10.0, 0.0, '2024-01-01', '2024-01-01')",
            rusqlite::params![run_id],
        ).expect("insert run");

        conn.execute(
            "INSERT INTO plan_steps
             (id, run_id, step_index, wave, agent_type, title, description,
              todos_json, files_json, depends_on_json, status, session_id,
              result_summary, created_at, updated_at)
             VALUES ('ps_corrupt_files', ?1, 0, 0, 'tester', 'bad step', '',
                     '[]', '{bad-files-json}', '[]', 'pending', NULL, NULL,
                     '2024-01-01', '2024-01-01')",
            rusqlite::params![run_id],
        )
        .expect("insert corrupt plan_step");

        let result = list_for_run_wave(&conn, run_id, 0);
        assert!(
            result.is_err(),
            "corrupt files_json must surface as Err in list_for_run_wave"
        );
    }

    #[test]
    fn list_for_run_wave_returns_only_matching_wave() {
        let (conn, _tmp) = open_test_db();
        let run_id = "run_test_list_wave";
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'obj', 'executing', 10.0, 0.0, '2024-01-01', '2024-01-01')",
            rusqlite::params![run_id],
        ).expect("insert run");

        // Wave 0 steps (offset=0)
        insert_steps(&conn, run_id, &[make_step("x", "builder")], 0).expect("wave 0");
        // Wave 3 steps (offset=3)
        insert_steps(
            &conn,
            run_id,
            &[make_step("y", "tester"), make_step("z", "reviewer")],
            3,
        )
        .expect("wave 3");

        let wave0 = list_for_run_wave(&conn, run_id, 0).expect("wave 0 query");
        assert_eq!(wave0.len(), 1);
        assert_eq!(wave0[0].agent_type, "builder");

        let wave3 = list_for_run_wave(&conn, run_id, 3).expect("wave 3 query");
        assert_eq!(wave3.len(), 2);

        let wave9 = list_for_run_wave(&conn, run_id, 9).expect("wave 9 query");
        assert!(wave9.is_empty());
    }
}
