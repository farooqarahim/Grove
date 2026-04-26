//! Tests for the `run_task_wave` git merge path in engine.rs.
//!
//! These exercise the single-task and multi-task parallel paths using real
//! git repos, verifying fork creation, commit, merge, and conflict behavior.

use grove_core::merge::executor::{MergeOutcome, execute};
use grove_core::worktree::git_ops;
use grove_core::worktree::manager;
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
    git(&["config", "core.autocrlf", "false"]);
    git(&["config", "user.email", "test@grove.local"]);
    git(&["config", "user.name", "Grove Test"]);
    fs::write(path.join("README.md"), "# Task Wave Test\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "initial commit"]);

    dir
}

// ── Single-task path ──────────────────────────────────────────────────────────

#[test]
fn single_task_fork_commits_to_branch() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    // Simulate single-task: create worktree via git, write file, commit.
    let h = manager::create(repo.path(), &base_dir, "wave-single").unwrap();
    assert!(h.is_git_worktree, "should create a git worktree");

    fs::write(h.path.join("task_output.txt"), "single task result\n").unwrap();
    git_ops::git_add_all(&h.path).unwrap();
    git_ops::git_commit(&h.path, "grove: builder subtask wave-single").unwrap();

    // Verify branch has the commit.
    assert!(
        git_ops::git_branch_exists(repo.path(), &h.branch).unwrap(),
        "branch should exist after commit"
    );

    // Verify file exists on the branch by checking the worktree.
    assert!(h.path.join("task_output.txt").exists());
    let content = fs::read_to_string(h.path.join("task_output.txt")).unwrap();
    assert_eq!(content, "single task result\n");
}

#[test]
fn single_task_branches_from_previous_agent() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    // First agent creates a file.
    let h1 = manager::create(repo.path(), &base_dir, "prev-agent").unwrap();
    fs::write(h1.path.join("prev_work.txt"), "previous agent output\n").unwrap();
    git_ops::git_add_all(&h1.path).unwrap();
    git_ops::git_commit(&h1.path, "grove: prev-agent").unwrap();

    // Single-task agent branches from previous agent's tip.
    let h2 = manager::create_from(repo.path(), &base_dir, "wave-task", &h1.branch).unwrap();

    // The file from the previous agent must be visible.
    assert!(
        h2.path.join("prev_work.txt").exists(),
        "single task should inherit files from previous agent"
    );
    let content = fs::read_to_string(h2.path.join("prev_work.txt")).unwrap();
    assert_eq!(content, "previous agent output\n");
}

// ── Multi-task parallel path ──────────────────────────────────────────────────

#[test]
fn parallel_task_wave_non_overlapping_files_merge_cleanly() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    // Fork two subtask agents from main.
    let t1 = manager::create(repo.path(), &base_dir, "task-a").unwrap();
    let t2 = manager::create(repo.path(), &base_dir, "task-b").unwrap();

    // Remove meta files so they don't conflict on merge.
    let _ = fs::remove_file(t1.path.join(manager::WORKTREE_META_FILENAME));
    let _ = fs::remove_file(t2.path.join(manager::WORKTREE_META_FILENAME));

    // Task A creates one file.
    fs::write(t1.path.join("task_a_output.txt"), "task A done\n").unwrap();
    git_ops::git_add_all(&t1.path).unwrap();
    git_ops::git_commit(&t1.path, "grove: task-a").unwrap();

    // Task B creates a different file.
    fs::write(t2.path.join("task_b_output.txt"), "task B done\n").unwrap();
    git_ops::git_add_all(&t2.path).unwrap();
    git_ops::git_commit(&t2.path, "grove: task-b").unwrap();

    // Create merge worktree and merge both.
    let merge_path = base_dir.join("wave_merge");
    git_ops::git_worktree_add_from(repo.path(), &merge_path, "grove/wave_merge", "main").unwrap();

    let out1 = execute(
        repo.path(),
        &t1.branch,
        "grove/wave_merge",
        "task-a",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(matches!(out1, MergeOutcome::Success { .. }));

    Command::new("git")
        .args(["reset", "--hard", "grove/wave_merge"])
        .current_dir(&merge_path)
        .output()
        .unwrap();

    let out2 = execute(
        repo.path(),
        &t2.branch,
        "grove/wave_merge",
        "task-b",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(matches!(out2, MergeOutcome::Success { .. }));

    Command::new("git")
        .args(["reset", "--hard", "grove/wave_merge"])
        .current_dir(&merge_path)
        .output()
        .unwrap();

    // Both files present in merge result.
    assert!(merge_path.join("task_a_output.txt").exists());
    assert!(merge_path.join("task_b_output.txt").exists());
    assert!(merge_path.join("README.md").exists());
}

#[test]
fn parallel_task_wave_conflict_returns_conflict_outcome() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    let t1 = manager::create(repo.path(), &base_dir, "conflict-a").unwrap();
    let t2 = manager::create(repo.path(), &base_dir, "conflict-b").unwrap();

    // Both modify README.md.
    fs::write(t1.path.join("README.md"), "# Task A version\n").unwrap();
    git_ops::git_add_all(&t1.path).unwrap();
    git_ops::git_commit(&t1.path, "grove: conflict-a").unwrap();

    fs::write(t2.path.join("README.md"), "# Task B version\n").unwrap();
    git_ops::git_add_all(&t2.path).unwrap();
    git_ops::git_commit(&t2.path, "grove: conflict-b").unwrap();

    // Create merge worktree.
    let merge_path = base_dir.join("wave_conflict");
    git_ops::git_worktree_add_from(repo.path(), &merge_path, "grove/wave_conflict", "main")
        .unwrap();

    // First merge succeeds.
    let out1 = execute(
        repo.path(),
        &t1.branch,
        "grove/wave_conflict",
        "conflict-a",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(matches!(out1, MergeOutcome::Success { .. }));

    Command::new("git")
        .args(["reset", "--hard", "grove/wave_conflict"])
        .current_dir(&merge_path)
        .output()
        .unwrap();

    // Second merge conflicts.
    let out2 = execute(
        repo.path(),
        &t2.branch,
        "grove/wave_conflict",
        "conflict-b",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(
        matches!(out2, MergeOutcome::Conflict { .. }),
        "conflicting task wave should produce Conflict"
    );

    // Repo must be clean after abort (no half-merged state).
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&merge_path)
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&status.stdout);
    assert!(
        output.trim().is_empty(),
        "merge worktree must be clean after conflict abort; got: {output}"
    );
}

#[test]
fn parallel_task_wave_conflict_lists_conflicting_files() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    let t1 = manager::create(repo.path(), &base_dir, "filelist-a").unwrap();
    let t2 = manager::create(repo.path(), &base_dir, "filelist-b").unwrap();

    fs::write(t1.path.join("README.md"), "# Version A\n").unwrap();
    git_ops::git_add_all(&t1.path).unwrap();
    git_ops::git_commit(&t1.path, "grove: filelist-a").unwrap();

    fs::write(t2.path.join("README.md"), "# Version B\n").unwrap();
    git_ops::git_add_all(&t2.path).unwrap();
    git_ops::git_commit(&t2.path, "grove: filelist-b").unwrap();

    let merge_path = base_dir.join("wave_filelist");
    git_ops::git_worktree_add_from(repo.path(), &merge_path, "grove/wave_filelist", "main")
        .unwrap();

    let _ = execute(
        repo.path(),
        &t1.branch,
        "grove/wave_filelist",
        "filelist-a",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    Command::new("git")
        .args(["reset", "--hard", "grove/wave_filelist"])
        .current_dir(&merge_path)
        .output()
        .unwrap();
    let out2 = execute(
        repo.path(),
        &t2.branch,
        "grove/wave_filelist",
        "filelist-b",
        "test-run",
        &Default::default(),
    )
    .unwrap();

    match out2 {
        MergeOutcome::Conflict { files } => {
            assert!(
                files.iter().any(|f| f.contains("README.md")),
                "conflicting files should include README.md; got: {files:?}"
            );
        }
        MergeOutcome::Success { .. } => {
            panic!("expected Conflict, got Success");
        }
    }
}
