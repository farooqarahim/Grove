use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex, MutexGuard};

use chrono::Utc;
use grove_core::config::{TrackerMode, defaults::default_config};
use grove_core::db::{self, DbHandle};
use grove_core::orchestrator;
use grove_core::publish;
use rusqlite::{Connection, params};
use tempfile::TempDir;

static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

struct TestRepo {
    repo: TempDir,
    remote: Option<TempDir>,
    branch: String,
    conversation_id: String,
}

struct GhStub {
    _guard: MutexGuard<'static, ()>,
    _bin_dir: TempDir,
    open_pr_file: PathBuf,
    create_count_file: PathBuf,
    issue_mode_file: PathBuf,
}

struct PathGuard {
    old_path: Option<String>,
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        if let Some(path) = self.old_path.take() {
            unsafe { std::env::set_var("PATH", path) };
        } else {
            unsafe { std::env::remove_var("PATH") };
        }
    }
}

impl TestRepo {
    fn new(with_remote: bool) -> Self {
        let repo = tempfile::tempdir().unwrap();
        git_ok(repo.path(), &["init", "--initial-branch=main"]);
        git_ok(repo.path(), &["config", "user.email", "grove@example.test"]);
        git_ok(repo.path(), &["config", "user.name", "Grove Tests"]);
        fs::write(repo.path().join("README.md"), "seed\n").unwrap();
        fs::write(repo.path().join(".gitignore"), ".grove/\n").unwrap();
        git_ok(repo.path(), &["add", "README.md", ".gitignore"]);
        git_ok(repo.path(), &["commit", "-m", "init"]);

        let remote = if with_remote {
            let remote = tempfile::tempdir().unwrap();
            git_ok(remote.path(), &["init", "--bare"]);
            git_ok(
                repo.path(),
                &["remote", "add", "origin", remote.path().to_str().unwrap()],
            );
            Some(remote)
        } else {
            None
        };

        let branch = "grove/s_test".to_string();
        git_ok(repo.path(), &["checkout", "-b", &branch]);
        db::initialize(repo.path()).unwrap();

        Self {
            repo,
            remote,
            branch,
            conversation_id: "conv_publish".to_string(),
        }
    }

    fn connect(&self) -> Connection {
        DbHandle::new(self.repo.path()).connect().unwrap()
    }

    fn seed_conversation(&self, conn: &Connection) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR IGNORE INTO conversations
             (id, project_id, title, state, branch_name, worktree_path, created_at, updated_at)
             VALUES (?1, 'proj1', 'Publish Conversation', 'active', ?2, ?3, ?4, ?4)",
            params![
                self.conversation_id,
                self.branch,
                self.repo.path().to_string_lossy().to_string(),
                now,
            ],
        )
        .unwrap();
    }

    fn insert_run(&self, conn: &Connection, run_id: &str, state: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs
             (id, objective, state, budget_usd, cost_used_usd, publish_status, conversation_id, created_at, updated_at)
             VALUES (?1, 'Publish test objective', ?2, 0, 0, 'pending_retry', ?3, ?4, ?4)",
            params![run_id, state, self.conversation_id, now],
        )
        .unwrap();
    }

    fn insert_issue(&self, conn: &Connection, run_id: &str, provider: &str, external_id: &str) {
        let now = Utc::now().to_rfc3339();
        let issue_id = format!("{provider}:{external_id}");
        conn.execute(
            "INSERT INTO issues
             (id, project_id, title, body, status, provider, external_id, run_id, created_at, updated_at, raw_json)
             VALUES (?1, 'proj1', 'Linked issue', NULL, 'open', ?2, ?3, ?4, ?5, ?5, '{}')",
            params![issue_id, provider, external_id, run_id, now],
        )
        .unwrap();
    }

    fn insert_running_task(&self, conn: &Connection, task_id: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tasks
             (id, objective, state, priority, queued_at, started_at, conversation_id)
             VALUES (?1, 'Task objective', 'running', 0, ?2, ?2, ?3)",
            params![task_id, now, self.conversation_id],
        )
        .unwrap();
    }

    fn git_stdout(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(self.repo.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn remote_git_stdout(&self, args: &[&str]) -> String {
        let remote = self.remote.as_ref().expect("remote repo required");
        let out = Command::new("git")
            .args(args)
            .current_dir(remote.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "remote git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }
}

impl GhStub {
    fn install() -> (Self, PathGuard) {
        let guard = ENV_LOCK.lock().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        let gh_path = bin_dir.path().join("gh");
        let open_pr_file = bin_dir.path().join("open_pr");
        let create_count_file = bin_dir.path().join("pr_create_count");
        let issue_mode_file = bin_dir.path().join("issue_mode");
        fs::write(&issue_mode_file, "success\n").unwrap();

        let script = format!(
            "#!/bin/sh\n\
OPEN_PR_FILE=\"{}\"\n\
CREATE_COUNT_FILE=\"{}\"\n\
ISSUE_MODE_FILE=\"{}\"\n\
case \"$1 $2\" in\n\
  \"pr list\")\n\
    if [ -f \"$OPEN_PR_FILE\" ]; then\n\
      printf '[{{\"number\":1,\"url\":\"https://example.test/pr/1\"}}]'\n\
    else\n\
      printf '[]'\n\
    fi\n\
    ;;\n\
  \"pr create\")\n\
    count=0\n\
    if [ -f \"$CREATE_COUNT_FILE\" ]; then count=$(cat \"$CREATE_COUNT_FILE\"); fi\n\
    count=$((count + 1))\n\
    printf '%s' \"$count\" > \"$CREATE_COUNT_FILE\"\n\
    printf 'https://example.test/pr/1' > \"$OPEN_PR_FILE\"\n\
    printf 'https://example.test/pr/1\\n'\n\
    ;;\n\
  \"pr edit\") exit 0 ;;\n\
  \"pr view\") printf '{{\"comments\":[]}}' ;;\n\
  \"pr comment\") exit 0 ;;\n\
  \"issue view\") printf '{{\"comments\":[]}}' ;;\n\
  \"issue comment\")\n\
    mode=success\n\
    if [ -f \"$ISSUE_MODE_FILE\" ]; then mode=$(cat \"$ISSUE_MODE_FILE\"); fi\n\
    if [ \"$mode\" = \"fail\" ]; then\n\
      echo 'issue comment failed' >&2\n\
      exit 1\n\
    fi\n\
    printf 'https://example.test/issue/comment/1\\n'\n\
    ;;\n\
  *)\n\
    echo \"unsupported gh invocation: $1 $2\" >&2\n\
    exit 1\n\
    ;;\n\
esac\n",
            open_pr_file.display(),
            create_count_file.display(),
            issue_mode_file.display(),
        );
        fs::write(&gh_path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&gh_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&gh_path, perms).unwrap();
        }

        let old_path = std::env::var("PATH").ok();
        unsafe {
            std::env::set_var(
                "PATH",
                format!(
                    "{}:{}",
                    bin_dir.path().display(),
                    old_path.as_deref().unwrap_or("")
                ),
            );
        }

        (
            Self {
                _guard: guard,
                _bin_dir: bin_dir,
                open_pr_file,
                create_count_file,
                issue_mode_file,
            },
            PathGuard { old_path },
        )
    }

    fn set_issue_mode(&self, mode: &str) {
        fs::write(&self.issue_mode_file, format!("{mode}\n")).unwrap();
    }

    fn create_count(&self) -> usize {
        fs::read_to_string(&self.create_count_file)
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or(0)
    }

    #[allow(dead_code)]
    fn pr_url(&self) -> Option<String> {
        fs::read_to_string(&self.open_pr_file)
            .ok()
            .map(|value| value.trim().to_string())
    }
}

fn base_publish_config() -> grove_core::config::GroveConfig {
    let mut cfg = default_config();
    cfg.publish.enabled = true;
    cfg.publish.auto_on_success = true;
    cfg.publish.retry_on_startup = true;
    cfg.publish.comment_on_issue = false;
    cfg.publish.comment_on_pr = true;
    cfg.tracker.mode = TrackerMode::GitHub;
    cfg.tracker.github.enabled = true;
    cfg
}

fn git_ok(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write_config(project_root: &Path, cfg: &grove_core::config::GroveConfig) {
    let config_dir = project_root.join(".grove");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("grove.yaml"),
        serde_yaml::to_string(cfg).unwrap(),
    )
    .unwrap();
}

#[test]
fn no_change_run_skips_commit_and_pr() {
    let repo = TestRepo::new(false);
    let conn = repo.connect();
    repo.seed_conversation(&conn);
    repo.insert_run(&conn, "run_no_changes", "publishing");
    let mut conn = conn;
    let cfg = base_publish_config();

    let result = publish::publish_run(
        &mut conn,
        &cfg,
        repo.repo.path(),
        "run_no_changes",
        repo.repo.path(),
        None,
        None,
    )
    .unwrap();

    assert_eq!(result.publish_status, "skipped_no_changes");
    assert_eq!(result.final_commit_sha, None);
    assert_eq!(
        repo.git_stdout(&["rev-list", "--count", "HEAD"]).trim(),
        "1"
    );
}

#[test]
fn second_run_reuses_same_pr_for_same_conversation_branch() {
    let repo = TestRepo::new(true);
    let (stub, _path_guard) = GhStub::install();
    let conn = repo.connect();
    repo.seed_conversation(&conn);
    repo.insert_run(&conn, "run_first_publish", "publishing");
    let mut conn = conn;
    let cfg = base_publish_config();

    fs::write(repo.repo.path().join("first.txt"), "first\n").unwrap();
    let first = publish::publish_run(
        &mut conn,
        &cfg,
        repo.repo.path(),
        "run_first_publish",
        repo.repo.path(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(first.publish_status, "published");
    assert_eq!(stub.create_count(), 1);

    conn.execute(
        "UPDATE runs SET state = 'completed' WHERE id = 'run_first_publish'",
        [],
    )
    .unwrap();
    repo.insert_run(&conn, "run_second_publish", "publishing");
    fs::write(repo.repo.path().join("second.txt"), "second\n").unwrap();

    let second = publish::publish_run(
        &mut conn,
        &cfg,
        repo.repo.path(),
        "run_second_publish",
        repo.repo.path(),
        None,
        None,
    )
    .unwrap();

    assert_eq!(second.publish_status, "published");
    assert_eq!(second.pr_url, first.pr_url);
    assert_eq!(stub.create_count(), 1);
    assert_eq!(
        repo.git_stdout(&["rev-list", "--count", "HEAD"]).trim(),
        "3"
    );
}

#[test]
fn retry_publish_succeeds_without_new_commit_and_reuses_existing_pr() {
    let repo = TestRepo::new(true);
    let (stub, _path_guard) = GhStub::install();
    stub.set_issue_mode("fail");
    let conn = repo.connect();
    repo.seed_conversation(&conn);
    repo.insert_run(&conn, "run_retry_publish", "publishing");
    repo.insert_issue(&conn, "run_retry_publish", "github", "123");
    let mut conn = conn;
    let mut cfg = base_publish_config();
    cfg.publish.comment_on_issue = true;
    write_config(repo.repo.path(), &cfg);

    fs::write(repo.repo.path().join("retry.txt"), "retry\n").unwrap();
    let first = publish::publish_run(
        &mut conn,
        &cfg,
        repo.repo.path(),
        "run_retry_publish",
        repo.repo.path(),
        None,
        None,
    )
    .unwrap();
    let first_sha = first.final_commit_sha.clone().unwrap();
    assert_eq!(first.publish_status, "failed");
    assert_eq!(stub.create_count(), 1);

    stub.set_issue_mode("success");
    let retried = orchestrator::retry_publish_run(repo.repo.path(), "run_retry_publish").unwrap();

    assert_eq!(retried.publish_status, "published");
    assert_eq!(
        retried.final_commit_sha.as_deref(),
        Some(first_sha.as_str())
    );
    assert_eq!(retried.pr_url, first.pr_url);
    assert_eq!(stub.create_count(), 1);
    assert_eq!(
        repo.git_stdout(&["rev-list", "--count", "HEAD"]).trim(),
        "2"
    );
}

#[test]
fn recover_interrupted_publish_reuses_existing_commit() {
    let repo = TestRepo::new(true);
    let (_stub, _path_guard) = GhStub::install();
    let conn = repo.connect();
    repo.seed_conversation(&conn);
    repo.insert_run(&conn, "run_recover_publish", "publishing");
    let mut conn = conn;

    fs::write(repo.repo.path().join("recover.txt"), "recover\n").unwrap();
    let mut disabled_cfg = base_publish_config();
    disabled_cfg.publish.auto_on_success = false;
    let pending = publish::publish_run(
        &mut conn,
        &disabled_cfg,
        repo.repo.path(),
        "run_recover_publish",
        repo.repo.path(),
        None,
        None,
    )
    .unwrap();
    let pending_sha = pending.final_commit_sha.clone().unwrap();
    assert_eq!(pending.publish_status, "pending_retry");

    let recovered =
        publish::recover_interrupted_publishes(&mut conn, repo.repo.path(), &base_publish_config())
            .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].publish_status, "published");
    assert_eq!(
        recovered[0].final_commit_sha.as_deref(),
        Some(pending_sha.as_str())
    );
}

#[test]
fn finish_task_copies_publish_metadata_from_run() {
    let repo = TestRepo::new(false);
    let conn = repo.connect();
    repo.seed_conversation(&conn);
    repo.insert_run(&conn, "run_task_publish", "completed");
    repo.insert_running_task(&conn, "task_publish_done");
    conn.execute(
        "UPDATE runs
         SET publish_status = 'published', publish_error = NULL, final_commit_sha = 'abc123', pr_url = 'https://example.test/pr/1'
         WHERE id = 'run_task_publish'",
        [],
    )
    .unwrap();
    drop(conn);

    orchestrator::finish_task(
        repo.repo.path(),
        "task_publish_done",
        "completed",
        Some("run_task_publish"),
    )
    .unwrap();

    let conn = repo.connect();
    let row: (String, Option<String>, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT publish_status, publish_error, final_commit_sha, pr_url
             FROM tasks WHERE id = 'task_publish_done'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(row.0, "published");
    assert_eq!(row.1, None);
    assert_eq!(row.2.as_deref(), Some("abc123"));
    assert_eq!(row.3.as_deref(), Some("https://example.test/pr/1"));
}

#[test]
fn publishing_fifth_run_pushes_all_previous_local_run_commits() {
    let repo = TestRepo::new(true);
    let (_stub, _path_guard) = GhStub::install();
    let conn = repo.connect();
    repo.seed_conversation(&conn);
    let mut conn = conn;

    let mut deferred_cfg = base_publish_config();
    deferred_cfg.publish.auto_on_success = false;

    for idx in 1..=4 {
        let run_id = format!("run_batch_{idx}");
        repo.insert_run(&conn, &run_id, "publishing");
        fs::write(
            repo.repo.path().join(format!("file_{idx}.txt")),
            format!("change {idx}\n"),
        )
        .unwrap();
        let result = publish::publish_run(
            &mut conn,
            &deferred_cfg,
            repo.repo.path(),
            &run_id,
            repo.repo.path(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(result.publish_status, "pending_retry");
        conn.execute(
            "UPDATE runs SET state = 'completed' WHERE id = ?1",
            [&run_id],
        )
        .unwrap();
    }

    repo.insert_run(&conn, "run_batch_5", "publishing");
    fs::write(repo.repo.path().join("file_5.txt"), "change 5\n").unwrap();
    let result = publish::publish_run(
        &mut conn,
        &base_publish_config(),
        repo.repo.path(),
        "run_batch_5",
        repo.repo.path(),
        None,
        None,
    )
    .unwrap();

    assert_eq!(result.publish_status, "published");
    assert_eq!(
        repo.git_stdout(&["rev-list", "--count", "HEAD"]).trim(),
        "6"
    );
    assert_eq!(
        repo.remote_git_stdout(&["rev-list", "--count", "refs/heads/grove/s_test"])
            .trim(),
        "6"
    );
}
