//! Verify critical queries use indexes via EXPLAIN QUERY PLAN.
//!
//! Each test checks that a hot query path does NOT perform a full table scan.
//! SQLite EXPLAIN QUERY PLAN outputs rows with the format:
//!   (id INTEGER, parent INTEGER, notused INTEGER, detail TEXT)
//!
//! The `detail` column contains human-readable plan text such as:
//!   "SCAN runs"                     → full table scan (bad)
//!   "SEARCH runs USING INDEX …"     → index lookup / range scan (good)
//!   "SEARCH runs USING COVERING …"  → covering index (good)
//!
//! Rule: any detail line that contains "SCAN" but not "USING" is a full table
//! scan and fails the assertion.

const SQL_0001: &str = include_str!("../../migrations/0001_init.sql");

fn open_db() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(SQL_0001).unwrap();
    conn
}

/// Seed the minimum rows required by FK constraints used in tests.
///
/// Inserts one workspace, project, conversation, and run.
/// Returns (workspace_id, project_id, conversation_id, run_id) as static strs.
fn seed_base(conn: &rusqlite::Connection) {
    let now = "2024-01-01T00:00:00Z";

    conn.execute(
        "INSERT INTO workspaces(id,name,state,credits_usd,created_at,updated_at)
         VALUES('ws1','Test WS','active',0.0,?1,?1)",
        [now],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO projects(id,workspace_id,name,root_path,state,source_kind,created_at,updated_at)
         VALUES('proj1','ws1','Test Project','/tmp/p','active','local',?1,?1)",
        [now],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO conversations(id,project_id,state,created_at,updated_at)
         VALUES('conv1','proj1','active',?1,?1)",
        [now],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,conversation_id,created_at,updated_at)
         VALUES('run1','test obj','created',1.0,0,'conv1',?1,?1)",
        [now],
    )
    .unwrap();
}

/// Assert that no step in `plans` is a full table scan.
fn assert_no_full_scan(sql: &str, plans: &[String]) {
    assert!(
        !plans.is_empty(),
        "EXPLAIN QUERY PLAN returned no rows for SQL: {sql}"
    );
    for plan in plans {
        let upper = plan.to_uppercase();
        // "SCAN <table>" without "USING" means a full table scan.
        // "SCAN <table> USING INDEX …" is acceptable.
        let is_full_scan = upper.contains("SCAN") && !upper.contains("USING");
        assert!(
            !is_full_scan,
            "Query performs a full table scan.\nSQL: {sql}\nPlan line: {plan}\nFull plan: {plans:?}"
        );
    }
}

// ── runs_by_conversation uses idx_runs_conversation ───────────────────────────

#[test]
fn runs_by_conversation_uses_index() {
    let conn = open_db();
    seed_base(&conn);

    let sql = "SELECT id, objective, state, budget_usd, cost_used_usd, publish_status,
                      publish_error, final_commit_sha, pr_url, published_at,
                      created_at, updated_at, conversation_id, provider, model, provider_thread_id
               FROM runs
               WHERE conversation_id = ?1
               ORDER BY created_at DESC";

    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let plans: Vec<String> = stmt
        .query_map(["conv1"], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_no_full_scan(sql, &plans);
}

// ── issues_by_project uses idx_issues_project_updated ─────────────────────────

#[test]
fn issues_by_project_uses_index() {
    let conn = open_db();
    seed_base(&conn);

    // Mirrors issues_repo::list: filter by project_id ordered by updated_at DESC.
    let sql = "SELECT id, project_id, title, status, canonical_status, updated_at
               FROM issues
               WHERE project_id = ?1
               ORDER BY updated_at DESC
               LIMIT 100";

    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let plans: Vec<String> = stmt
        .query_map(["proj1"], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_no_full_scan(sql, &plans);
}

// ── events_by_run uses idx_events_run_created ─────────────────────────────────

#[test]
fn events_by_run_uses_index() {
    let conn = open_db();
    seed_base(&conn);

    // Mirrors the typical event listing pattern: events for a run in order.
    let sql = "SELECT id, run_id, session_id, type, payload_json, created_at
               FROM events
               WHERE run_id = ?1
               ORDER BY created_at ASC";

    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let plans: Vec<String> = stmt
        .query_map(["run1"], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_no_full_scan(sql, &plans);
}

// ── messages_by_conversation uses idx_messages_conversation ──────────────────

#[test]
fn messages_by_conversation_uses_index() {
    let conn = open_db();
    seed_base(&conn);

    // Mirrors messages_repo::list_for_conversation.
    let sql = "SELECT id, conversation_id, run_id, role, agent_type, session_id,
                      content, created_at, user_id
               FROM messages
               WHERE conversation_id = ?1
               ORDER BY created_at ASC
               LIMIT 500";

    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let plans: Vec<String> = stmt
        .query_map(["conv1"], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_no_full_scan(sql, &plans);
}

// ── sessions_by_run uses idx_sessions_run_state ───────────────────────────────

#[test]
fn sessions_by_run_uses_index() {
    let conn = open_db();
    seed_base(&conn);

    // Mirrors sessions_repo::list_for_run.
    let sql = "SELECT id, run_id, agent_type, state, worktree_path,
                      started_at, ended_at, created_at, updated_at, provider_session_id,
                      last_heartbeat, stalled_since, checkpoint_sha, parent_checkpoint_sha
               FROM sessions
               WHERE run_id = ?1
               ORDER BY created_at ASC";

    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let plans: Vec<String> = stmt
        .query_map(["run1"], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_no_full_scan(sql, &plans);
}

// ── runs_by_state uses idx_runs_state ─────────────────────────────────────────

#[test]
fn runs_by_state_uses_index() {
    let conn = open_db();
    seed_base(&conn);

    // Mirrors watchdog / orchestrator pattern: find active runs by state.
    let sql = "SELECT id, state FROM runs WHERE state = ?1";

    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let plans: Vec<String> = stmt
        .query_map(["executing"], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_no_full_scan(sql, &plans);
}

// ── issues count_open uses idx_issues_project ─────────────────────────────────

#[test]
fn issues_count_open_by_project_uses_index() {
    let conn = open_db();
    seed_base(&conn);

    // Mirrors issues_repo::count_open.
    let sql = "SELECT COUNT(*) FROM issues
               WHERE project_id = ?1
                 AND canonical_status IN ('open', 'in_progress', 'in_review', 'blocked')";

    let explain_sql = format!("EXPLAIN QUERY PLAN {sql}");
    let mut stmt = conn.prepare(&explain_sql).unwrap();
    let plans: Vec<String> = stmt
        .query_map(["proj1"], |row| row.get::<_, String>(3))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert_no_full_scan(sql, &plans);
}

// ── Stress test: list runs with 1000 rows must complete under 200ms ───────────

#[test]
fn list_runs_with_1000_rows_under_200ms() {
    let conn = open_db();
    seed_base(&conn);

    // Insert 1000 additional runs tied to the same conversation.
    for i in 0_u32..1000 {
        // Produce distinct, monotonically increasing timestamps.
        let secs = i % 60;
        let mins = (i / 60) % 60;
        let hours = (i / 3600) % 24;
        // All fit in day 01 for i < 86400; sufficient for 1000 rows.
        let ts = format!("2024-02-01T{hours:02}:{mins:02}:{secs:02}Z");
        conn.execute(
            "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,
                              conversation_id,created_at,updated_at)
             VALUES(?1,'stress obj','completed',1.0,0.5,'conv1',?2,?2)",
            rusqlite::params![format!("run-stress-{i}"), ts],
        )
        .unwrap();
    }

    let start = std::time::Instant::now();
    let mut stmt = conn
        .prepare(
            "SELECT id, objective, state, budget_usd, cost_used_usd, publish_status,
                    publish_error, final_commit_sha, pr_url, published_at,
                    created_at, updated_at, conversation_id, provider, model, provider_thread_id
             FROM runs
             ORDER BY created_at DESC
             LIMIT 100",
        )
        .unwrap();
    let count = stmt.query_map([], |_| Ok(())).unwrap().count();
    let elapsed = start.elapsed();

    assert_eq!(count, 100, "expected 100 rows back from LIMIT 100");
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "list_runs with 1001 rows took {elapsed:?}, expected < 200ms"
    );
}

// ── Stress test: issues for a project with 1000 rows under 200ms ──────────────

#[test]
fn list_issues_with_1000_rows_under_200ms() {
    let conn = open_db();
    seed_base(&conn);

    // Insert 1000 issues for proj1 with distinct updated_at timestamps.
    for i in 0_u32..1000 {
        let secs = i % 60;
        let mins = (i / 60) % 60;
        let ts = format!("2024-03-01T00:{mins:02}:{secs:02}Z");
        conn.execute(
            "INSERT INTO issues(id,project_id,title,status,canonical_status,
                                provider,is_native,created_at,updated_at)
             VALUES(?1,'proj1',?2,'open','open','grove',1,?3,?3)",
            rusqlite::params![format!("issue-{i}"), format!("Issue {i}"), ts],
        )
        .unwrap();
    }

    let start = std::time::Instant::now();
    let mut stmt = conn
        .prepare(
            "SELECT id, project_id, title, status, canonical_status, updated_at
             FROM issues
             WHERE project_id = 'proj1'
             ORDER BY updated_at DESC
             LIMIT 100",
        )
        .unwrap();
    let count = stmt.query_map([], |_| Ok(())).unwrap().count();
    let elapsed = start.elapsed();

    assert_eq!(count, 100, "expected 100 rows back from LIMIT 100");
    assert!(
        elapsed < std::time::Duration::from_millis(200),
        "list_issues with 1000 rows took {elapsed:?}, expected < 200ms"
    );
}
