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
    git(&["config", "user.email", "test@grove.local"]);
    git(&["config", "user.name", "Grove Test"]);
    fs::write(path.join("README.md"), "# Test Repo\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "initial commit"]);

    dir
}

#[test]
fn git_worktree_add_from_creates_branch_at_start_point() {
    let repo = init_git_repo();
    let wt_path = repo.path().join("worktrees").join("sess-test1");

    git_ops::git_worktree_add_from(repo.path(), &wt_path, "grove/sess-test1", "main").unwrap();

    assert!(wt_path.exists(), "worktree directory should exist");
    assert!(
        wt_path.join("README.md").exists(),
        "worktree should contain files from start point"
    );

    // Verify branch was created.
    assert!(
        git_ops::git_branch_exists(repo.path(), "grove/sess-test1").unwrap(),
        "branch grove/sess-test1 should exist"
    );
}

#[test]
fn sequential_agents_branch_from_previous_tip() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    // Agent 1: create worktree from main, add a file.
    let h1 = manager::create(repo.path(), &base_dir, "agent1").unwrap();
    fs::write(h1.path.join("agent1.txt"), "from agent1\n").unwrap();
    git_ops::git_add_all(&h1.path).unwrap();
    git_ops::git_commit(&h1.path, "grove: agent1 work").unwrap();

    // Agent 2: branch from agent1's tip.
    let h2 = manager::create_from(repo.path(), &base_dir, "agent2", &h1.branch).unwrap();
    assert!(
        h2.path.join("agent1.txt").exists(),
        "agent2 should see agent1's committed file"
    );

    let content = fs::read_to_string(h2.path.join("agent1.txt")).unwrap();
    assert_eq!(content, "from agent1\n");
}

#[test]
fn parallel_merge_success_with_non_overlapping_changes() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    // Fork two agents from main.
    let h1 = manager::create(repo.path(), &base_dir, "par1").unwrap();
    let h2 = manager::create(repo.path(), &base_dir, "par2").unwrap();

    // Remove the meta file from both worktrees so it doesn't conflict on merge.
    // In production, the meta file is not committed (agents use `commit_agent_work`
    // which only stages tracked changes).
    let _ = fs::remove_file(h1.path.join(manager::WORKTREE_META_FILENAME));
    let _ = fs::remove_file(h2.path.join(manager::WORKTREE_META_FILENAME));

    // Agent 1 creates file_a.txt.
    fs::write(h1.path.join("file_a.txt"), "agent 1 output\n").unwrap();
    git_ops::git_add_all(&h1.path).unwrap();
    git_ops::git_commit(&h1.path, "grove: par1").unwrap();

    // Agent 2 creates file_b.txt.
    fs::write(h2.path.join("file_b.txt"), "agent 2 output\n").unwrap();
    git_ops::git_add_all(&h2.path).unwrap();
    git_ops::git_commit(&h2.path, "grove: par2").unwrap();

    // Create merge worktree from main, merge both forks.
    let merge_path = base_dir.join("merge_result");
    git_ops::git_worktree_add_from(repo.path(), &merge_path, "grove/merge_result", "main").unwrap();

    let outcome1 = execute(
        repo.path(),
        &h1.branch,
        "grove/merge_result",
        "par1",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(matches!(outcome1, MergeOutcome::Success { .. }));

    Command::new("git")
        .args(["reset", "--hard", "grove/merge_result"])
        .current_dir(&merge_path)
        .output()
        .unwrap();

    let outcome2 = execute(
        repo.path(),
        &h2.branch,
        "grove/merge_result",
        "par2",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(
        matches!(outcome2, MergeOutcome::Success { .. }),
        "expected Success but got: {outcome2:?}"
    );

    Command::new("git")
        .args(["reset", "--hard", "grove/merge_result"])
        .current_dir(&merge_path)
        .output()
        .unwrap();

    // Verify both files are present.
    assert!(merge_path.join("file_a.txt").exists());
    assert!(merge_path.join("file_b.txt").exists());
    assert!(merge_path.join("README.md").exists());
}

#[test]
fn parallel_merge_detects_conflict_on_same_file() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    let h1 = manager::create(repo.path(), &base_dir, "conf1").unwrap();
    let h2 = manager::create(repo.path(), &base_dir, "conf2").unwrap();

    // Both modify README.md (same file, same line).
    fs::write(h1.path.join("README.md"), "# Agent 1 version\n").unwrap();
    git_ops::git_add_all(&h1.path).unwrap();
    git_ops::git_commit(&h1.path, "grove: conf1").unwrap();

    fs::write(h2.path.join("README.md"), "# Agent 2 version\n").unwrap();
    git_ops::git_add_all(&h2.path).unwrap();
    git_ops::git_commit(&h2.path, "grove: conf2").unwrap();

    // Merge first fork succeeds.
    let merge_path = base_dir.join("merge_conflict");
    git_ops::git_worktree_add_from(repo.path(), &merge_path, "grove/merge_conflict", "main")
        .unwrap();

    let outcome1 = execute(
        repo.path(),
        &h1.branch,
        "grove/merge_conflict",
        "conf1",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(matches!(outcome1, MergeOutcome::Success { .. }));

    Command::new("git")
        .args(["reset", "--hard", "grove/merge_conflict"])
        .current_dir(&merge_path)
        .output()
        .unwrap();

    // Second fork conflicts.
    let outcome2 = execute(
        repo.path(),
        &h2.branch,
        "grove/merge_conflict",
        "conf2",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(
        matches!(outcome2, MergeOutcome::Conflict { .. }),
        "second merge with conflicting changes should fail"
    );

    // Working tree must be clean after the abort (no half-merged state).
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
fn cleanup_orphaned_branches_removes_stale_branches() {
    let repo = init_git_repo();
    let base_dir = repo.path().join(".grove").join("worktrees");

    // Create a worktree → creates the branch.
    let h = manager::create(repo.path(), &base_dir, "orphan-test").unwrap();
    let branch = h.branch.clone();

    // Remove the worktree directory manually (simulating stale state).
    fs::remove_dir_all(&h.path).unwrap();
    // Prune the git worktree records so git doesn't complain.
    Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo.path())
        .output()
        .unwrap();

    // Branch should still exist.
    assert!(
        git_ops::git_branch_exists(repo.path(), &branch).unwrap(),
        "branch should exist before cleanup"
    );

    // Run orphan cleanup.
    grove_core::worktree::cleanup::cleanup_orphaned_branches(repo.path());

    // Branch should be gone.
    assert!(
        !git_ops::git_branch_exists(repo.path(), &branch).unwrap(),
        "orphaned branch should be removed after cleanup"
    );
}

#[test]
fn create_from_nonexistent_start_point_returns_error() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    let result = manager::create_from(repo.path(), &base_dir, "sess-bad", "nonexistent-branch");

    assert!(
        result.is_err(),
        "creating a worktree from a nonexistent branch should fail"
    );
}

#[test]
fn promotion_merges_agent_branch_into_main() {
    let repo = init_git_repo();
    let base_dir = repo.path().join("worktrees");

    // Agent creates a worktree, writes a file, commits.
    let h = manager::create(repo.path(), &base_dir, "promo-agent").unwrap();
    fs::write(h.path.join("new_feature.txt"), "feature code\n").unwrap();
    git_ops::git_add_all(&h.path).unwrap();
    git_ops::git_commit(&h.path, "grove: promo-agent work").unwrap();

    // Promote: merge agent branch into main (simulates promote_via_git_merge).
    let outcome = execute(
        repo.path(),
        &h.branch,
        "main",
        "promo-agent",
        "test-run",
        &Default::default(),
    )
    .unwrap();
    assert!(
        matches!(outcome, MergeOutcome::Success { .. }),
        "promotion merge into main should succeed"
    );

    // Verify the file exists on the main branch working tree.
    assert!(
        !repo.path().join("new_feature.txt").exists(),
        "safe merge should not rewrite the checked-out project_root worktree"
    );

    let show = Command::new("git")
        .args(["show", "main:new_feature.txt"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    assert!(
        show.status.success(),
        "target branch should contain the promoted file"
    );

    // Verify git log shows the merge commit.
    let log = Command::new("git")
        .args(["log", "--oneline", "-3"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let log_output = String::from_utf8_lossy(&log.stdout);
    assert!(
        log_output.contains("promo-agent"),
        "git log should contain the promotion merge commit; got: {log_output}"
    );
}
