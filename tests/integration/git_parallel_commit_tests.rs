/// 3.13 — Parallel git commits across linked worktrees do not corrupt the
///         shared git object store.
/// 3.14 — No `.git/index.lock` errors when 20 agents commit concurrently.
///
/// Each git worktree has its own index file in
/// `.git/worktrees/<name>/index`, so `git add` in different worktrees never
/// races on the same lock.  The object store (`.git/objects/`) is shared but
/// protected by git's own pack-file and loose-object write primitives.
///
/// These tests confirm that Grove's worktree-per-agent model is safe at the
/// git layer: 20 concurrent add+commit cycles must all succeed with zero
/// corruption and zero index.lock errors.
use std::fs;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use tempfile::TempDir;

const AGENT_COUNT: usize = 20;

/// Initialise a bare-enough git repo suitable for linked-worktree tests.
fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let p = dir.path();

    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(p)
            .env("GIT_AUTHOR_NAME", "Grove Test")
            .env("GIT_AUTHOR_EMAIL", "test@grove.local")
            .env("GIT_COMMITTER_NAME", "Grove Test")
            .env("GIT_COMMITTER_EMAIL", "test@grove.local")
            .output()
            .expect("git")
    };

    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "test@grove.local"]);
    git(&["config", "user.name", "Grove Test"]);
    fs::write(p.join("README.md"), "# test\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "root"]);

    dir
}

/// Skip if git is not available in PATH.
fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// Run `git <args>` in `dir` with standard test identity env vars.
fn git_in(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "Grove Test")
        .env("GIT_AUTHOR_EMAIL", "test@grove.local")
        .env("GIT_COMMITTER_NAME", "Grove Test")
        .env("GIT_COMMITTER_EMAIL", "test@grove.local")
        .output()
        .expect("git")
}

/// 3.13 — All 20 parallel commits must be reachable in the shared object store.
#[test]
fn parallel_commits_across_worktrees_do_not_corrupt_object_store() {
    if !git_available() {
        return;
    }

    let repo = init_repo();
    let repo_path = repo.path();

    // Create 20 linked worktrees, each on its own branch.
    let mut wt_dirs: Vec<TempDir> = Vec::with_capacity(AGENT_COUNT);
    for i in 0..AGENT_COUNT {
        let wt = TempDir::new().unwrap();
        let branch = format!("agent-{i}");
        let out = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch,
                wt.path().to_str().unwrap(),
            ])
            .current_dir(repo_path)
            .output()
            .expect("git worktree add");
        assert!(
            out.status.success(),
            "worktree add for agent-{i} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        wt_dirs.push(wt);
    }

    // Spawn one thread per worktree; each writes a unique file then commits.
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::with_capacity(AGENT_COUNT);

    for (i, wt) in wt_dirs.iter().enumerate() {
        let wt_path = wt.path().to_path_buf();
        let errs = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            let filename = format!("agent_{i}.txt");
            fs::write(wt_path.join(&filename), format!("output {i}\n")).unwrap();

            let add = git_in(&wt_path, &["add", "."]);
            if !add.status.success() {
                errs.lock().unwrap().push(format!(
                    "agent {i}: git add failed: {}",
                    String::from_utf8_lossy(&add.stderr)
                ));
                return;
            }

            let commit_msg = format!("agent {i} commit");
            let commit = git_in(&wt_path, &["commit", "-m", &commit_msg]);
            if !commit.status.success() {
                errs.lock().unwrap().push(format!(
                    "agent {i}: git commit failed: {}",
                    String::from_utf8_lossy(&commit.stderr)
                ));
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    {
        let errs = errors.lock().unwrap();
        assert!(
            errs.is_empty(),
            "parallel commit failures:\n{}",
            errs.join("\n")
        );
    }

    // 3.13: every branch must have the expected commit in the object store.
    for i in 0..AGENT_COUNT {
        let branch = format!("agent-{i}");
        let out = git_in(repo_path, &["log", "--oneline", "-1", &branch]);
        let log = String::from_utf8_lossy(&out.stdout);
        assert!(
            log.contains(&format!("agent {i} commit")),
            "branch {branch} missing expected commit; git log returned: {log}"
        );
    }

    // Confirm the object store is internally consistent.
    let fsck = git_in(repo_path, &["fsck", "--no-progress"]);
    assert!(
        fsck.status.success(),
        "git fsck reported corruption after parallel commits:\n{}",
        String::from_utf8_lossy(&fsck.stderr)
    );
}

/// 3.14 — No `.git/index.lock` errors when agents commit in parallel.
///
/// Each git worktree maintains its own index under `.git/worktrees/<name>/index`,
/// so concurrent `git add` operations in separate worktrees must never produce
/// an "index.lock" error.
#[test]
fn no_index_lock_errors_under_parallel_add_commit() {
    if !git_available() {
        return;
    }

    let repo = init_repo();
    let repo_path = repo.path();

    let mut wt_dirs: Vec<TempDir> = Vec::with_capacity(AGENT_COUNT);
    for i in 0..AGENT_COUNT {
        let wt = TempDir::new().unwrap();
        let branch = format!("lock-{i}");
        let out = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch,
                wt.path().to_str().unwrap(),
            ])
            .current_dir(repo_path)
            .output()
            .expect("git worktree add");
        assert!(
            out.status.success(),
            "worktree add for lock-{i} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        wt_dirs.push(wt);
    }

    let lock_errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::with_capacity(AGENT_COUNT);

    for (i, wt) in wt_dirs.iter().enumerate() {
        let wt_path = wt.path().to_path_buf();
        let errs = Arc::clone(&lock_errors);
        handles.push(thread::spawn(move || {
            fs::write(wt_path.join(format!("f{i}.txt")), "x").unwrap();

            let add = git_in(&wt_path, &["add", "."]);
            let commit = git_in(&wt_path, &["commit", "-m", "lock test"]);

            for (op, out) in [("add", &add), ("commit", &commit)] {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("index.lock") {
                    errs.lock().unwrap().push(format!(
                        "agent {i} git {op}: index.lock contention: {stderr}"
                    ));
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("thread panicked");
    }

    let errs = lock_errors.lock().unwrap();
    assert!(
        errs.is_empty(),
        "index.lock errors detected — worktree index isolation is broken:\n{}",
        errs.join("\n")
    );
}
