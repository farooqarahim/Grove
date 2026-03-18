use grove_core::merge::executor::{MergeOutcome, execute};
use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Creates a git repository with two conflicting branches:
/// - `main` has "main branch change" in file.txt
/// - `feature` has "feature branch change" in file.txt (diverged from same base)
fn setup_conflict_repo() -> TempDir {
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

    // Base commit shared by both branches.
    fs::write(path.join("file.txt"), "shared base content\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "base: initial commit"]);

    // feature branch — diverging change.
    git(&["checkout", "-b", "feature"]);
    fs::write(path.join("file.txt"), "feature branch change\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "feat: change in feature"]);

    // Back to main — conflicting change on the same line.
    git(&["checkout", "main"]);
    fs::write(path.join("file.txt"), "main branch change\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "main: conflicting change"]);

    dir
}

#[test]
fn merge_executor_detects_conflict() {
    let repo = setup_conflict_repo();

    let outcome = execute(
        repo.path(),
        "feature",
        "main",
        "sess-merge-test",
        "test-run",
        &Default::default(),
    )
    .unwrap();

    assert!(
        matches!(outcome, MergeOutcome::Conflict { .. }),
        "expected Conflict outcome for branches with conflicting changes"
    );
}

#[test]
fn default_branch_is_unchanged_after_conflict() {
    let repo = setup_conflict_repo();

    execute(
        repo.path(),
        "feature",
        "main",
        "sess-merge-test",
        "test-run",
        &Default::default(),
    )
    .unwrap();

    // file.txt must still contain the main branch version.
    let content = fs::read_to_string(repo.path().join("file.txt")).unwrap();
    assert_eq!(
        content.trim(),
        "main branch change",
        "default branch file must be unchanged after a conflicting merge"
    );
}

#[test]
fn working_tree_is_clean_after_abort() {
    let repo = setup_conflict_repo();

    execute(
        repo.path(),
        "feature",
        "main",
        "sess-merge-test",
        "test-run",
        &Default::default(),
    )
    .unwrap();

    // `git status --porcelain` must output nothing (clean tree).
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo.path())
        .output()
        .unwrap();
    let output = String::from_utf8_lossy(&status.stdout);
    assert!(
        output.trim().is_empty(),
        "working tree must be clean after abort; got: {output}"
    );
}
