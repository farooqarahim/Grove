use grove_core::worktree::manager;
use tempfile::TempDir;

#[test]
fn create_falls_back_to_plain_dir_outside_git_repo() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = project_dir.path().join("worktrees");

    let handle = manager::create(project_dir.path(), &base_dir, "sess-abc123").unwrap();

    assert!(handle.path.exists(), "worktree path should exist on disk");
    assert!(handle.path.is_dir(), "worktree path should be a directory");
    assert!(
        !handle.is_git_worktree,
        "should be a plain dir outside a git repo"
    );
}

#[test]
fn remove_cleans_up_plain_dir() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = project_dir.path().join("worktrees");

    let handle = manager::create(project_dir.path(), &base_dir, "sess-remove").unwrap();
    let path = handle.path.clone();
    assert!(path.exists());

    manager::remove(project_dir.path(), &handle).unwrap();
    assert!(
        !path.exists(),
        "worktree directory should have been removed"
    );
}

#[test]
fn multiple_sessions_get_distinct_paths() {
    let project_dir = TempDir::new().unwrap();
    let base_dir = project_dir.path().join("worktrees");

    let h1 = manager::create(project_dir.path(), &base_dir, "sess-111").unwrap();
    let h2 = manager::create(project_dir.path(), &base_dir, "sess-222").unwrap();

    assert_ne!(
        h1.path, h2.path,
        "distinct sessions must have distinct paths"
    );
    assert!(h1.path.exists());
    assert!(h2.path.exists());
}
