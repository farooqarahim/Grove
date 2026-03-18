use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::{GroveError, GroveResult};

use super::{git_ops, paths};

/// A live worktree handle. Drop-safe — the caller is responsible for calling
/// `remove` explicitly so errors can be surfaced.
#[derive(Debug, Clone)]
pub struct WorktreeHandle {
    pub path: PathBuf,
    pub branch: String,
    /// `true` if this is a real `git worktree`, `false` if it is a plain
    /// directory created as a fallback (e.g. outside a git repo).
    pub is_git_worktree: bool,
    /// The commit SHA that HEAD pointed to when this worktree was created.
    /// Used by the merge layer to identify the common ancestor for 3-way merge.
    /// `None` for plain-directory worktrees or repos with no commits.
    pub base_commit: Option<String>,
}

/// Create an isolated worktree for `session_id` under `base_dir`.
///
/// Strategy:
/// 1. If `project_root` is inside a git repo, use `git worktree add` to
///    create a proper isolated branch.
/// 2. Otherwise, create a plain directory (useful in tests / CI without git).
///
/// Records the base commit SHA and writes a `.grove_worktree_meta.json` file
/// for crash recovery and in-flight upgrade safety.
pub fn create(
    project_root: &Path,
    base_dir: &Path,
    session_id: &str,
) -> GroveResult<WorktreeHandle> {
    fs::create_dir_all(base_dir)?;

    let wt_path = paths::worktree_path(base_dir, session_id);
    let branch = paths::branch_name_for_session(session_id);

    if git_ops::is_git_repo(project_root) && git_ops::has_commits(project_root) {
        let base_commit = git_ops::git_rev_parse_head(project_root).ok();
        git_ops::git_worktree_add(project_root, &wt_path, &branch)?;
        write_worktree_meta(&wt_path, session_id, base_commit.as_deref())?;
        return Ok(WorktreeHandle {
            path: wt_path,
            branch,
            is_git_worktree: true,
            base_commit,
        });
    }

    // Fallback: plain directory (no git, or zero-commit repo).
    fs::create_dir_all(&wt_path)?;
    write_worktree_meta(&wt_path, session_id, None)?;
    Ok(WorktreeHandle {
        path: wt_path,
        branch,
        is_git_worktree: false,
        base_commit: None,
    })
}

/// Create an isolated worktree branching from a specific start point.
///
/// Used when chaining sequential agents: each agent branches from the
/// previous agent's committed branch tip, maintaining git ancestry.
pub fn create_from(
    project_root: &Path,
    base_dir: &Path,
    session_id: &str,
    start_point: &str,
) -> GroveResult<WorktreeHandle> {
    fs::create_dir_all(base_dir)?;

    let wt_path = paths::worktree_path(base_dir, session_id);
    let branch = paths::branch_name_for_session(session_id);

    if git_ops::is_git_repo(project_root) && git_ops::has_commits(project_root) {
        let base_commit = git_ops::git_rev_parse_head(project_root).ok();
        git_ops::git_worktree_add_from(project_root, &wt_path, &branch, start_point)?;
        write_worktree_meta(&wt_path, session_id, base_commit.as_deref())?;
        return Ok(WorktreeHandle {
            path: wt_path,
            branch,
            is_git_worktree: true,
            base_commit,
        });
    }

    // Fallback: plain directory (no git, or zero-commit repo).
    fs::create_dir_all(&wt_path)?;
    write_worktree_meta(&wt_path, session_id, None)?;
    Ok(WorktreeHandle {
        path: wt_path,
        branch,
        is_git_worktree: false,
        base_commit: None,
    })
}

/// Name of the metadata file written to each worktree for crash recovery
/// and in-flight upgrade safety.
pub const WORKTREE_META_FILENAME: &str = ".grove_worktree_meta.json";

/// Write a metadata file into the worktree directory.
///
/// This file records the Grove version and base commit at creation time so
/// that:
/// - The merge layer can identify the common ancestor without extra git calls.
/// - An upgrade mid-run can detect "old" worktrees and degrade gracefully.
/// - Crash recovery can identify partially-created worktrees.
fn write_worktree_meta(
    wt_path: &Path,
    session_id: &str,
    base_commit: Option<&str>,
) -> GroveResult<()> {
    let meta = serde_json::json!({
        "grove_version": env!("CARGO_PKG_VERSION"),
        "session_id": session_id,
        "base_commit": base_commit,
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let meta_path = wt_path.join(WORKTREE_META_FILENAME);
    fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta)
            .map_err(|e| GroveError::Runtime(format!("serialize worktree meta: {e}")))?,
    )
    .map_err(|e| GroveError::Runtime(format!("write worktree meta: {e}")))?;
    Ok(())
}

/// Read the base_commit from a worktree's metadata file.
///
/// Returns `None` if the file doesn't exist (old Grove created this worktree)
/// or if it's unparseable. Callers should fall back to `LastWriterWins` merge
/// when this returns `None`.
pub fn read_base_commit(wt_path: &Path) -> Option<String> {
    let meta_path = wt_path.join(WORKTREE_META_FILENAME);
    let content = fs::read_to_string(&meta_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    parsed.get("base_commit")?.as_str().map(String::from)
}

/// Remove a worktree created by `create`.
///
/// For real git worktrees, runs `git worktree remove --force`.
/// For plain directories, removes the directory tree.
pub fn remove(project_root: &Path, handle: &WorktreeHandle) -> GroveResult<()> {
    if handle.is_git_worktree {
        // Best-effort: ignore errors from prune/remove so callers stay clean.
        let _ = git_ops::git_worktree_remove(project_root, &handle.path);
        let _ = git_ops::git_worktree_prune(project_root);
    } else if handle.path.exists() {
        fs::remove_dir_all(&handle.path)?;
    }
    Ok(())
}
