use std::fs;
use std::process::Command;
use std::sync::Arc;

use grove_core::config::{DEFAULT_CONFIG_YAML, GroveConfig};
use grove_core::db;
use grove_core::db::repositories::conversations_repo::ConversationRow;
use grove_core::db::repositories::projects_repo::ProjectRow;
use grove_core::events;
use grove_core::orchestrator;
use grove_core::orchestrator::{RunOptions, execute_objective};
use grove_core::providers::{MockProvider, Provider};
use tempfile::TempDir;

fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };

    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "test@grove.local"]);
    git(&["config", "user.name", "Grove Test"]);
    fs::write(path.join("README.md"), "base\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "base"]);

    dir
}

fn setup_project(conn: &mut rusqlite::Connection, repo: &TempDir) {
    let workspace_id = orchestrator::workspace::ensure_workspace(conn).unwrap();
    grove_core::db::repositories::projects_repo::insert(
        conn,
        &ProjectRow {
            id: "proj1".to_string(),
            workspace_id,
            name: Some("Test Project".to_string()),
            root_path: repo.path().to_string_lossy().to_string(),
            state: "active".to_string(),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            base_ref: None,
            source_kind: "local".to_string(),
            source_details: None,
        },
    )
    .unwrap();
}

fn insert_conversation(conn: &mut rusqlite::Connection, conversation_id: &str, branch_name: &str) {
    grove_core::db::repositories::conversations_repo::insert(
        conn,
        &ConversationRow {
            id: conversation_id.to_string(),
            project_id: "proj1".to_string(),
            title: Some("Conversation".to_string()),
            state: "active".to_string(),
            conversation_kind: grove_core::orchestrator::conversation::RUN_CONVERSATION_KIND
                .to_string(),
            cli_provider: None,
            cli_model: None,
            branch_name: Some(branch_name.to_string()),
            remote_branch_name: None,
            remote_registration_state: "local_only".to_string(),
            remote_registration_error: None,
            remote_registered_at: None,
            worktree_path: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
            workspace_id: None,
            user_id: None,
        },
    )
    .unwrap();
}

fn insert_run(conn: &rusqlite::Connection, run_id: &str, conversation_id: &str) {
    conn.execute(
        "INSERT INTO runs(id, conversation_id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
         VALUES(?1, ?2, 'test', 'completed', 1.0, 0.0, '2024-01-01T00:00:00Z', '2024-01-01T00:00:00Z')",
        [run_id, conversation_id],
    )
    .unwrap();
}

fn mock_cfg() -> GroveConfig {
    let mut cfg: GroveConfig = serde_yaml::from_str(DEFAULT_CONFIG_YAML).unwrap();
    cfg.providers.default = "mock".to_string();
    cfg.providers.claude_code.enabled = false;
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
    cfg.publish.enabled = false;
    cfg.worktree.fetch_before_run = false;
    cfg.worktree.sync_before_run = grove_core::config::SyncBeforeRun::Rebase;
    cfg
}

fn run_options(conversation_id: Option<String>) -> RunOptions {
    RunOptions {
        budget_usd: None,
        max_agents: None,
        model: None,
        interactive: false,
        pause_after: vec![],
        disable_phase_gates: false,
        permission_mode: None,
        pipeline: None,
        conversation_id,
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
        session_host_registry: None,
    }
}

#[test]
fn rebase_conversation_emits_conv_rebased_event() {
    let repo = init_repo();
    let path = repo.path();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };

    git(&["checkout", "-b", "grove/s_conv-rebase"]);
    fs::write(path.join("feature.txt"), "feature\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "feature"]);
    git(&["checkout", "main"]);
    fs::write(path.join("README.md"), "base updated\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "main update"]);

    db::initialize(path).unwrap();
    let mut conn = db::DbHandle::new(path).connect().unwrap();
    setup_project(&mut conn, &repo);
    insert_conversation(&mut conn, "conv-rebase", "grove/s_conv-rebase");
    insert_run(&conn, "run-conv-rebase", "conv-rebase");
    drop(conn);

    let message = orchestrator::rebase_conversation(path, "conv-rebase").unwrap();
    assert!(message.contains("successfully rebased"));

    assert!(
        grove_core::worktree::git_ops::git_detect_stale_base(path, "grove/s_conv-rebase", "main")
            .is_none(),
        "conversation branch should no longer be stale after rebase"
    );

    let conn = db::DbHandle::new(path).connect().unwrap();
    let event_types: Vec<String> = events::list_for_run(&conn, "run-conv-rebase")
        .unwrap()
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert!(
        event_types
            .iter()
            .any(|t| t == grove_core::events::event_types::CONV_REBASED),
        "expected conv_rebased event, got {event_types:?}"
    );
}

#[test]
fn merge_conversation_direct_emits_conv_merged_and_keeps_project_root_clean() {
    let repo = init_repo();
    let path = repo.path();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };

    git(&["checkout", "-b", "grove/s_conv-merge"]);
    fs::write(path.join("merged.txt"), "from conversation\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "conversation work"]);
    git(&["checkout", "main"]);

    db::initialize(path).unwrap();
    let mut conn = db::DbHandle::new(path).connect().unwrap();
    setup_project(&mut conn, &repo);
    insert_conversation(&mut conn, "conv-merge", "grove/s_conv-merge");
    insert_run(&conn, "run-conv-merge", "conv-merge");
    drop(conn);

    let result = orchestrator::merge_conversation(path, "conv-merge").unwrap();
    assert_eq!(result.outcome, "merged");
    assert_eq!(result.target_branch, "main");
    assert!(
        !path.join("merged.txt").exists(),
        "safe merge should not rewrite the checked-out project_root worktree"
    );

    let show = Command::new("git")
        .args(["show", "main:merged.txt"])
        .current_dir(path)
        .output()
        .unwrap();
    assert!(
        show.status.success(),
        "target branch should contain merged content after safe merge"
    );

    let conn = db::DbHandle::new(path).connect().unwrap();
    let event_types: Vec<String> = events::list_for_run(&conn, "run-conv-merge")
        .unwrap()
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert!(
        event_types
            .iter()
            .any(|t| t == grove_core::events::event_types::CONV_MERGED),
        "expected conv_merged event, got {event_types:?}"
    );
}

#[test]
fn execute_objective_auto_rebases_stale_conversation_branch_when_enabled() {
    let repo = init_repo();
    db::initialize(repo.path()).unwrap();

    let cfg = mock_cfg();
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);

    let first = execute_objective(
        repo.path(),
        &cfg,
        "first run",
        run_options(None),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(repo.path()).connect().unwrap();
    let conversation_id: String = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&first.run_id],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    fs::write(repo.path().join("README.md"), "main advanced\n").unwrap();
    git(&["add", "README.md"]);
    git(&["commit", "-m", "advance main"]);

    let second = execute_objective(
        repo.path(),
        &cfg,
        "second run",
        run_options(Some(conversation_id.clone())),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(repo.path()).connect().unwrap();
    let events_for_run = events::list_for_run(&conn, &second.run_id).unwrap();
    let event_types: Vec<String> = events_for_run
        .iter()
        .map(|e| e.event_type.clone())
        .collect();
    let stale_payload = events_for_run
        .iter()
        .find(|e| e.event_type == "conv_branch_stale")
        .map(|e| e.payload.clone());

    let conv_branch = format!("grove/s_{conversation_id}");
    let stale =
        grove_core::worktree::git_ops::git_detect_stale_base(repo.path(), &conv_branch, "main");
    let main_sha = grove_core::worktree::git_ops::git_rev_parse(repo.path(), "main").unwrap();
    let conv_sha = grove_core::worktree::git_ops::git_rev_parse(repo.path(), &conv_branch).unwrap();
    let ahead = grove_core::worktree::git_ops::git_log_oneline(
        repo.path(),
        &format!("main..{conv_branch}"),
    )
    .unwrap();
    assert!(
        stale.is_none(),
        "conversation branch should be rebased before the second run starts; stale={stale:?}; main_sha={main_sha}; conv_sha={conv_sha}; ahead={ahead:?}; events={event_types:?}; stale_payload={stale_payload:?}"
    );
    assert!(
        event_types
            .iter()
            .any(|t| t == grove_core::events::event_types::CONV_REBASED),
        "expected conv_rebased event on auto-rebase, got {event_types:?}"
    );
}

#[test]
fn execute_objective_leaves_stale_branch_when_auto_rebase_disabled() {
    let repo = init_repo();
    db::initialize(repo.path()).unwrap();

    let mut cfg = mock_cfg();
    cfg.worktree.sync_before_run = grove_core::config::SyncBeforeRun::None;
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);

    let first = execute_objective(
        repo.path(),
        &cfg,
        "first run",
        run_options(None),
        Arc::clone(&provider),
    )
    .unwrap();

    let conn = db::DbHandle::new(repo.path()).connect().unwrap();
    let conversation_id: String = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&first.run_id],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    fs::write(
        repo.path().join("README.md"),
        "main advanced without rebase\n",
    )
    .unwrap();
    git(&["add", "README.md"]);
    git(&["commit", "-m", "advance main"]);

    let second = execute_objective(
        repo.path(),
        &cfg,
        "second run",
        run_options(Some(conversation_id.clone())),
        Arc::clone(&provider),
    )
    .unwrap();

    let conv_branch = format!("grove/s_{conversation_id}");
    assert!(
        grove_core::worktree::git_ops::git_detect_stale_base(repo.path(), &conv_branch, "main")
            .is_some(),
        "conversation branch should remain stale when auto-rebase is disabled"
    );

    let conn = db::DbHandle::new(repo.path()).connect().unwrap();
    let event_types: Vec<String> = events::list_for_run(&conn, &second.run_id)
        .unwrap()
        .into_iter()
        .map(|e| e.event_type)
        .collect();
    assert!(
        event_types.iter().any(|t| t == "conv_branch_stale"),
        "expected stale warning event, got {event_types:?}"
    );
    assert!(
        !event_types
            .iter()
            .any(|t| t == grove_core::events::event_types::CONV_REBASED),
        "auto-rebase disabled should not emit conv_rebased"
    );
}
