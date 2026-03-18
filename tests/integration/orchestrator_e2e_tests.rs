/// End-to-end integration tests for the full orchestrator cycle.
///
/// These tests exercise `execute_objective` end-to-end with a `MockProvider`,
/// asserting on DB state, emitted events, and plan shape without touching
/// a real Claude CLI.
use std::sync::Arc;

use grove_core::config::{DEFAULT_CONFIG_YAML, GroveConfig};
use grove_core::db;
use grove_core::events;
use grove_core::merge;
use grove_core::orchestrator::abort_handle::AbortHandle;
use grove_core::orchestrator::{RunOptions, execute_objective};
use grove_core::providers::{MockProvider, Provider};
use tempfile::TempDir;

fn setup() -> (TempDir, GroveConfig, Arc<dyn Provider>) {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let mut cfg: GroveConfig = serde_yaml::from_str(DEFAULT_CONFIG_YAML).unwrap();
    // Disable Claude Code provider so planning uses hardcoded fallback (no CLI calls).
    cfg.providers.claude_code.enabled = false;
    // Minimal pipeline: Builder + Tester only (fast, no planner agent needed).
    cfg.orchestration.enforce_design_first = false;
    cfg.agents.reviewer.enabled = false;
    cfg.agents.qa.enabled = false;
    cfg.agents.security.enabled = false;
    cfg.agents.validator.enabled = false;
    cfg.agents.prd.enabled = false;
    cfg.agents.spec.enabled = false;
    cfg.agents.documenter.enabled = false;
    cfg.agents.reporter.enabled = false;
    cfg.agents.compliance.enabled = false;
    cfg.agents.dependency_manager.enabled = false;
    cfg.agents.optimizer.enabled = false;
    cfg.agents.accessibility.enabled = false;
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    (dir, cfg, provider)
}

fn test_options() -> RunOptions {
    RunOptions {
        budget_usd: None,
        max_agents: None,
        model: None,
        interactive: false,
        pause_after: vec![],
        disable_phase_gates: false,
        permission_mode: None,
        pipeline: None,
        conversation_id: None,
        continue_last: false,
        db_path: None,
        abort_handle: None,
        issue_id: None,
        issue: None,
        provider: None,
        on_run_created: None,
        resume_provider_session_id: None,
        input_handle_callback: None,
        run_control_callback: None,
    }
}

// ── Basic lifecycle ────────────────────────────────────────────────────────────

#[test]
fn run_returns_completed_state() {
    let (dir, cfg, provider) = setup();
    let result = execute_objective(
        dir.path(),
        &cfg,
        "build a widget",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();
    assert_eq!(result.state, "completed");
    assert!(!result.run_id.is_empty());
    assert_eq!(result.objective, "build a widget");
}

#[test]
fn run_plan_is_non_empty() {
    let (dir, cfg, provider) = setup();
    let result = execute_objective(
        dir.path(),
        &cfg,
        "implement auth",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();
    assert!(!result.plan.is_empty(), "run must return a non-empty plan");
}

// ── Event assertions ───────────────────────────────────────────────────────────

#[test]
fn run_emits_run_created_event() {
    let (dir, cfg, provider) = setup();
    let result = execute_objective(
        dir.path(),
        &cfg,
        "event test",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let evts = events::list_for_run(&conn, &result.run_id).unwrap();
    let types: Vec<&str> = evts.iter().map(|e| e.event_type.as_str()).collect();

    assert!(
        types.contains(&"run_created"),
        "run_created event must be emitted; got: {types:?}"
    );
}

#[test]
fn run_emits_plan_generated_event() {
    let (dir, cfg, provider) = setup();
    let result = execute_objective(
        dir.path(),
        &cfg,
        "plan event test",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let evts = events::list_for_run(&conn, &result.run_id).unwrap();
    let types: Vec<&str> = evts.iter().map(|e| e.event_type.as_str()).collect();

    assert!(
        types.contains(&"plan_generated"),
        "plan_generated event must be emitted; got: {types:?}"
    );
}

#[test]
fn run_emits_run_completed_event() {
    let (dir, cfg, provider) = setup();
    let result = execute_objective(
        dir.path(),
        &cfg,
        "complete event test",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let evts = events::list_for_run(&conn, &result.run_id).unwrap();
    let types: Vec<&str> = evts.iter().map(|e| e.event_type.as_str()).collect();

    assert!(
        types.contains(&"run_completed"),
        "run_completed event must be emitted; got: {types:?}"
    );
}

// ── Per-conversation run lock ──────────────────────────────────────────────────

#[test]
fn per_conversation_lock_blocks_same_conversation() {
    let (dir, cfg, provider) = setup();

    // Run to create a conversation and get its ID.
    let r1 = execute_objective(
        dir.path(),
        &cfg,
        "first",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let conv_id: String = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&r1.run_id],
            |r| r.get(0),
        )
        .unwrap();

    // Insert a fake "executing" run on the SAME conversation.
    // Use a recent timestamp so crash recovery doesn't auto-fail it (5-min threshold).
    let recent = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at,conversation_id)
         VALUES('blocker','in progress','executing',1.0,0,?1,?1,?2)",
        rusqlite::params![recent, conv_id],
    )
    .unwrap();
    drop(conn);

    // Try to run on the same conversation — must be blocked.
    let mut opts = test_options();
    opts.conversation_id = Some(conv_id.clone());
    let result = execute_objective(dir.path(), &cfg, "should fail", opts, Arc::clone(&provider));
    assert!(
        result.is_err(),
        "must reject when same conversation has an active run"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("blocker") && msg.contains("already in progress on this conversation"),
        "error must mention the active run and conversation scope; got: {msg}"
    );
}

#[test]
fn per_conversation_lock_allows_different_conversation() {
    let (dir, cfg, provider) = setup();

    // Run to create a conversation.
    let r1 = execute_objective(
        dir.path(),
        &cfg,
        "first",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let conv_id: String = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&r1.run_id],
            |r| r.get(0),
        )
        .unwrap();

    // Insert a fake "executing" run on conv_id.
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at,conversation_id)
         VALUES('blocker','in progress','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z',?1)",
        [&conv_id],
    )
    .unwrap();
    drop(conn);

    // Execute with conversation_id=None → creates a NEW conversation → NOT blocked.
    let result = execute_objective(
        dir.path(),
        &cfg,
        "different conv",
        test_options(),
        Arc::clone(&provider),
    );
    assert!(
        result.is_ok(),
        "different conversation must not be blocked; got: {:?}",
        result.err()
    );
}

#[test]
fn legacy_null_conversation_runs_do_not_block_new_runs() {
    let (dir, cfg, provider) = setup();
    let conn = db::DbHandle::new(dir.path()).connect().unwrap();

    // Insert a legacy run with NULL conversation_id (pre-F10 runs).
    conn.execute(
        "INSERT INTO runs(id,objective,state,budget_usd,cost_used_usd,created_at,updated_at)
         VALUES('legacy','old run','executing',1.0,0,'2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
        [],
    )
    .unwrap();
    drop(conn);

    // New run with a fresh conversation → not blocked by the legacy NULL run.
    let result = execute_objective(
        dir.path(),
        &cfg,
        "new run",
        test_options(),
        Arc::clone(&provider),
    );
    assert!(
        result.is_ok(),
        "legacy NULL conversation_id runs must not block new runs; got: {:?}",
        result.err()
    );
}

// ── Conversation thread ───────────────────────────────────────────────────────

#[test]
fn run_creates_conversation_and_user_message() {
    let (dir, cfg, provider) = setup();
    let result = execute_objective(
        dir.path(),
        &cfg,
        "conv test",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();

    // Run should have a conversation_id
    let conv_id: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&result.run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(conv_id.is_some(), "run must be linked to a conversation");

    let conv_id = conv_id.unwrap();
    // Conversation must exist
    let conv_state: String = conn
        .query_row(
            "SELECT state FROM conversations WHERE id=?1",
            [&conv_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(conv_state, "active");

    // User message must exist
    let msg_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE conversation_id=?1 AND role='user'",
            [&conv_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(msg_count >= 1, "at least one user message must be recorded");

    let msg_content: String = conn
        .query_row(
            "SELECT content FROM messages WHERE conversation_id=?1 AND role='user' ORDER BY created_at ASC LIMIT 1",
            [&conv_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(msg_content, "conv test");
}

#[test]
fn continue_last_reuses_conversation() {
    let (dir, cfg, provider) = setup();

    // First run — creates a conversation
    let r1 = execute_objective(
        dir.path(),
        &cfg,
        "first run",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();

    // Second run with continue_last
    let mut opts = test_options();
    opts.continue_last = true;
    let r2 =
        execute_objective(dir.path(), &cfg, "second run", opts, Arc::clone(&provider)).unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let conv1: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&r1.run_id],
            |r| r.get(0),
        )
        .unwrap();
    let conv2: Option<String> = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&r2.run_id],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(
        conv1, conv2,
        "continue_last should reuse the same conversation"
    );
}

// ── Run is persisted in DB ─────────────────────────────────────────────────────

#[test]
fn run_persists_run_record_in_db() {
    let (dir, cfg, provider) = setup();
    let result = execute_objective(
        dir.path(),
        &cfg,
        "persist test",
        test_options(),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(dir.path()).connect().unwrap();
    let state: String = conn
        .query_row(
            "SELECT state FROM runs WHERE id=?1",
            [&result.run_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "completed");
}

// ── Merge queue ──────────────────────────────────────────────────────────────

#[test]
fn merge_queue_enqueue_dequeue_fifo() {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let mut conn = db::DbHandle::new(dir.path()).connect().unwrap();

    // Insert conversations to satisfy FK constraints.
    for cid in ["conv_a", "conv_b"] {
        conn.execute(
            "INSERT INTO conversations(id,project_id,state,created_at,updated_at)
             VALUES(?1,'proj1','active','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
            [cid],
        )
        .unwrap();
    }

    // Enqueue two entries.
    merge::queue::enqueue(&mut conn, "conv_a", "grove/s_a", "main", "direct").unwrap();
    merge::queue::enqueue(&mut conn, "conv_b", "grove/s_b", "main", "direct").unwrap();

    // Dequeue should return FIFO order.
    let first = merge::queue::dequeue_next(&mut conn)
        .unwrap()
        .expect("first entry");
    assert_eq!(first.conversation_id, "conv_a");
    assert_eq!(first.status, "queued"); // row snapshot before update
    merge::queue::mark_done(&conn, first.id).unwrap();

    let second = merge::queue::dequeue_next(&mut conn)
        .unwrap()
        .expect("second entry");
    assert_eq!(second.conversation_id, "conv_b");
    merge::queue::mark_done(&conn, second.id).unwrap();

    // Queue is now empty.
    let none = merge::queue::dequeue_next(&mut conn).unwrap();
    assert!(none.is_none(), "queue should be empty after draining");
}

// ── Abort handle map ─────────────────────────────────────────────────────────

#[test]
fn abort_handle_map_independent_keys() {
    let h1 = AbortHandle::new();
    let h2 = AbortHandle::new();

    // Set two handles.
    let map = std::sync::Mutex::new(std::collections::HashMap::<String, AbortHandle>::new());
    map.lock().unwrap().insert("conv_1".to_string(), h1);
    map.lock().unwrap().insert("conv_2".to_string(), h2);

    // Take one — the other must remain.
    let taken = map.lock().unwrap().remove("conv_1");
    assert!(taken.is_some(), "conv_1 handle must exist");

    let remaining = map.lock().unwrap().contains_key("conv_2");
    assert!(
        remaining,
        "conv_2 handle must still be present after removing conv_1"
    );

    // Take the other.
    let taken2 = map.lock().unwrap().remove("conv_2");
    assert!(taken2.is_some(), "conv_2 handle must exist");
    assert!(
        map.lock().unwrap().is_empty(),
        "map should be empty after removing all"
    );
}

// ── Markdown config → engine path ────────────────────────────────────────────

#[test]
fn markdown_agent_config_drives_instructions_and_scope() {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();

    // Create skills/agents/build_prd.md with scope config
    let agents_dir = dir.path().join("skills").join("agents");
    std::fs::create_dir_all(&agents_dir).unwrap();
    std::fs::write(
        agents_dir.join("build_prd.md"),
        r#"---
id: build_prd
name: Build PRD
description: Test agent
can_write: true
can_run_commands: false
artifact: "GROVE_PRD_{run_id}.md"
allowed_tools:
  - Read
  - Write
skills: []
---

# Build PRD Agent

Objective: {objective}
Write `{artifact_filename}`.
"#,
    )
    .unwrap();

    // Load and verify
    let configs = grove_core::config::agent_config::load_all(dir.path()).unwrap();
    assert!(configs.agents.contains_key("build_prd"));

    let agent_config = configs.agents.get("build_prd").unwrap();
    assert_eq!(agent_config.name, "Build PRD");
    assert!(agent_config.can_write);
    assert!(!agent_config.can_run_commands);
    assert_eq!(agent_config.allowed_tools.as_ref().unwrap().len(), 2);

    // Build instructions with preloaded configs and verify template replacement
    let instructions = grove_core::config::agent_config::build_instructions_from_config(
        grove_core::agents::AgentType::BuildPrd,
        "Add auth",
        "abc12345def67890",
        dir.path(),
        None,
        dir.path(),
        Some(&configs),
    );
    assert!(
        instructions.contains("Add auth"),
        "objective should be interpolated"
    );
    assert!(
        instructions.contains("GROVE_PRD_abc12345.md"),
        "artifact filename should be interpolated"
    );
}

// ── Pipeline DB round-trip ────────────────────────────────────────────────────

#[test]
fn pipeline_survives_db_round_trip() {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();

    // Queue a task with pipeline="plan"
    let task = grove_core::orchestrator::queue_task(
        dir.path(),
        "test pipeline round-trip",
        None,
        0,
        None,
        None,
        None,
        None,
        Some("plan"),
        None,
        false,
    )
    .unwrap();

    assert_eq!(task.pipeline.as_deref(), Some("plan"));

    // Read it back from DB
    let tasks = grove_core::orchestrator::list_tasks(dir.path()).unwrap();
    let our_task = tasks.iter().find(|t| t.id == task.id).unwrap();
    assert_eq!(our_task.pipeline.as_deref(), Some("plan"));

    // Verify PipelineKind parse
    let kind = our_task
        .pipeline
        .as_deref()
        .and_then(grove_core::orchestrator::pipeline::PipelineKind::from_str);
    assert_eq!(
        kind,
        Some(grove_core::orchestrator::pipeline::PipelineKind::Plan)
    );
}

#[test]
fn pipeline_none_defaults_to_autonomous() {
    let kind = None::<&str>
        .and_then(grove_core::orchestrator::pipeline::PipelineKind::from_str)
        .unwrap_or_default();
    assert_eq!(
        kind,
        grove_core::orchestrator::pipeline::PipelineKind::Autonomous
    );
}
