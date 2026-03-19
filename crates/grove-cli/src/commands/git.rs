use std::path::Path;

use grove_core::git;
use grove_core::worktree::git_ops;

use crate::cli::{GitAction, GitArgs, MergeStrategy};
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json, text};

// ── Public dispatch ───────────────────────────────────────────────────────────

pub fn dispatch(args: GitArgs, project: &Path, mode: OutputMode) -> CliResult<()> {
    match args.action {
        GitAction::Status => status_cmd(project, mode),
        GitAction::Stage { paths } => stage_cmd(project, &paths, mode),
        GitAction::Unstage { paths } => unstage_cmd(project, &paths, mode),
        GitAction::Revert { paths, all } => revert_cmd(project, &paths, all, mode),
        GitAction::Commit { msg, all, push } => {
            commit_cmd(project, msg.as_deref(), all, push, mode)
        }
        GitAction::Push => push_cmd(project, mode),
        GitAction::Pull => pull_cmd(project, mode),
        GitAction::Branch => branch_cmd(project, mode),
        GitAction::Log { n } => log_cmd(project, n as usize, mode),
        GitAction::Undo => undo_cmd(project, mode),
        GitAction::Pr {
            title,
            body,
            base,
            push,
        } => pr_cmd(project, title, body, base, push, mode),
        GitAction::PrStatus => pr_status_cmd(project, mode),
        GitAction::Merge { strategy, admin } => merge_cmd(project, strategy, admin, mode),
    }
}

// ── status ────────────────────────────────────────────────────────────────────

fn status_cmd(project: &Path, mode: OutputMode) -> CliResult<()> {
    let changes = git::worktree_status(project).map_err(|e| CliError::Other(e.to_string()))?;

    let branch_info = git::branch_info(project).map_err(|e| CliError::Other(e.to_string()))?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({
                "branch": branch_info.branch,
                "ahead": branch_info.ahead,
                "behind": branch_info.behind,
                "files": changes.iter().map(|c| serde_json::json!({
                    "status": c.status,
                    "path": c.path,
                    "area": c.area,
                })).collect::<Vec<_>>(),
            });
            println!("{}", json::emit_json(&val));
        }
        OutputMode::Text { no_color } => {
            // Header: "branch: main  ↑2 ↓0"
            let ahead_behind = format_ahead_behind(branch_info.ahead, branch_info.behind);
            let branch_line = if no_color {
                format!("branch: {}  {}", branch_info.branch, ahead_behind)
            } else {
                format!(
                    "branch: {}  {}",
                    text::bold(&branch_info.branch),
                    ahead_behind
                )
            };
            println!("{branch_line}");

            if changes.is_empty() {
                println!("nothing to commit, working tree clean");
            } else {
                for c in &changes {
                    let prefix = status_prefix(&c.status, &c.area);
                    println!("{prefix}  {}", c.path);
                }
            }
        }
    }
    Ok(())
}

fn format_ahead_behind(ahead: i32, behind: i32) -> String {
    format!("\u{2191}{ahead} \u{2193}{behind}")
}

fn status_prefix(status: &str, area: &str) -> String {
    // Mimic porcelain: staged=XY where X=index, Y=worktree
    // We show a 2-char code: staged file gets "X " and unstaged gets " X"
    match area {
        "staged" => format!("{status} "),
        "unstaged" | "untracked" => format!(" {status}"),
        _ => format!("{status} "),
    }
}

// ── stage ─────────────────────────────────────────────────────────────────────

fn stage_cmd(project: &Path, paths: &[String], mode: OutputMode) -> CliResult<()> {
    if paths.is_empty() {
        return Err(CliError::BadArg("stage requires at least one path".into()));
    }

    if paths.len() == 1 && paths[0] == "." {
        git_ops::git_add_all(project).map_err(|e| CliError::Other(e.to_string()))?;
    } else {
        // Stage each path individually using git add via the worktree git_ops module.
        // git_ops doesn't expose per-path staging, so we invoke git directly via
        // grove_core::worktree::git_ops's git_add_all for "." or run a subprocess
        // for specific paths (consistent with the module's pattern of using Command).
        run_git(project, {
            let mut args = vec!["add".to_string(), "--".to_string()];
            args.extend_from_slice(paths);
            args
        })?;
    }

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json::emit_json(&serde_json::json!({ "staged": paths }))
            );
        }
        OutputMode::Text { .. } => {
            println!("Staged {} path(s).", paths.len());
        }
    }
    Ok(())
}

// ── unstage ───────────────────────────────────────────────────────────────────

fn unstage_cmd(project: &Path, paths: &[String], mode: OutputMode) -> CliResult<()> {
    if paths.is_empty() {
        return Err(CliError::BadArg(
            "unstage requires at least one path".into(),
        ));
    }

    run_git(project, {
        let mut args = vec!["reset".to_string(), "HEAD".to_string(), "--".to_string()];
        args.extend_from_slice(paths);
        args
    })?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json::emit_json(&serde_json::json!({ "unstaged": paths }))
            );
        }
        OutputMode::Text { .. } => {
            println!("Unstaged {} path(s).", paths.len());
        }
    }
    Ok(())
}

// ── revert ────────────────────────────────────────────────────────────────────

fn revert_cmd(project: &Path, paths: &[String], all: bool, mode: OutputMode) -> CliResult<()> {
    if all || paths.is_empty() {
        // Discard all working-tree changes and remove untracked files.
        run_git(project, vec!["checkout".to_string(), ".".to_string()])?;
        run_git(project, vec!["clean".to_string(), "-fd".to_string()])?;
        match mode {
            OutputMode::Json => println!(
                "{}",
                json::emit_json(&serde_json::json!({ "reverted": "all" }))
            ),
            OutputMode::Text { .. } => println!("Reverted all changes."),
        }
    } else {
        run_git(project, {
            let mut args = vec!["checkout".to_string(), "--".to_string()];
            args.extend_from_slice(paths);
            args
        })?;
        match mode {
            OutputMode::Json => println!(
                "{}",
                json::emit_json(&serde_json::json!({ "reverted": paths }))
            ),
            OutputMode::Text { .. } => println!("Reverted {} path(s).", paths.len()),
        }
    }
    Ok(())
}

// ── commit ────────────────────────────────────────────────────────────────────

fn commit_cmd(
    project: &Path,
    msg: Option<&str>,
    all: bool,
    push: bool,
    mode: OutputMode,
) -> CliResult<()> {
    if all {
        git_ops::git_add_all(project).map_err(|e| CliError::Other(e.to_string()))?;
    }

    let message = msg.unwrap_or("grove: committed via grove-cli");
    git_ops::git_commit(project, message).map_err(|e| CliError::Other(e.to_string()))?;

    let sha = git_ops::git_rev_parse_head(project).map_err(|e| CliError::Other(e.to_string()))?;
    let sha_short: String = sha.chars().take(7).collect();

    let mut pushed = false;
    let mut push_err: Option<String> = None;
    if push {
        match push_smart(project) {
            Ok(()) => pushed = true,
            Err(e) => push_err = Some(e.to_string()),
        }
    }

    match mode {
        OutputMode::Json => {
            let mut val = serde_json::json!({
                "sha": sha,
                "message": message,
            });
            if push {
                val["pushed"] = serde_json::json!(pushed);
                if let Some(ref e) = push_err {
                    val["push_error"] = serde_json::json!(e);
                }
            }
            println!("{}", json::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            println!("[{sha_short}] {message}");
            if pushed {
                println!("Pushed to origin.");
            } else if let Some(ref e) = push_err {
                eprintln!("Push failed: {e}");
            }
        }
    }
    Ok(())
}

// ── push ──────────────────────────────────────────────────────────────────────

fn push_cmd(project: &Path, mode: OutputMode) -> CliResult<()> {
    push_smart(project)?;
    match mode {
        OutputMode::Json => println!(
            "{}",
            json::emit_json(&serde_json::json!({ "pushed": true }))
        ),
        OutputMode::Text { .. } => println!("Pushed to origin."),
    }
    Ok(())
}

fn push_smart(project: &Path) -> CliResult<()> {
    let output = std::process::Command::new("git")
        .args(["push"])
        .current_dir(project)
        .output()
        .map_err(|e| CliError::Other(format!("failed to run git push: {e}")))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if stderr.contains("no upstream branch")
        || stderr.contains("has no upstream")
        || stderr.contains("--set-upstream")
    {
        let retry = std::process::Command::new("git")
            .args(["push", "--set-upstream", "origin", "HEAD"])
            .current_dir(project)
            .output()
            .map_err(|e| CliError::Other(format!("failed to run git push --set-upstream: {e}")))?;

        if retry.status.success() {
            return Ok(());
        }
        let retry_err = String::from_utf8_lossy(&retry.stderr).to_string();
        return Err(CliError::Other(friendly_push_error(&retry_err)));
    }

    Err(CliError::Other(friendly_push_error(&stderr)))
}

fn friendly_push_error(stderr: &str) -> String {
    if stderr.contains("non-fast-forward") {
        "Push rejected: remote has changes you don't have locally. Pull first.".to_string()
    } else if stderr.contains("Permission denied") || stderr.contains("403") {
        "Push failed: permission denied. Check your git credentials.".to_string()
    } else if stderr.contains("could not read Username") {
        "Push failed: authentication required. Run `gh auth login`.".to_string()
    } else {
        format!("git push failed: {stderr}")
    }
}

// ── pull ──────────────────────────────────────────────────────────────────────

fn pull_cmd(project: &Path, mode: OutputMode) -> CliResult<()> {
    let output = run_git_output(project, vec!["pull".to_string()])?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json::emit_json(&serde_json::json!({ "pulled": true }))
            );
        }
        OutputMode::Text { .. } => {
            if stdout.trim() == "Already up to date." {
                println!("Already up to date.");
            } else {
                print!("{stdout}");
            }
        }
    }
    Ok(())
}

// ── branch ────────────────────────────────────────────────────────────────────

fn branch_cmd(project: &Path, mode: OutputMode) -> CliResult<()> {
    let info = git::branch_info(project).map_err(|e| CliError::Other(e.to_string()))?;

    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json::emit_json(&serde_json::json!({
                    "branch": info.branch,
                    "default_branch": info.default_branch,
                    "ahead": info.ahead,
                    "behind": info.behind,
                }))
            );
        }
        OutputMode::Text { .. } => {
            let ab = format_ahead_behind(info.ahead, info.behind);
            println!("{} {ab}", info.branch);
            println!("  default: {}", info.default_branch);
        }
    }
    Ok(())
}

// ── log ───────────────────────────────────────────────────────────────────────

fn log_cmd(project: &Path, n: usize, mode: OutputMode) -> CliResult<()> {
    let commits = git::commit_log(project, n).map_err(|e| CliError::Other(e.to_string()))?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::json!({
                "commits": commits.iter().map(|c| serde_json::json!({
                    "hash": c.hash,
                    "subject": c.subject,
                    "author": c.author,
                    "date": c.date,
                    "is_pushed": c.is_pushed,
                })).collect::<Vec<_>>(),
            });
            println!("{}", json::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if commits.is_empty() {
                println!("No commits found.");
                return Ok(());
            }
            for commit in &commits {
                let hash_short: String = commit.hash.chars().take(7).collect();
                let pushed = if commit.is_pushed { "  [pushed]" } else { "" };
                // Truncate subject to 40 chars for display.
                let subject: String = commit.subject.chars().take(40).collect();
                // Trim to date only (first 10 chars of RFC3339).
                let date: String = commit.date.chars().take(10).collect();
                println!("* {hash_short}  {subject:<40}  {date}{pushed}");
            }
        }
    }
    Ok(())
}

// ── undo ──────────────────────────────────────────────────────────────────────

fn undo_cmd(project: &Path, mode: OutputMode) -> CliResult<()> {
    // Safety: verify HEAD~1 exists.
    let has_parent = git_ops::git_rev_parse(project, "HEAD~1").is_ok();
    if !has_parent {
        return Err(CliError::Other(
            "Cannot undo: this is the initial commit.".into(),
        ));
    }

    // Soft reset — keeps changes staged.
    run_git(
        project,
        vec![
            "reset".to_string(),
            "--soft".to_string(),
            "HEAD~1".to_string(),
        ],
    )?;

    // Get subject of what we just undid (now at new HEAD).
    let subject = run_git_stdout(
        project,
        vec![
            "log".to_string(),
            "-1".to_string(),
            "--pretty=format:%s".to_string(),
        ],
    )
    .unwrap_or_else(|_| "(unknown)".to_string());

    match mode {
        OutputMode::Json => println!(
            "{}",
            json::emit_json(&serde_json::json!({ "undone": true, "subject": subject }))
        ),
        OutputMode::Text { .. } => println!("Undid last commit. Changes are now staged."),
    }
    Ok(())
}

// ── pr ────────────────────────────────────────────────────────────────────────

fn pr_cmd(
    _project: &Path,
    _title: Option<String>,
    _body: Option<String>,
    _base: Option<String>,
    _push: bool,
    _mode: OutputMode,
) -> CliResult<()> {
    Err(CliError::Other(
        "grove git pr: not yet available — use `gh pr create` directly.".into(),
    ))
}

// ── pr-status ─────────────────────────────────────────────────────────────────

fn pr_status_cmd(_project: &Path, _mode: OutputMode) -> CliResult<()> {
    Err(CliError::Other(
        "grove git pr-status: not yet available — use `gh pr view` directly.".into(),
    ))
}

// ── merge ─────────────────────────────────────────────────────────────────────

fn merge_cmd(
    _project: &Path,
    _strategy: Option<MergeStrategy>,
    _admin: bool,
    _mode: OutputMode,
) -> CliResult<()> {
    Err(CliError::Other(
        "grove git merge: not yet available — use `gh pr merge` directly.".into(),
    ))
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Run a git subcommand in `cwd`; return `Err` on non-zero exit.
fn run_git(cwd: &Path, args: Vec<String>) -> CliResult<()> {
    run_git_output(cwd, args)?;
    Ok(())
}

/// Run a git subcommand and return the raw output.
fn run_git_output(cwd: &Path, args: Vec<String>) -> CliResult<std::process::Output> {
    let output = std::process::Command::new("git")
        .args(&args)
        .current_dir(cwd)
        .output()
        .map_err(|e| CliError::Other(format!("failed to spawn git: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let cmd = args.first().map(String::as_str).unwrap_or("git");
        return Err(CliError::Other(format!("git {cmd} failed: {stderr}")));
    }
    Ok(output)
}

/// Run a git subcommand and return stdout as a trimmed String.
fn run_git_stdout(cwd: &Path, args: Vec<String>) -> CliResult<String> {
    let output = run_git_output(cwd, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Helper: create a minimal git repo with one initial commit.
    fn make_git_repo() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "test@grove.test"]);
        run(&["config", "user.name", "Grove Test"]);
        std::fs::write(dir.path().join("README.md"), "# Test\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "initial commit"]);
        dir
    }

    // ── Spec tests ────────────────────────────────────────────────────────────

    #[test]
    fn git_status_on_non_git_dir_returns_error_not_panic() {
        let dir = tempdir().unwrap();
        let result = status_cmd(dir.path(), OutputMode::Text { no_color: true });
        // Non-git directory must return Err, not panic.
        assert!(
            result.is_err(),
            "status on non-git dir should return Err, got Ok"
        );
    }

    #[test]
    fn git_dispatch_compiles_with_all_actions() {
        // Compilation test: all GitAction variants are handled.
        let _ = |a: GitArgs, p: &std::path::Path, m: OutputMode| dispatch(a, p, m);
    }

    // ── status ────────────────────────────────────────────────────────────────

    #[test]
    fn status_clean_repo_prints_nothing_to_commit() {
        let dir = make_git_repo();
        // Must not panic.
        let result = status_cmd(dir.path(), OutputMode::Text { no_color: true });
        assert!(result.is_ok(), "status on clean repo failed: {result:?}");
    }

    #[test]
    fn status_json_mode_does_not_panic() {
        let dir = make_git_repo();
        let result = status_cmd(dir.path(), OutputMode::Json);
        assert!(result.is_ok(), "status json mode failed: {result:?}");
    }

    // ── log ───────────────────────────────────────────────────────────────────

    #[test]
    fn log_returns_initial_commit() {
        let dir = make_git_repo();
        let result = log_cmd(dir.path(), 10, OutputMode::Text { no_color: true });
        assert!(result.is_ok(), "log failed: {result:?}");
    }

    #[test]
    fn log_json_mode_does_not_panic() {
        let dir = make_git_repo();
        let result = log_cmd(dir.path(), 5, OutputMode::Json);
        assert!(result.is_ok(), "log json failed: {result:?}");
    }

    // ── branch ────────────────────────────────────────────────────────────────

    #[test]
    fn branch_cmd_on_main_succeeds() {
        let dir = make_git_repo();
        let result = branch_cmd(dir.path(), OutputMode::Text { no_color: true });
        assert!(result.is_ok(), "branch cmd failed: {result:?}");
    }

    // ── stage/unstage ─────────────────────────────────────────────────────────

    #[test]
    fn stage_empty_paths_returns_bad_arg() {
        let dir = make_git_repo();
        let result = stage_cmd(dir.path(), &[], OutputMode::Text { no_color: true });
        assert!(matches!(result, Err(CliError::BadArg(_))));
    }

    #[test]
    fn unstage_empty_paths_returns_bad_arg() {
        let dir = make_git_repo();
        let result = unstage_cmd(dir.path(), &[], OutputMode::Text { no_color: true });
        assert!(matches!(result, Err(CliError::BadArg(_))));
    }

    #[test]
    fn stage_new_file_succeeds() {
        let dir = make_git_repo();
        std::fs::write(dir.path().join("new.txt"), "hello\n").unwrap();
        let result = stage_cmd(
            dir.path(),
            &["new.txt".to_string()],
            OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok(), "stage failed: {result:?}");
    }

    // ── revert ────────────────────────────────────────────────────────────────

    #[test]
    fn revert_all_flag_on_clean_repo_succeeds() {
        let dir = make_git_repo();
        // Revert --all on a clean repo should not error.
        let result = revert_cmd(dir.path(), &[], true, OutputMode::Text { no_color: true });
        assert!(result.is_ok(), "revert --all failed: {result:?}");
    }

    // ── commit ────────────────────────────────────────────────────────────────

    #[test]
    fn commit_cmd_with_staged_change_succeeds() {
        let dir = make_git_repo();
        std::fs::write(dir.path().join("change.txt"), "new\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "change.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let result = commit_cmd(
            dir.path(),
            Some("test commit"),
            false,
            false,
            OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok(), "commit failed: {result:?}");
    }

    // ── undo ──────────────────────────────────────────────────────────────────

    #[test]
    fn undo_initial_commit_returns_error() {
        let dir = make_git_repo();
        let result = undo_cmd(dir.path(), OutputMode::Text { no_color: true });
        // Initial commit has no parent — must error.
        assert!(result.is_err(), "undo on initial commit should fail");
    }

    #[test]
    fn undo_second_commit_leaves_changes_staged() {
        let dir = make_git_repo();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
        };
        std::fs::write(dir.path().join("second.txt"), "two\n").unwrap();
        git(&["add", "second.txt"]);
        git(&["commit", "-m", "second commit"]);

        let result = undo_cmd(dir.path(), OutputMode::Text { no_color: true });
        assert!(result.is_ok(), "undo second commit failed: {result:?}");

        // Verify second.txt is now staged (index has it, but HEAD does not).
        let status = git(&["status", "--porcelain=v1"]);
        let output = String::from_utf8_lossy(&status.stdout).to_string();
        assert!(
            output.contains("second.txt"),
            "second.txt should be staged after undo"
        );
    }

    // ── stub commands ─────────────────────────────────────────────────────────

    #[test]
    fn pr_cmd_returns_not_yet_available_error() {
        let dir = tempdir().unwrap();
        let result = pr_cmd(
            dir.path(),
            None,
            None,
            None,
            false,
            OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("not yet available"),
            "expected 'not yet available' in: {msg}"
        );
    }

    #[test]
    fn pr_status_cmd_returns_not_yet_available_error() {
        let dir = tempdir().unwrap();
        let result = pr_status_cmd(dir.path(), OutputMode::Text { no_color: true });
        assert!(result.is_err());
    }

    #[test]
    fn merge_cmd_returns_not_yet_available_error() {
        let dir = tempdir().unwrap();
        let result = merge_cmd(dir.path(), None, false, OutputMode::Text { no_color: true });
        assert!(result.is_err());
    }

    // ── format helpers ────────────────────────────────────────────────────────

    #[test]
    fn format_ahead_behind_produces_arrows() {
        let s = format_ahead_behind(2, 0);
        assert!(s.contains('2'), "should contain ahead count");
        assert!(s.contains('0'), "should contain behind count");
    }

    #[test]
    fn status_prefix_staged_has_trailing_space() {
        let p = status_prefix("M", "staged");
        assert_eq!(p, "M ", "staged prefix should be 'M '");
    }

    #[test]
    fn status_prefix_unstaged_has_leading_space() {
        let p = status_prefix("M", "unstaged");
        assert_eq!(p, " M", "unstaged prefix should be ' M'");
    }

    #[test]
    fn status_prefix_untracked_uses_leading_space() {
        let p = status_prefix("?", "untracked");
        assert_eq!(p, " ?", "untracked prefix should be ' ?'");
    }
}
