use std::path::{Path, PathBuf};

use crate::errors::GroveResult;

/// Return the path for a conversation's worktree, creating it if absent.
///
/// The worktree is placed on branch `{branch_prefix}/s_{conv_id}`.
/// Idempotent — safe to call on every run start.
///
/// Self-heals: if the directory exists but `git status` fails (e.g. the branch
/// was deleted manually), the corrupt worktree is removed and recreated.
pub fn ensure_conversation_worktree(
    project_root: &Path,
    conv_id: &str,
    branch_prefix: &str,
) -> GroveResult<PathBuf> {
    let wt_path = crate::config::grove_dir(project_root)
        .join("worktrees")
        .join(conv_id);

    // Health check: if the directory exists, verify git status works.
    // A failing status means the worktree is in a corrupt state (e.g. the
    // backing branch was deleted). Remove and fall through to recreation.
    if wt_path.exists() {
        if super::git_ops::git_current_branch(&wt_path).is_ok() {
            return Ok(wt_path); // healthy — nothing to do
        }
        tracing::warn!(conv_id, "conversation worktree corrupt — recreating");
        let _ = std::fs::remove_dir_all(&wt_path);
    }

    std::fs::create_dir_all(crate::config::grove_dir(project_root).join("worktrees"))?;

    let conv_branch = format!("{branch_prefix}/s_{conv_id}");
    let branch_exists =
        super::git_ops::git_branch_exists(project_root, &conv_branch).unwrap_or(false);
    if branch_exists {
        super::git_ops::git_worktree_checkout_existing(project_root, &wt_path, &conv_branch)?;
    } else {
        super::git_ops::git_worktree_add_from(project_root, &wt_path, &conv_branch, "HEAD")?;
    }
    tracing::info!(conv_id, path = %wt_path.display(), "created conversation worktree");
    Ok(wt_path)
}

/// Remove a conversation's worktree directory.
///
/// Called when a conversation is archived. The branch (`{prefix}/s_{conv_id}`)
/// is preserved for history — only the on-disk worktree is removed.
pub fn remove_conversation_worktree(project_root: &Path, conv_id: &str) -> GroveResult<()> {
    let wt_path = crate::config::grove_dir(project_root)
        .join("worktrees")
        .join(conv_id);
    if wt_path.exists() {
        if let Err(e) = super::git_ops::git_worktree_remove(project_root, &wt_path) {
            tracing::warn!(conv_id, error = %e, "git worktree remove failed, forcing fs remove");
            std::fs::remove_dir_all(&wt_path)?;
        }
        tracing::info!(conv_id, "removed conversation worktree");
    }
    Ok(())
}
