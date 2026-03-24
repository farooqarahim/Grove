/// End-to-end integration tests for the full workflow chain:
///
///   conversation → run → session → agent → messages
///
/// Also exercises F13 features: watchdog heartbeats, signals, hooks, and issue tracker.
/// Uses direct DB operations (no real provider) to verify the data model integrity
/// across the entire conversation-to-agent pipeline.
use chrono::Utc;
use rusqlite::{Connection, params};
use serde_json::json;
use std::collections::HashMap;

use grove_core::config::{CapabilityGuard, HookDefinition, HookEvent, HooksConfig, WatchdogConfig};
use grove_core::db;
use grove_core::db::DbHandle;
use grove_core::db::repositories::{conversations_repo, messages_repo};
use grove_core::events;
use grove_core::hooks::{self, HookContext};
use grove_core::orchestrator::conversation;
use grove_core::signals::{self, SignalPriority, SignalType};
use grove_core::tracker;
use grove_core::watchdog;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn setup_db() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let conn = DbHandle::new(dir.path()).connect().unwrap();
    (dir, conn)
}

fn insert_run(conn: &Connection, run_id: &str, conversation_id: Option<&str>) {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO runs (id, conversation_id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
         VALUES (?1, ?2, 'e2e objective', 'executing', 5.0, 0.0, ?3, ?3)",
        params![run_id, conversation_id, now],
    )
    .unwrap();
}

fn insert_session(conn: &Connection, session_id: &str, run_id: &str, agent_type: &str) {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sessions (id, run_id, agent_type, state, worktree_path, started_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'running', '/tmp/wt', ?4, ?4, ?4)",
        params![session_id, run_id, agent_type, now],
    )
    .unwrap();
}

// ── Test: Full Conversation → Run → Session → Messages Chain ─────────────────

#[test]
fn full_conversation_to_messages_chain() {
    let (dir, mut conn) = setup_db();

    // 1. Resolve a new conversation
    let conv_id = conversation::resolve_conversation(
        &mut conn,
        dir.path(),
        None,
        false,
        Some("grove"),
        None,
        conversation::RUN_CONVERSATION_KIND,
    )
    .unwrap();
    assert_eq!(conv_id.len(), 32); // plain UUID simple format

    // 2. Create a run linked to the conversation
    let run_id = "run_e2e_full";
    insert_run(&conn, run_id, Some(&conv_id));

    // 3. Record the user's objective as a message
    conversation::record_user_message(&mut conn, &conv_id, run_id, "build a REST API").unwrap();

    // 4. Create sessions (architect + builder) and record agent messages
    insert_session(&conn, "sess_arch", run_id, "architect");
    conversation::record_agent_message(
        &mut conn,
        &conv_id,
        run_id,
        "architect",
        "sess_arch",
        "Designed API with 3 endpoints: GET /users, POST /users, DELETE /users/:id",
    )
    .unwrap();

    insert_session(&conn, "sess_build", run_id, "builder");
    conversation::record_agent_message(
        &mut conn,
        &conv_id,
        run_id,
        "builder",
        "sess_build",
        "Implemented all 3 endpoints with full error handling",
    )
    .unwrap();

    // 5. Verify: messages by conversation
    let conv_msgs = messages_repo::list_for_conversation(&conn, &conv_id, 100).unwrap();
    assert_eq!(conv_msgs.len(), 3, "expected 1 user + 2 agent messages");
    assert_eq!(conv_msgs[0].role, "user");
    assert_eq!(conv_msgs[0].content, "build a REST API");
    assert_eq!(conv_msgs[1].role, "agent");
    assert_eq!(conv_msgs[1].agent_type, Some("architect".to_string()));
    assert_eq!(conv_msgs[2].role, "agent");
    assert_eq!(conv_msgs[2].agent_type, Some("builder".to_string()));

    // 6. Verify: messages by run
    let run_msgs = messages_repo::list_for_run(&conn, run_id).unwrap();
    assert_eq!(run_msgs.len(), 3);

    // 7. Verify: conversation is queryable
    let row = conversations_repo::get(&conn, &conv_id).unwrap();
    assert_eq!(row.state, "active");
}

// ── Test: Continue Conversation Across Runs ──────────────────────────────────

#[test]
fn continue_conversation_across_runs() {
    let (dir, mut conn) = setup_db();

    // First run in a new conversation
    let conv_id = conversation::resolve_conversation(
        &mut conn,
        dir.path(),
        None,
        false,
        Some("grove"),
        None,
        conversation::RUN_CONVERSATION_KIND,
    )
    .unwrap();
    let run1 = "run_multi_1";
    insert_run(&conn, run1, Some(&conv_id));
    conversation::record_user_message(&mut conn, &conv_id, run1, "create user model").unwrap();
    insert_session(&conn, "sess_r1", run1, "builder");
    conversation::record_agent_message(
        &mut conn,
        &conv_id,
        run1,
        "builder",
        "sess_r1",
        "Created User struct",
    )
    .unwrap();

    // Complete run1 before starting run2 (partial unique index enforces
    // at most one active run per conversation).
    conn.execute(
        "UPDATE runs SET state = 'completed' WHERE id = ?1",
        params![run1],
    )
    .unwrap();

    // Second run continues the same conversation
    let continued_id = conversation::resolve_conversation(
        &mut conn,
        dir.path(),
        None,
        true,
        Some("grove"),
        None,
        conversation::RUN_CONVERSATION_KIND,
    )
    .unwrap();
    assert_eq!(
        continued_id, conv_id,
        "continue_last should reuse conversation"
    );

    let run2 = "run_multi_2";
    insert_run(&conn, run2, Some(&conv_id));
    conversation::record_user_message(&mut conn, &conv_id, run2, "add email validation").unwrap();
    insert_session(&conn, "sess_r2", run2, "builder");
    conversation::record_agent_message(
        &mut conn,
        &conv_id,
        run2,
        "builder",
        "sess_r2",
        "Added email regex validation",
    )
    .unwrap();

    // All 4 messages visible in the conversation
    let all_msgs = messages_repo::list_for_conversation(&conn, &conv_id, 100).unwrap();
    assert_eq!(all_msgs.len(), 4, "2 user + 2 agent across 2 runs");

    // But per-run queries are scoped correctly
    let r1_msgs = messages_repo::list_for_run(&conn, run1).unwrap();
    assert_eq!(r1_msgs.len(), 2);
    let r2_msgs = messages_repo::list_for_run(&conn, run2).unwrap();
    assert_eq!(r2_msgs.len(), 2);
}

// ── Test: Watchdog Heartbeats During Workflow ────────────────────────────────

#[test]
fn watchdog_heartbeats_in_workflow() {
    let (_dir, conn) = setup_db();
    let run_id = "run_wd_e2e";
    insert_run(&conn, run_id, None);

    let now = Utc::now();
    let started = (now - chrono::Duration::seconds(10)).to_rfc3339();
    insert_session(&conn, "sess_wd1", run_id, "builder");
    conn.execute(
        "UPDATE sessions SET started_at = ?1 WHERE id = 'sess_wd1'",
        [&started],
    )
    .unwrap();

    // Before heartbeat: session just started, should be healthy (within boot window)
    let cfg = WatchdogConfig::default();
    let actions = watchdog::poll_sessions(&conn, run_id, &cfg, &now.to_rfc3339()).unwrap();
    assert_eq!(actions, vec![watchdog::WatchdogAction::Healthy]);

    // Touch heartbeat (simulates agent completing a step)
    watchdog::touch_heartbeat(&conn, "sess_wd1").unwrap();

    // Verify heartbeat was recorded
    let hb: Option<String> = conn
        .query_row(
            "SELECT last_heartbeat FROM sessions WHERE id = 'sess_wd1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(hb.is_some(), "heartbeat should be set after touch");

    // Poll again — still healthy
    let actions2 = watchdog::poll_sessions(&conn, run_id, &cfg, &Utc::now().to_rfc3339()).unwrap();
    assert_eq!(actions2, vec![watchdog::WatchdogAction::Healthy]);
}

// ── Test: Signals Between Agents During Workflow ─────────────────────────────

#[test]
fn signals_between_agents_in_workflow() {
    let (_dir, conn) = setup_db();
    let run_id = "run_sig_e2e";
    insert_run(&conn, run_id, None);

    // Set up two running sessions
    insert_session(&conn, "sess_arch_sig", run_id, "architect");
    insert_session(&conn, "sess_build_sig", run_id, "builder");

    // Architect sends DesignReady signal to builder
    let sig_id = signals::send_signal(
        &conn,
        run_id,
        "architect",
        "builder",
        SignalType::DesignReady,
        SignalPriority::Normal,
        json!({"endpoints": ["/users", "/posts"]}),
    )
    .unwrap();
    assert!(sig_id.starts_with("sig_"));

    // Builder checks for unread signals
    let inbox = signals::check_signals(&conn, run_id, "builder").unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].from_agent, "architect");
    assert_eq!(inbox[0].signal_type, "design_ready");
    assert!(!inbox[0].read);

    // Builder marks signal as read
    signals::mark_read(&conn, &sig_id).unwrap();

    // No more unread signals for builder
    let inbox2 = signals::check_signals(&conn, run_id, "builder").unwrap();
    assert!(inbox2.is_empty());

    // Builder sends WorkerDone back to architect
    signals::send_signal(
        &conn,
        run_id,
        "builder",
        "architect",
        SignalType::WorkerDone,
        SignalPriority::Normal,
        json!({"files_changed": 5}),
    )
    .unwrap();

    // All signals visible via list_for_run
    let all = signals::list_for_run(&conn, run_id).unwrap();
    assert_eq!(all.len(), 2);

    // Verify signal events were emitted
    let evts = events::list_for_run(&conn, run_id).unwrap();
    let signal_events: Vec<&str> = evts
        .iter()
        .filter(|e| e.event_type.starts_with("signal_"))
        .map(|e| e.event_type.as_str())
        .collect();
    assert!(
        signal_events.contains(&"signal_sent"),
        "signal_sent events should be emitted"
    );
}

// ── Test: Broadcast Signals to All Agents ────────────────────────────────────

#[test]
fn broadcast_signals_to_all_agents() {
    let (_dir, conn) = setup_db();
    let run_id = "run_bcast_e2e";
    insert_run(&conn, run_id, None);

    // 3 running sessions
    insert_session(&conn, "sess_a", run_id, "architect");
    insert_session(&conn, "sess_b", run_id, "builder");
    insert_session(&conn, "sess_t", run_id, "tester");

    // Architect broadcasts to @all
    let ids = signals::broadcast(
        &conn,
        run_id,
        "architect",
        signals::GROUP_ALL,
        SignalType::Status,
        SignalPriority::High,
        json!({"phase": "design complete"}),
    )
    .unwrap();

    // Should exclude sender (architect)
    assert_eq!(
        ids.len(),
        2,
        "broadcast should reach builder + tester, not architect"
    );

    // Builder receives the broadcast
    let builder_inbox = signals::check_signals(&conn, run_id, "builder").unwrap();
    assert_eq!(builder_inbox.len(), 1);
    assert_eq!(builder_inbox[0].from_agent, "architect");

    // Tester receives the broadcast
    let tester_inbox = signals::check_signals(&conn, run_id, "tester").unwrap();
    assert_eq!(tester_inbox.len(), 1);

    // Architect does NOT receive their own broadcast
    let arch_inbox = signals::check_signals(&conn, run_id, "architect").unwrap();
    assert!(arch_inbox.is_empty());
}

// ── Test: Hooks Guard System ─────────────────────────────────────────────────

#[test]
fn hooks_guard_file_and_tool_checks() {
    // Set up guards: builder can only touch src/**, tester can't use Bash
    let mut guards = HashMap::new();
    guards.insert(
        "builder".to_string(),
        CapabilityGuard {
            allowed_paths: vec!["src/**".to_string()],
            blocked_paths: vec!["src/secrets/**".to_string()],
            blocked_tools: vec![],
        },
    );
    guards.insert(
        "tester".to_string(),
        CapabilityGuard {
            allowed_paths: vec![],
            blocked_paths: vec![],
            blocked_tools: vec!["Bash".to_string(), "Write".to_string()],
        },
    );

    // Builder file guards
    assert!(hooks::check_file_guard(&guards, "builder", "src/main.rs"));
    assert!(hooks::check_file_guard(
        &guards,
        "builder",
        "src/lib/mod.rs"
    ));
    assert!(!hooks::check_file_guard(
        &guards,
        "builder",
        "config/app.yaml"
    ));
    assert!(!hooks::check_file_guard(
        &guards,
        "builder",
        "src/secrets/api_key.txt"
    ));

    // Tester tool guards
    assert!(!hooks::check_tool_guard(&guards, "tester", "Bash"));
    assert!(!hooks::check_tool_guard(&guards, "tester", "Write"));
    assert!(hooks::check_tool_guard(&guards, "tester", "Read"));
    assert!(hooks::check_tool_guard(&guards, "tester", "Glob"));

    // Unguarded agent type has no restrictions
    assert!(hooks::check_file_guard(&guards, "reviewer", "anything.txt"));
    assert!(hooks::check_tool_guard(&guards, "reviewer", "Bash"));
}

// ── Test: Hook Execution (non-blocking) ──────────────────────────────────────

#[test]
fn hooks_execute_non_blocking_on_post_run() {
    let mut on = HashMap::new();
    on.insert(
        HookEvent::PostRun,
        vec![HookDefinition {
            command: "true".into(), // always succeeds
            blocking: false,
            timeout_secs: 5,
        }],
    );
    let cfg = HooksConfig {
        post_run: vec![],
        on,
        guards: HashMap::new(),
    };
    let ctx = HookContext {
        run_id: "run_hook_e2e".into(),
        session_id: None,
        agent_type: None,
        worktree_path: None,
        event: HookEvent::PostRun,
    };
    let tmp = tempfile::tempdir().unwrap();
    let result = hooks::run_hooks(&cfg, HookEvent::PostRun, &ctx, tmp.path());
    assert!(result.is_ok());
}

// ── Test: Issue Tracker Cache + Link ─────────────────────────────────────────

#[test]
fn issue_tracker_cache_and_link_to_run() {
    let (_dir, conn) = setup_db();
    let run_id = "run_issue_e2e";
    insert_run(&conn, run_id, None);

    // Cache an issue
    let issue = tracker::Issue {
        external_id: "GH-42".to_string(),
        provider: "github".to_string(),
        title: "Fix login bug".to_string(),
        status: "open".to_string(),
        labels: vec!["bug".to_string(), "priority:high".to_string()],
        body: Some("Users can't log in with SSO".to_string()),
        url: Some("https://github.com/org/repo/issues/42".to_string()),
        assignee: None,
        raw_json: json!({"number": 42, "url": "https://github.com/org/repo/issues/42"}),
        provider_native_id: Some("issue-node-42".to_string()),
        provider_scope_type: Some("repository".to_string()),
        provider_scope_key: Some("org/repo".to_string()),
        provider_scope_name: Some("org/repo".to_string()),
        provider_metadata: json!({"label_names": ["bug", "priority:high"]}),
        id: None,
        project_id: None,
        canonical_status: None,
        priority: None,
        is_native: false,
        created_at: None,
        updated_at: None,
        synced_at: None,
        run_id: None,
    };
    let project_id = "test-project-1";
    tracker::cache_issue(&conn, &issue, project_id).unwrap();

    // Retrieve from cache
    let cached = tracker::get_cached(&conn, "GH-42", project_id).unwrap();
    assert!(cached.is_some());
    let cached = cached.unwrap();
    assert_eq!(cached.title, "Fix login bug");
    assert_eq!(cached.status, "open");
    assert_eq!(cached.labels, vec!["bug", "priority:high"]);

    // Link run to issue
    tracker::link_run_to_issue(&conn, run_id, "GH-42").unwrap();

    // Verify link in DB (issues_cache was superseded by the `issues` table in migration 0023)
    let linked_run: Option<String> = conn
        .query_row(
            "SELECT run_id FROM issues WHERE external_id = 'GH-42'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(linked_run.as_deref(), Some(run_id));

    // List cached issues (scoped to project)
    let all = tracker::list_cached(&conn, project_id).unwrap();
    assert!(!all.is_empty());
    assert!(all.iter().any(|i| i.external_id == "GH-42"));

    // Different project should see nothing
    let other = tracker::list_cached(&conn, "other-project").unwrap();
    assert!(other.is_empty());
}

// ── Test: Full Workflow with All F13 Features ────────────────────────────────

#[test]
fn full_workflow_with_all_f13_features() {
    let (dir, mut conn) = setup_db();

    // ── Step 1: Conversation ──
    let conv_id = conversation::resolve_conversation(
        &mut conn,
        dir.path(),
        None,
        false,
        Some("grove"),
        None,
        conversation::RUN_CONVERSATION_KIND,
    )
    .unwrap();

    // ── Step 2: Run ──
    let run_id = "run_full_f13";
    insert_run(&conn, run_id, Some(&conv_id));

    // ── Step 3: User message ──
    conversation::record_user_message(&mut conn, &conv_id, run_id, "implement auth system")
        .unwrap();

    // ── Step 4: Issue linked to run ──
    let issue = tracker::Issue {
        external_id: "AUTH-100".to_string(),
        provider: "github".to_string(),
        title: "Implement authentication".to_string(),
        status: "in_progress".to_string(),
        labels: vec!["feature".to_string()],
        body: None,
        url: None,
        assignee: None,
        raw_json: json!({}),
        provider_native_id: Some("issue-node-auth-100".to_string()),
        provider_scope_type: Some("repository".to_string()),
        provider_scope_key: Some("org/repo".to_string()),
        provider_scope_name: Some("org/repo".to_string()),
        provider_metadata: json!({"label_names": ["feature"]}),
        id: None,
        project_id: None,
        canonical_status: None,
        priority: None,
        is_native: false,
        created_at: None,
        updated_at: None,
        synced_at: None,
        run_id: None,
    };
    let f13_project_id = conversation::derive_project_id(dir.path());
    tracker::cache_issue(&conn, &issue, &f13_project_id).unwrap();
    tracker::link_run_to_issue(&conn, run_id, "AUTH-100").unwrap();

    // ── Step 5: Architect session + heartbeat ──
    insert_session(&conn, "sess_arch_full", run_id, "architect");
    watchdog::touch_heartbeat(&conn, "sess_arch_full").unwrap();

    // Architect produces output → record message
    conversation::record_agent_message(
        &mut conn,
        &conv_id,
        run_id,
        "architect",
        "sess_arch_full",
        "Auth design: JWT tokens, bcrypt password hashing, middleware guard",
    )
    .unwrap();

    // Architect sends DesignReady signal to builder
    signals::send_signal(
        &conn,
        run_id,
        "architect",
        "builder",
        SignalType::DesignReady,
        SignalPriority::Normal,
        json!({"approach": "JWT + bcrypt"}),
    )
    .unwrap();

    // ── Step 6: Builder session + heartbeat ──
    insert_session(&conn, "sess_build_full", run_id, "builder");
    watchdog::touch_heartbeat(&conn, "sess_build_full").unwrap();

    // Builder checks signals
    let builder_signals = signals::check_signals(&conn, run_id, "builder").unwrap();
    assert_eq!(
        builder_signals.len(),
        1,
        "builder should see architect's DesignReady"
    );
    signals::mark_read(&conn, &builder_signals[0].id).unwrap();

    // Builder produces output → record message
    conversation::record_agent_message(
        &mut conn,
        &conv_id,
        run_id,
        "builder",
        "sess_build_full",
        "Implemented JWT auth middleware + user registration endpoint",
    )
    .unwrap();

    // Builder signals WorkerDone to @leads
    signals::broadcast(
        &conn,
        run_id,
        "builder",
        signals::GROUP_LEADS,
        SignalType::WorkerDone,
        SignalPriority::Normal,
        json!({"files_changed": 8}),
    )
    .unwrap();

    // ── Step 7: Tester session + heartbeat ──
    insert_session(&conn, "sess_test_full", run_id, "tester");
    watchdog::touch_heartbeat(&conn, "sess_test_full").unwrap();

    conversation::record_agent_message(
        &mut conn,
        &conv_id,
        run_id,
        "tester",
        "sess_test_full",
        "All 12 auth tests passing",
    )
    .unwrap();

    // Tester sends TestResult to architect
    signals::send_signal(
        &conn,
        run_id,
        "tester",
        "architect",
        SignalType::TestResult,
        SignalPriority::Normal,
        json!({"passed": 12, "failed": 0}),
    )
    .unwrap();

    // ── Step 8: Watchdog poll — all healthy ──
    let wd_cfg = WatchdogConfig::default();
    let actions =
        watchdog::poll_sessions(&conn, run_id, &wd_cfg, &Utc::now().to_rfc3339()).unwrap();
    assert_eq!(
        actions,
        vec![watchdog::WatchdogAction::Healthy],
        "all sessions have recent heartbeats, should be healthy"
    );

    // ── Step 9: Guards check ──
    let mut guards = HashMap::new();
    guards.insert(
        "tester".to_string(),
        CapabilityGuard {
            allowed_paths: vec!["tests/**".to_string()],
            blocked_paths: vec![],
            blocked_tools: vec!["Write".to_string()],
        },
    );
    assert!(hooks::check_file_guard(
        &guards,
        "tester",
        "tests/auth_test.rs"
    ));
    assert!(!hooks::check_file_guard(&guards, "tester", "src/main.rs"));
    assert!(!hooks::check_tool_guard(&guards, "tester", "Write"));

    // ── Verify: Complete conversation chain ──
    let all_msgs = messages_repo::list_for_conversation(&conn, &conv_id, 100).unwrap();
    assert_eq!(all_msgs.len(), 4, "1 user + 3 agent messages");
    assert_eq!(all_msgs[0].role, "user");
    assert_eq!(all_msgs[0].content, "implement auth system");
    assert_eq!(all_msgs[1].agent_type, Some("architect".to_string()));
    assert_eq!(all_msgs[2].agent_type, Some("builder".to_string()));
    assert_eq!(all_msgs[3].agent_type, Some("tester".to_string()));

    // ── Verify: All signals emitted ──
    let all_signals = signals::list_for_run(&conn, run_id).unwrap();
    assert!(
        all_signals.len() >= 3,
        "at least 3 signals: DesignReady + broadcast + TestResult"
    );

    // ── Verify: Issue linked ──
    let cached = tracker::get_cached(&conn, "AUTH-100", &f13_project_id)
        .unwrap()
        .unwrap();
    assert_eq!(cached.title, "Implement authentication");

    // ── Verify: Events were emitted ──
    let evts = events::list_for_run(&conn, run_id).unwrap();
    let evt_types: Vec<&str> = evts.iter().map(|e| e.event_type.as_str()).collect();
    assert!(
        evt_types.contains(&"signal_sent"),
        "signal_sent events should be in the event log"
    );
}
