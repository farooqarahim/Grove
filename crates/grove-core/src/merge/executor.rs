use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::config::{HookEvent, HooksConfig};
use crate::errors::{GroveError, GroveResult};
use crate::hooks::{HookContext, run_hooks};

const MERGE_COMMAND_TIMEOUT_SECS: u64 = 60;

#[derive(Debug)]
pub enum MergeOutcome {
    /// Merge completed cleanly.
    Success { merged_branch: String },
    /// Merge had conflicts; default branch is untouched.
    Conflict { files: Vec<String> },
}

/// Execute `git merge --no-ff <branch>` into `target_branch` using a detached
/// temporary worktree. The caller's checked-out worktree is never modified.
///
/// On conflict the merge is aborted immediately so the default branch
/// is never left in a dirty state.
///
/// `session_id` is used only for the commit message. `run_id` is propagated
/// to hook context. `hooks_cfg` is consulted for `PreMerge` hooks; a blocking
/// hook failure aborts the merge before the commit is written.
pub fn execute(
    project_root: &Path,
    branch: &str,
    target_branch: &str,
    session_id: &str,
    run_id: &str,
    hooks_cfg: &HooksConfig,
) -> GroveResult<MergeOutcome> {
    let temp_id = format!("merge_{}", uuid::Uuid::new_v4().simple());
    let temp_parent = crate::config::grove_dir(project_root).join("worktrees");
    std::fs::create_dir_all(&temp_parent)?;
    let temp_path = temp_parent.join(&temp_id);

    crate::worktree::git_ops::git_worktree_add_detached_at(
        project_root,
        &temp_path,
        target_branch,
    )?;

    let result = (|| {
        // 2.4: Pre-flight — identify files changed on both sides before attempting
        // the merge. Logged as a warning; does not block the merge attempt.
        let likely_conflicts =
            crate::worktree::git_ops::git_preflight_conflict_check(&temp_path, branch);
        if !likely_conflicts.is_empty() {
            tracing::warn!(
                branch = %branch,
                target_branch = %target_branch,
                count = likely_conflicts.len(),
                files = %likely_conflicts.join(", "),
                "pre-merge: files changed on both sides — conflicts likely"
            );
        } else {
            tracing::debug!(branch = %branch, target_branch = %target_branch, "pre-merge: no overlapping changes detected");
        }

        let merge_out = run_command(
            Command::new("git")
                .args(["merge", "--no-ff", "--no-commit", branch])
                .current_dir(&temp_path),
            "git merge",
        )?;

        if merge_out.status.success() {
            let hook_ctx = HookContext {
                run_id: run_id.to_string(),
                session_id: Some(session_id.to_string()),
                agent_type: None,
                worktree_path: Some(temp_path.to_string_lossy().to_string()),
                event: HookEvent::PreMerge,
            };
            if let Err(e) = run_hooks(hooks_cfg, HookEvent::PreMerge, &hook_ctx, &temp_path) {
                tracing::warn!(error = %e, branch = %branch, target_branch = %target_branch, "pre_merge hook failed — aborting merge");
                let _ = abort_merge(&temp_path);
                return Err(e);
            }

            let commit_msg = format!("grove(merge): {branch} into {target_branch}");
            let commit_out = run_command(
                Command::new("git")
                    .args(["commit", "-m", &commit_msg])
                    .current_dir(&temp_path),
                "git commit",
            )?;

            if !commit_out.status.success() {
                let _ = abort_merge(&temp_path);
                let stderr = String::from_utf8_lossy(&commit_out.stderr)
                    .trim()
                    .to_string();
                return Err(GroveError::Runtime(format!(
                    "git commit failed after merge: {stderr}"
                )));
            }

            let new_sha = run_command(
                Command::new("git")
                    .args(["rev-parse", "HEAD"])
                    .current_dir(&temp_path),
                "git rev-parse HEAD",
            )?;
            if !new_sha.status.success() {
                return Err(GroveError::Runtime(format!(
                    "git rev-parse HEAD failed after merge: {}",
                    String::from_utf8_lossy(&new_sha.stderr).trim()
                )));
            }

            let new_sha = String::from_utf8_lossy(&new_sha.stdout).trim().to_string();
            crate::worktree::git_ops::git_update_ref(
                project_root,
                &format!("refs/heads/{target_branch}"),
                &new_sha,
            )?;

            return Ok(MergeOutcome::Success {
                merged_branch: branch.to_string(),
            });
        }

        let conflict_files = list_conflict_files(&temp_path);
        let _ = abort_merge(&temp_path);

        if conflict_files.is_empty() {
            let stderr = String::from_utf8_lossy(&merge_out.stderr)
                .trim()
                .to_string();
            let stdout = String::from_utf8_lossy(&merge_out.stdout)
                .trim()
                .to_string();
            let msg = if !stderr.is_empty() { stderr } else { stdout };
            return Err(GroveError::Runtime(format!("git merge failed: {msg}")));
        }

        Ok(MergeOutcome::Conflict {
            files: conflict_files,
        })
    })();

    let _ = crate::worktree::git_ops::git_worktree_remove(project_root, &temp_path);
    if temp_path.exists() {
        let _ = std::fs::remove_dir_all(&temp_path);
    }
    let _ = crate::worktree::git_ops::git_worktree_prune(project_root);

    result
}

fn abort_merge(project_root: &Path) -> std::io::Result<()> {
    Command::new("git")
        .args(["merge", "--abort"])
        .current_dir(project_root)
        .output()?;
    Ok(())
}

fn list_conflict_files(project_root: &Path) -> Vec<String> {
    let out = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(project_root)
        .output();

    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(_) => vec![],
    }
}

fn run_command(cmd: &mut Command, label: &str) -> GroveResult<Output> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| GroveError::Runtime(format!("{label} failed to start: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(MERGE_COMMAND_TIMEOUT_SECS);

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|e| GroveError::Runtime(format!("{label} output read failed: {e}")));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GroveError::Runtime(format!(
                        "{label} timed out after {}s",
                        MERGE_COMMAND_TIMEOUT_SECS
                    )));
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GroveError::Runtime(format!("{label} wait failed: {e}")));
            }
        }
    }
}
