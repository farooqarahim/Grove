/// F11: Worktree Consolidation integration tests.
///
/// Tests for shared run worktree, checkpoint SHA recording,
/// git clean/reset primitives, run worktree protection guard,
/// and promotion via run branch.
use grove_core::merge::executor::{MergeOutcome, execute};
use grove_core::worktree::git_ops;
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Helper: init a git repo with one commit and return the TempDir.
fn init_git_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let path = dir.path();

    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .unwrap()
    };

    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "test@grove.local"]);
    git(&["config", "user.name", "Grove Test"]);
    fs::write(path.join("README.md"), "# Test Repo\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "initial commit"]);

    dir
}

// ── git_clean_worktree ──────────────────────────────────────────────────────

#[test]
fn git_clean_worktree_restores_modified_tracked_files() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("clean-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/clean-test").unwrap();

    // Modify a tracked file.
    fs::write(wt_path.join("README.md"), "MODIFIED\n").unwrap();
    let content_before = fs::read_to_string(wt_path.join("README.md")).unwrap();
    assert_eq!(content_before, "MODIFIED\n");

    // Clean should restore it.
    git_ops::git_clean_worktree(&wt_path).unwrap();

    let content_after = fs::read_to_string(wt_path.join("README.md")).unwrap();
    assert_eq!(
        content_after, "# Test Repo\n",
        "tracked file should be restored to committed state"
    );
}

#[test]
fn git_clean_worktree_removes_untracked_files() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("untracked-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/untracked-test").unwrap();

    // Create an untracked file.
    fs::write(wt_path.join("build_cache.txt"), "temp data\n").unwrap();
    assert!(wt_path.join("build_cache.txt").exists());

    // Clean should remove it.
    git_ops::git_clean_worktree(&wt_path).unwrap();

    assert!(
        !wt_path.join("build_cache.txt").exists(),
        "untracked file should be removed"
    );
}

#[test]
fn git_clean_worktree_removes_untracked_directories() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("untracked-dir-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/untracked-dir-test").unwrap();

    // Create an untracked directory with files.
    fs::create_dir_all(wt_path.join("target/debug")).unwrap();
    fs::write(wt_path.join("target/debug/output.o"), "object file\n").unwrap();
    assert!(wt_path.join("target").exists());

    // Clean should remove the entire directory.
    git_ops::git_clean_worktree(&wt_path).unwrap();

    assert!(
        !wt_path.join("target").exists(),
        "untracked directory should be removed"
    );
}

#[test]
fn git_clean_worktree_preserves_committed_files() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("preserve-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/preserve-test").unwrap();

    // Add and commit a new file.
    fs::write(wt_path.join("new_code.rs"), "fn main() {}\n").unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "add new_code.rs").unwrap();

    // Add an untracked file too.
    fs::write(wt_path.join("temp.log"), "log\n").unwrap();

    // Clean should keep committed files but remove untracked.
    git_ops::git_clean_worktree(&wt_path).unwrap();

    assert!(
        wt_path.join("new_code.rs").exists(),
        "committed file should survive clean"
    );
    assert!(
        !wt_path.join("temp.log").exists(),
        "untracked file should be removed"
    );
}

// ── git_reset_hard ──────────────────────────────────────────────────────────

#[test]
fn git_reset_hard_rolls_back_to_checkpoint() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("reset-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/reset-test").unwrap();

    // Record checkpoint SHA.
    let checkpoint = git_ops::git_rev_parse_head(&wt_path).unwrap();

    // Make a commit.
    fs::write(wt_path.join("new_file.txt"), "new content\n").unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "add new_file").unwrap();
    assert!(wt_path.join("new_file.txt").exists());

    // Reset to checkpoint — new file should be gone.
    git_ops::git_reset_hard(&wt_path, &checkpoint).unwrap();

    assert!(
        !wt_path.join("new_file.txt").exists(),
        "file added after checkpoint should be gone"
    );
    assert!(
        wt_path.join("README.md").exists(),
        "original file should still exist"
    );
}

#[test]
fn git_reset_hard_restores_deleted_files() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("reset-delete-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/reset-delete-test").unwrap();

    let checkpoint = git_ops::git_rev_parse_head(&wt_path).unwrap();

    // Delete README and commit.
    fs::remove_file(wt_path.join("README.md")).unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "delete README").unwrap();
    assert!(!wt_path.join("README.md").exists());

    // Reset should restore the deleted file.
    git_ops::git_reset_hard(&wt_path, &checkpoint).unwrap();

    assert!(
        wt_path.join("README.md").exists(),
        "deleted file should be restored after reset"
    );
}

#[test]
fn git_reset_hard_with_invalid_sha_returns_error() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("bad-sha-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/bad-sha-test").unwrap();

    let result = git_ops::git_reset_hard(&wt_path, "deadbeef0000000000000000000000000000dead");
    assert!(result.is_err(), "reset to nonexistent SHA should fail");
}

// ── Run branch creation + promotion ─────────────────────────────────────────

#[test]
fn run_branch_worktree_lifecycle() {
    let repo = init_git_repo();
    let run_id = "test_run_001";
    let run_branch = format!("grove/run-{run_id}");
    let wt_path = repo.path().join("worktrees").join(format!("run_{run_id}"));

    // Create run branch via worktree.
    git_ops::git_worktree_add(repo.path(), &wt_path, &run_branch).unwrap();
    assert!(git_ops::git_branch_exists(repo.path(), &run_branch).unwrap());
    assert!(wt_path.join("README.md").exists());

    // Simulate agent work: add file, commit.
    fs::write(wt_path.join("feature.rs"), "pub fn feature() {}\n").unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "grove: architect work").unwrap();

    let sha = git_ops::git_rev_parse_head(&wt_path).unwrap();
    assert!(!sha.is_empty(), "checkpoint SHA should be non-empty");

    // Clean between agents.
    git_ops::git_clean_worktree(&wt_path).unwrap();
    assert!(
        wt_path.join("feature.rs").exists(),
        "committed file should survive clean"
    );

    // Simulate second agent: modify the committed file.
    fs::write(wt_path.join("feature.rs"), "pub fn feature() { todo!() }\n").unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "grove: builder work").unwrap();

    let sha2 = git_ops::git_rev_parse_head(&wt_path).unwrap();
    assert_ne!(sha, sha2, "second commit should produce a different SHA");

    // Promotion: merge run branch into main.
    let outcome = execute(
        repo.path(),
        &run_branch,
        "main",
        run_id,
        run_id,
        &Default::default(),
    )
    .unwrap();
    assert!(
        matches!(outcome, MergeOutcome::Success { .. }),
        "merging run branch into main should succeed"
    );

    // Safe merge should advance main without rewriting the checked-out project root.
    assert!(
        !repo.path().join("feature.rs").exists(),
        "safe merge should not rewrite the checked-out project root"
    );
    let show = Command::new("git")
        .args(["show", "main:feature.rs"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        show.status.success(),
        "feature file should exist on main after promotion"
    );
    let content = String::from_utf8_lossy(&show.stdout);
    assert_eq!(
        content, "pub fn feature() { todo!() }\n",
        "should have the builder's version"
    );
}

#[test]
fn run_branch_rollback_via_checkpoint() {
    let repo = init_git_repo();
    let run_branch = "grove/run-rollback-test";
    let wt_path = repo.path().join("worktrees").join("run_rollback_test");

    git_ops::git_worktree_add(repo.path(), &wt_path, run_branch).unwrap();

    // Agent 1: add file, commit, record checkpoint.
    fs::write(wt_path.join("good.rs"), "fn good() {}\n").unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "grove: agent1 success").unwrap();
    let checkpoint = git_ops::git_rev_parse_head(&wt_path).unwrap();
    git_ops::git_clean_worktree(&wt_path).unwrap();

    // Agent 2: add a bad file, commit (simulating work before failure detected).
    fs::write(wt_path.join("bad.rs"), "fn bad() { panic!() }\n").unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "grove: agent2 bad work").unwrap();
    assert!(wt_path.join("bad.rs").exists());

    // Rollback to agent 1's checkpoint.
    git_ops::git_reset_hard(&wt_path, &checkpoint).unwrap();

    assert!(
        wt_path.join("good.rs").exists(),
        "good file from agent 1 should survive rollback"
    );
    assert!(
        !wt_path.join("bad.rs").exists(),
        "bad file from agent 2 should be gone after rollback"
    );
}

// ── Run worktree protection guard ───────────────────────────────────────────

#[test]
fn run_worktree_name_detection() {
    // Test the naming convention used by cleanup_after_merge to protect run worktrees.
    let test_cases = vec![
        ("run_abc123", true),
        ("run_test_run_001", true),
        ("sess_abc123", false),
        ("merge_result", false),
        ("architect_work", false),
    ];

    for (name, expected) in test_cases {
        let is_run = name.starts_with("run_");
        assert_eq!(
            is_run, expected,
            "starts_with(\"run_\") for '{name}': expected {expected}, got {is_run}"
        );
    }
}

// ── Checkpoint SHA recording ────────────────────────────────────────────────

#[test]
fn checkpoint_sha_column_exists_after_migration() {
    let dir = TempDir::new().unwrap();
    grove_core::db::initialize(dir.path()).unwrap();
    let conn = grove_core::db::DbHandle::new(dir.path()).connect().unwrap();

    // Verify the checkpoint_sha column exists on sessions.
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(sessions)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert!(
        cols.contains(&"checkpoint_sha".to_string()),
        "sessions table should have checkpoint_sha column; found: {cols:?}"
    );
}

#[test]
fn checkpoint_sha_can_be_written_and_read() {
    let dir = TempDir::new().unwrap();
    grove_core::db::initialize(dir.path()).unwrap();
    let conn = grove_core::db::DbHandle::new(dir.path()).connect().unwrap();

    // Insert a run and session to test checkpoint_sha storage.
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
         VALUES ('run1', 'test', 'executing', 1.0, 0.0, ?1, ?1)",
        [&now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions (id, run_id, agent_type, state, worktree_path, created_at, updated_at)
         VALUES ('sess1', 'run1', 'builder', 'completed', '/tmp/test', ?1, ?1)",
        [&now],
    ).unwrap();

    // Write a checkpoint SHA.
    let sha = "abc123def456";
    conn.execute(
        "UPDATE sessions SET checkpoint_sha = ?1 WHERE id = ?2",
        rusqlite::params![sha, "sess1"],
    )
    .unwrap();

    // Read it back.
    let stored: Option<String> = conn
        .query_row(
            "SELECT checkpoint_sha FROM sessions WHERE id = 'sess1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, Some(sha.to_string()));
}

#[test]
fn checkpoint_sha_defaults_to_null() {
    let dir = TempDir::new().unwrap();
    grove_core::db::initialize(dir.path()).unwrap();
    let conn = grove_core::db::DbHandle::new(dir.path()).connect().unwrap();

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
         VALUES ('run1', 'test', 'executing', 1.0, 0.0, ?1, ?1)",
        [&now],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sessions (id, run_id, agent_type, state, worktree_path, created_at, updated_at)
         VALUES ('sess1', 'run1', 'builder', 'queued', '/tmp/test', ?1, ?1)",
        [&now],
    ).unwrap();

    let stored: Option<String> = conn
        .query_row(
            "SELECT checkpoint_sha FROM sessions WHERE id = 'sess1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, None, "checkpoint_sha should default to NULL");
}

// ── git_branch_exists ───────────────────────────────────────────────────────

#[test]
fn git_branch_exists_returns_false_for_nonexistent() {
    let repo = init_git_repo();
    let exists = git_ops::git_branch_exists(repo.path(), "grove/nonexistent").unwrap();
    assert!(!exists);
}

#[test]
fn git_branch_exists_returns_true_after_worktree_add() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("exists-test");
    git_ops::git_worktree_add(repo.path(), &wt_path, "grove/exists-test").unwrap();

    let exists = git_ops::git_branch_exists(repo.path(), "grove/exists-test").unwrap();
    assert!(exists);
}

// ── Sequential handoff simulation ───────────────────────────────────────────

#[test]
fn sequential_agents_share_worktree_with_clean_handoff() {
    let repo = init_git_repo();
    let run_branch = "grove/run-handoff-test";
    let wt_path = repo.path().join("worktrees").join("run_handoff_test");

    git_ops::git_worktree_add(repo.path(), &wt_path, run_branch).unwrap();

    // Architect: creates a plan file + leaves temp files.
    fs::write(wt_path.join("PLAN.md"), "# Architecture Plan\n").unwrap();
    fs::write(wt_path.join(".architect_cache"), "cache\n").unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "grove: architect").unwrap();
    let arch_sha = git_ops::git_rev_parse_head(&wt_path).unwrap();

    // Clean between architect and builder.
    fs::write(wt_path.join("leftover.tmp"), "garbage\n").unwrap();
    git_ops::git_clean_worktree(&wt_path).unwrap();

    // Verify: committed files present, untracked removed.
    assert!(
        wt_path.join("PLAN.md").exists(),
        "architect's committed plan should persist"
    );
    assert!(
        !wt_path.join("leftover.tmp").exists(),
        "untracked temp should be removed"
    );

    // Builder: sees architect's work, adds implementation.
    assert!(
        wt_path.join("PLAN.md").exists(),
        "builder should see architect's plan"
    );
    fs::create_dir_all(wt_path.join("src")).unwrap();
    fs::write(
        wt_path.join("src/main.rs"),
        "fn main() { println!(\"hello\"); }\n",
    )
    .unwrap();
    git_ops::git_add_all(&wt_path).unwrap();
    git_ops::git_commit(&wt_path, "grove: builder").unwrap();
    let builder_sha = git_ops::git_rev_parse_head(&wt_path).unwrap();

    assert_ne!(
        arch_sha, builder_sha,
        "builder commit should produce a new SHA"
    );

    // Clean between builder and tester.
    git_ops::git_clean_worktree(&wt_path).unwrap();

    // Tester: sees both architect's and builder's work.
    assert!(wt_path.join("PLAN.md").exists());
    assert!(wt_path.join("src/main.rs").exists());

    // Verify we can rollback to architect's checkpoint.
    git_ops::git_reset_hard(&wt_path, &arch_sha).unwrap();
    assert!(
        wt_path.join("PLAN.md").exists(),
        "plan should survive rollback"
    );
    assert!(
        !wt_path.join("src").exists(),
        "builder's src dir should be gone after rollback"
    );
}
