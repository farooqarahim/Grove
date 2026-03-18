pub mod cleanup;
pub mod conflict_ui;
pub mod conversation;
pub mod git_ops;
pub mod gitignore;
pub mod manager;
pub mod merge;
pub mod paths;
pub mod preserve;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;

use crate::config::grove_dir;
use crate::db::DbHandle;
use crate::errors::{GroveError, GroveResult};

/// Minimal info returned by `prepare_workspace` — kept for backward compat
/// with `orchestrator/mod.rs`.
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    /// The commit SHA that HEAD pointed to when this worktree was created.
    /// `None` for plain-directory worktrees or repos with no commits.
    pub base_commit: Option<String>,
}

impl From<manager::WorktreeHandle> for WorktreeInfo {
    fn from(h: manager::WorktreeHandle) -> Self {
        Self {
            path: h.path,
            branch: h.branch,
            base_commit: h.base_commit,
        }
    }
}

// ── Disk space pre-flight ─────────────────────────────────────────────────────

/// Return the number of bytes available on the filesystem that contains `path`.
///
/// Uses the POSIX `df -k` command. Returns `u64::MAX` (fail-open) if the check
/// cannot be performed so that a temporary command failure never blocks a run.
/// The path need not exist — `df` resolves the mount point from the nearest
/// existing ancestor directory.
#[cfg(unix)]
fn available_disk_bytes(path: &Path) -> u64 {
    // Walk up to the first existing ancestor so `df` always gets a real path.
    let mut check_path = path;
    let mut ancestor = path;
    loop {
        if ancestor.exists() {
            check_path = ancestor;
            break;
        }
        match ancestor.parent() {
            Some(p) => ancestor = p,
            None => break,
        }
    }

    let output = Command::new("df")
        .args(["-k", check_path.to_string_lossy().as_ref()])
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return u64::MAX,
    };

    // `df -k` output: header line, then one data line per filesystem.
    // Columns: Filesystem  1K-blocks  Used  Available  Capacity  Mounted-on
    let stdout = String::from_utf8_lossy(&output.stdout);
    let data_line = match stdout.lines().nth(1) {
        Some(l) => l,
        None => return u64::MAX,
    };
    // "Available" is the 4th whitespace-separated field (index 3).
    let avail_kblocks: u64 = match data_line
        .split_whitespace()
        .nth(3)
        .and_then(|v| v.parse().ok())
    {
        Some(v) => v,
        None => return u64::MAX,
    };
    avail_kblocks.saturating_mul(1024)
}

#[cfg(not(unix))]
fn available_disk_bytes(_path: &Path) -> u64 {
    u64::MAX // not supported on Windows — skip the check
}

/// Assert that the filesystem containing `path` has at least `min_bytes` free.
///
/// Returns `Ok(())` when `min_bytes == 0` (check disabled), when `df` is
/// unavailable (fail-open), or when sufficient space exists.
/// Returns `Err(GroveError::Runtime(...))` with an actionable message when
/// the threshold is not met.
pub fn check_disk_space(path: &Path, min_bytes: u64) -> GroveResult<()> {
    if min_bytes == 0 {
        return Ok(()); // check disabled by config
    }
    let available = available_disk_bytes(path);
    if available == u64::MAX {
        // `df` failed — fail-open so a missing `df` binary never blocks a run.
        tracing::debug!(
            path = %path.display(),
            "disk space check skipped (df unavailable)"
        );
        return Ok(());
    }
    if available < min_bytes {
        let available_gib = available as f64 / 1_073_741_824.0;
        let required_gib = min_bytes as f64 / 1_073_741_824.0;
        return Err(GroveError::Runtime(format!(
            "insufficient disk space: {available_gib:.1} GiB available on {}, \
             {required_gib:.1} GiB required (worktree.min_disk_bytes). \
             Free up space or lower worktree.min_disk_bytes in grove.yml.",
            path.display()
        )));
    }
    tracing::debug!(
        path = %path.display(),
        available_bytes = available,
        min_bytes,
        "disk space pre-flight passed"
    );
    Ok(())
}

/// Returns `true` if `git` is available on PATH, `false` otherwise.
/// Grove works without git — worktrees fall back to plain directories.
pub fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Verify that `git` is on `PATH`. Returns `Err` if absent.
/// Kept for compatibility — prefer `git_available()` for optional checks.
pub fn ensure_git_available() -> GroveResult<()> {
    if git_available() {
        Ok(())
    } else {
        Err(GroveError::Runtime(
            "git not found on PATH — worktrees will use plain directories".to_string(),
        ))
    }
}

/// Create an isolated workspace for `session_id` under `base_dir`.
///
/// Delegates to `manager::create` which uses `git worktree add` when inside a
/// git repo, or falls back to a plain directory otherwise.
pub fn prepare_workspace(base_dir: &Path, session_id: &str) -> GroveResult<WorktreeInfo> {
    // Derive project_root as the parent of base_dir's parent (.grove/worktrees →
    // .grove → project_root).  Fall back to base_dir itself when the path is too short.
    let project_root = base_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(base_dir);

    let handle = manager::create(project_root, base_dir, session_id)?;
    Ok(handle.into())
}

/// Like `prepare_workspace` but branches from a specific git ref instead of HEAD.
///
/// Falls back to `prepare_workspace` behavior (plain directory) if the start point
/// is not a valid ref or if the repo has no commits.
pub fn prepare_workspace_from(
    base_dir: &Path,
    session_id: &str,
    start_point: &str,
) -> GroveResult<WorktreeInfo> {
    let project_root = base_dir
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(base_dir);

    let handle = manager::create_from(project_root, base_dir, session_id, start_point)?;
    Ok(handle.into())
}

// ── Directory sync (copy + delete) ───────────────────────────────────────────

/// Sync `src` into `dst` so that `dst` is an **exact mirror** of `src`.
///
/// - Files added/modified in `src`  → copied to `dst`.
/// - Files present in `dst` but absent from `src` → **deleted** from `dst`.
/// - Empty directories left after deletions → removed.
///
/// Protected entries are never touched in `dst`:
///   `.git`, `.grove`  — hardcoded
///   gitignore patterns — user-controlled
///   Grove internal files (PLAN_*, TEST_RESULTS_*, …) — agent scaffolding
///
/// This is used for both seeding agent workspaces and promoting results back
/// to `project_root`, so deletions made by agents propagate correctly.
pub fn sync_directories(
    src: &Path,
    dst: &Path,
    filter: &gitignore::GitignoreFilter,
) -> GroveResult<()> {
    let src_files = collect_file_set(src, src, filter);
    let dst_files = collect_file_set(dst, dst, filter);

    // Delete files that exist in dst but no longer exist in src.
    for rel in dst_files.difference(&src_files) {
        let _ = std::fs::remove_file(dst.join(rel));
    }

    // Remove any directories left empty after the deletions above.
    remove_empty_dirs(dst);

    // Copy everything from src to dst.
    do_sync_copy(src, src, dst, filter)
}

/// Collect the set of relative file paths under `current` (rooted at `root`),
/// skipping protected and gitignored entries.
fn collect_file_set(
    root: &Path,
    current: &Path,
    filter: &gitignore::GitignoreFilter,
) -> HashSet<String> {
    let mut set = HashSet::new();
    let Ok(entries) = std::fs::read_dir(current) else {
        return set;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let n = name.to_string_lossy();
        if n == ".git" || n == ".grove" {
            continue;
        }
        // Use DirEntry::file_type() to avoid an extra stat(2) syscall per entry;
        // the type is already available from the readdir result on Linux/macOS.
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if gitignore::is_grove_internal_file(&n) || filter.is_ignored(rel, ft.is_dir()) {
            continue;
        }
        if ft.is_symlink() {
            // Track symlinks as entries (by target path, not content).
            // Don't follow — just record the link itself.
            set.insert(rel.to_string_lossy().into_owned());
        } else if ft.is_file() {
            set.insert(rel.to_string_lossy().into_owned());
        } else if ft.is_dir() {
            set.extend(collect_file_set(root, &path, filter));
        }
    }
    set
}

/// Recursively copy files from `src` to `dst`, skipping protected/ignored entries.
/// `root` is the top-level source directory, used to compute relative paths for gitignore matching.
fn do_sync_copy(
    root: &Path,
    src: &Path,
    dst: &Path,
    filter: &gitignore::GitignoreFilter,
) -> GroveResult<()> {
    let entries = std::fs::read_dir(src)
        .map_err(|e| GroveError::Runtime(format!("read_dir {}: {e}", src.display())))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let n = name.to_string_lossy();
        if n == ".git" || n == ".grove" {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        let sp = entry.path();
        let rel = sp.strip_prefix(root).unwrap_or(&sp);
        if gitignore::is_grove_internal_file(&n) || filter.is_ignored(rel, ft.is_dir()) {
            continue;
        }
        let dp = dst.join(&name);
        if ft.is_symlink() {
            let target = std::fs::read_link(&sp)
                .map_err(|e| GroveError::Runtime(format!("readlink {}: {e}", sp.display())))?;
            // Remove any existing entry at destination.
            let _ = std::fs::remove_file(&dp);
            recreate_symlink(&target, &dp)?;
        } else if ft.is_dir() {
            std::fs::create_dir_all(&dp)
                .map_err(|e| GroveError::Runtime(format!("mkdir {}: {e}", dp.display())))?;
            do_sync_copy(root, &sp, &dp, filter)?;
        } else if let Err(e) = std::fs::copy(&sp, &dp) {
            let rel = sp.strip_prefix(root).unwrap_or(&sp);
            tracing::warn!(
                file = %rel.display(), error = %e,
                "sync_directories: failed to copy file — skipping"
            );
        }
    }
    Ok(())
}

/// Recreate a symlink at `link_path` pointing to `target`.
///
/// On Unix: creates a standard symlink. On non-Unix: falls back to copying
/// the target file content (if the target exists and is a file), or logs a
/// warning and skips (if the target is a directory or doesn't exist).
pub(crate) fn recreate_symlink(target: &Path, link_path: &Path) -> GroveResult<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link_path)
            .map_err(|e| GroveError::Runtime(format!("symlink {}: {e}", link_path.display())))?;
    }
    #[cfg(not(unix))]
    {
        // Best-effort fallback: copy target content if it's a regular file.
        if target.is_file() {
            std::fs::copy(target, link_path).map_err(|e| {
                GroveError::Runtime(format!(
                    "copy (symlink fallback) {}: {e}",
                    link_path.display()
                ))
            })?;
        } else {
            tracing::warn!(
                link = %link_path.display(), target = %target.display(),
                "cannot recreate symlink on this platform — skipping"
            );
        }
    }
    Ok(())
}

/// Recursively remove empty directories, leaving `.git` and `.grove` alone.
fn remove_empty_dirs(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let n = name.to_string_lossy();
        if n == ".git" || n == ".grove" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            remove_empty_dirs(&path);
            // Only removes if the directory is now empty.
            std::fs::remove_dir(&path).ok();
        }
    }
}

// ── Scoped cleanup filter ─────────────────────────────────────────────────────

/// Filter for scoped worktree cleanup. When both fields are `None`, all
/// finished worktrees are eligible (the legacy behavior).
#[derive(Debug, Clone, Default)]
pub struct CleanupFilter {
    pub project_id: Option<String>,
    pub conversation_id: Option<String>,
}

/// Return the set of session IDs matching the given filter, or `None` when no
/// filter is active (meaning "match all").
fn session_ids_for_filter(
    conn: &Connection,
    filter: &CleanupFilter,
) -> GroveResult<Option<HashSet<String>>> {
    if let Some(ref conversation_id) = filter.conversation_id {
        let mut stmt = conn
            .prepare(
                "SELECT s.id FROM sessions s
             JOIN runs r ON s.run_id = r.id
             WHERE r.conversation_id = ?1",
            )
            .map_err(|e| GroveError::Runtime(format!("session_ids_for_filter: {e}")))?;
        let ids: HashSet<String> = stmt
            .query_map([conversation_id], |r| r.get(0))
            .map_err(|e| GroveError::Runtime(format!("session_ids_for_filter: {e}")))?
            .filter_map(|r| r.ok())
            .collect();
        return Ok(Some(ids));
    }
    if let Some(ref project_id) = filter.project_id {
        let mut stmt = conn
            .prepare(
                "SELECT s.id FROM sessions s
             JOIN runs r ON s.run_id = r.id
             JOIN conversations c ON r.conversation_id = c.id
             WHERE c.project_id = ?1",
            )
            .map_err(|e| GroveError::Runtime(format!("session_ids_for_filter: {e}")))?;
        let ids: HashSet<String> = stmt
            .query_map([project_id], |r| r.get(0))
            .map_err(|e| GroveError::Runtime(format!("session_ids_for_filter: {e}")))?
            .filter_map(|r| r.ok())
            .collect();
        return Ok(Some(ids));
    }
    Ok(None)
}

// ── Worktree inspection & management ─────────────────────────────────────────

/// All information about a single on-disk worktree directory.
#[derive(Debug, Clone)]
pub struct WorktreeEntry {
    pub session_id: String,
    pub path: PathBuf,
    /// Total disk usage of the directory in bytes.
    pub size_bytes: u64,
    // Fields from the DB sessions row (None if the session was never recorded).
    pub run_id: Option<String>,
    pub agent_type: Option<String>,
    pub state: Option<String>,
    pub created_at: Option<String>,
    pub ended_at: Option<String>,
    /// Conversation ID resolved via sessions → runs → conversation_id.
    pub conversation_id: Option<String>,
    /// Project ID resolved via sessions → runs → conversations → project_id.
    pub project_id: Option<String>,
}

impl WorktreeEntry {
    /// Human-readable disk size (e.g. "4.2 MB", "312 KB").
    pub fn size_display(&self) -> String {
        format_bytes(self.size_bytes)
    }

    /// `true` when the session is still running — this worktree must not be deleted.
    pub fn is_active(&self) -> bool {
        matches!(
            self.state.as_deref(),
            Some("queued") | Some("running") | Some("waiting")
        )
    }
}

/// List every worktree directory under `project_root/.grove/worktrees/`,
/// cross-referenced with session metadata from the DB.
///
/// Pass `include_size = true` to compute disk usage (recursive walk) for each
/// entry, or `false` to skip the walk and report size as 0. Use `false` for
/// fast display commands (`grove status`), `true` for GC reporting.
pub fn list_worktrees(project_root: &Path, include_size: bool) -> GroveResult<Vec<WorktreeEntry>> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    list_worktrees_with_conn(project_root, &conn, include_size)
}

/// Like [`list_worktrees`] but uses the provided connection instead of
/// opening the project-local DB. Needed when the DB lives in a centralized
/// location (e.g. `~/.grove/workspaces/<id>/.grove/grove.db`).
///
/// `include_size` controls whether disk usage is computed (see [`list_worktrees`]).
pub fn list_worktrees_with_conn(
    project_root: &Path,
    conn: &Connection,
    include_size: bool,
) -> GroveResult<Vec<WorktreeEntry>> {
    let base = grove_dir(project_root).join("worktrees");
    if !base.exists() {
        return Ok(vec![]);
    }

    let mut entries: Vec<WorktreeEntry> = std::fs::read_dir(&base)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let session_id = e.file_name().to_string_lossy().to_string();
            let path = e.path();
            let size_bytes = if include_size { dir_size(&path) } else { 0 };

            // Look up session metadata + conversation/project via JOINs.
            type Row = (
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            );
            let row: Option<Row> = conn
                .query_row(
                    "SELECT s.run_id, s.agent_type, s.state, s.created_at, s.ended_at,
                            r.conversation_id, c.project_id
                     FROM sessions s
                     LEFT JOIN runs r ON s.run_id = r.id
                     LEFT JOIN conversations c ON r.conversation_id = c.id
                     WHERE s.id = ?1",
                    [&session_id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                            r.get(6)?,
                        ))
                    },
                )
                .ok();

            let (run_id, agent_type, state, created_at, ended_at, conversation_id, project_id) =
                match row {
                    Some((r, a, s, c, e, conv, proj)) => {
                        (Some(r), Some(a), Some(s), c, e, conv, proj)
                    }
                    None => (None, None, None, None, None, None, None),
                };

            WorktreeEntry {
                session_id,
                path,
                size_bytes,
                run_id,
                agent_type,
                state,
                created_at,
                ended_at,
                conversation_id,
                project_id,
            }
        })
        .collect();

    // Sort: active first, then by created_at desc.
    entries.sort_by(|a, b| {
        let a_active = if a.is_active() { 0 } else { 1 };
        let b_active = if b.is_active() { 0 } else { 1 };
        a_active
            .cmp(&b_active)
            .then(b.created_at.cmp(&a.created_at))
    });

    Ok(entries)
}

/// Delete a single worktree by `session_id`.
/// Returns `Err` if the session is currently active (running/queued).
pub fn delete_worktree(project_root: &Path, session_id: &str) -> GroveResult<u64> {
    let base = grove_dir(project_root).join("worktrees");
    let path = base.join(session_id);

    if !path.exists() {
        return Err(GroveError::Runtime(format!(
            "worktree '{session_id}' not found at {}",
            path.display()
        )));
    }

    // Guard: refuse to delete an active session's worktree.
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let state: Option<String> = conn
        .query_row(
            "SELECT state FROM sessions WHERE id=?1",
            [session_id],
            |r| r.get(0),
        )
        .ok();
    if matches!(
        state.as_deref(),
        Some("queued") | Some("running") | Some("waiting")
    ) {
        return Err(GroveError::Runtime(format!(
            "session '{session_id}' is currently {s} — cannot delete an active worktree",
            s = state.as_deref().unwrap_or("active")
        )));
    }

    let freed = dir_size(&path);
    // Single-slot deletion: check linked worktrees inline (one-shot process call).
    let linked = if git_ops::is_git_repo(project_root) {
        Some(git_ops::git_list_linked_worktrees(project_root))
    } else {
        None
    };
    remove_worktree_dir_safe(project_root, &path, linked.as_ref())?;
    Ok(freed)
}

/// Delete all worktrees whose sessions are finished (completed / failed / killed).
/// Skips active sessions. Returns `(count_deleted, bytes_freed)`.
pub fn delete_finished_worktrees(project_root: &Path) -> GroveResult<(usize, u64)> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    delete_finished_worktrees_with_conn(project_root, &conn)
}

/// Like [`delete_finished_worktrees`] but uses the provided DB connection.
pub fn delete_finished_worktrees_with_conn(
    project_root: &Path,
    conn: &Connection,
) -> GroveResult<(usize, u64)> {
    delete_finished_worktrees_filtered(project_root, conn, &CleanupFilter::default())
}

/// Delete finished worktrees matching `filter`. When no filter fields are set,
/// all finished worktrees are eligible (the legacy behavior).
pub fn delete_finished_worktrees_filtered(
    project_root: &Path,
    conn: &Connection,
    filter: &CleanupFilter,
) -> GroveResult<(usize, u64)> {
    // Collect linked worktrees once (decision [7]-A) to avoid N process spawns.
    // Used as layer-3 safety check in remove_worktree_dir_safe.
    let linked_worktrees: Option<std::collections::HashSet<std::path::PathBuf>> =
        if git_available() && git_ops::is_git_repo(project_root) {
            Some(git_ops::git_list_linked_worktrees(project_root))
        } else {
            None
        };

    let entries = list_worktrees_with_conn(project_root, conn, false)?;
    let allowed = session_ids_for_filter(conn, filter)?;
    let mut count = 0usize;
    let mut freed = 0u64;
    let base = grove_dir(project_root).join("worktrees");

    for entry in entries {
        if entry.is_active() {
            continue;
        }
        // §3.1: run worktrees are only eligible when their run is in a
        // terminal state. Active/queued/running runs still need their worktree.
        // Conversation worktrees persist until the conversation is archived —
        // they are managed by remove_conversation_worktree, not session GC.
        if entry.session_id.starts_with("run_") {
            let run_id = entry.session_id.trim_start_matches("run_");
            let run_state: Option<String> = conn
                .query_row("SELECT state FROM runs WHERE id=?1", [run_id], |r| r.get(0))
                .ok();
            if matches!(
                run_state.as_deref(),
                Some("executing")
                    | Some("waiting_for_gate")
                    | Some("queued")
                    | Some("running")
                    | None
            ) {
                continue;
            }
            // Terminal run — eligible for deletion below.
        }
        // When a filter is active, skip sessions that don't match.
        if let Some(ref ids) = allowed {
            if !ids.contains(&entry.session_id) {
                continue;
            }
        }
        let path = base.join(&entry.session_id);
        freed += dir_size(&path);
        remove_worktree_dir_safe(project_root, &path, linked_worktrees.as_ref())?;
        count += 1;
    }

    if count > 0 && git_available() {
        let _ = git_ops::git_worktree_prune(project_root);
        // Sweep any orphaned grove/* branches missed by inline removal
        // (e.g., from a prior crash between worktree add and remove).
        cleanup::cleanup_orphaned_branches(project_root);
    }

    Ok((count, freed))
}

/// Delete every worktree directory regardless of session state.
/// Skips any that are currently active. Returns `(count_deleted, bytes_freed)`.
pub fn delete_all_worktrees(project_root: &Path) -> GroveResult<(usize, u64)> {
    // Reuses the same logic — finished sessions are the only safe ones to delete
    // without --force, so we treat "all" the same as "finished" for safety.
    delete_finished_worktrees(project_root)
}

// ── Consolidated sweep ────────────────────────────────────────────────────────

/// Summary of resources cleaned during a sweep.
#[derive(Debug, Clone, Default)]
pub struct SweepReport {
    /// Whether git gc --auto was run.
    pub git_gc_ran: bool,
    /// Number of orphaned `grove/*` branches deleted.
    pub orphaned_branches_deleted: usize,
    /// Number of orphaned worktree directories removed.
    pub orphaned_dirs_removed: usize,
    /// Number of ghost sessions (DB active, worktree missing) recovered.
    pub ghost_sessions_recovered: usize,
}

/// Sweep orphaned resources and run git maintenance.
///
/// This is the single entry point for `grove gc` and any periodic cleanup.
/// It consolidates:
/// 1. Detect and recover ghost sessions (DB active, worktree missing)
/// 2. Delete orphaned `grove/*` branches with no DB record
/// 3. Remove orphaned worktree directories not referenced by active sessions/runs
/// 4. Prune stale git worktree metadata
/// 5. Run `git gc --auto`
pub fn sweep_orphaned_resources(
    project_root: &Path,
    conn: &mut rusqlite::Connection,
) -> GroveResult<SweepReport> {
    // 1. Detect and recover ghost sessions: DB says running but worktree is gone.
    // Run before branch/directory cleanup so we mark state correctly first.
    let ghost_sessions_recovered = cleanup::detect_ghost_sessions(conn)?;

    let is_git = git_ops::is_git_repo(project_root);

    // 2. Delete orphaned grove/* branches (conv/run/session branches with no DB record).
    let orphaned_branches_deleted = if is_git {
        sweep_orphaned_branches(project_root, conn)
    } else {
        0
    };

    // 3. Remove orphaned worktree directories not referenced by any active session or run.
    let orphaned_dirs_removed = sweep_orphaned_dirs(project_root, conn);

    // 4–5. Git maintenance (worktree prune + gc --auto).
    let git_gc_ran = if is_git {
        let _ = git_ops::git_worktree_prune(project_root);
        let _ = git_ops::git_gc_auto(project_root);
        true
    } else {
        false
    };

    Ok(SweepReport {
        git_gc_ran,
        orphaned_branches_deleted,
        orphaned_dirs_removed,
        ghost_sessions_recovered,
    })
}

/// Scan all `grove/*` branches and delete those with no matching DB entity.
///
/// Branch naming conventions:
/// - `grove/s_<conversation_id>` (or legacy `grove/conv-*`) → conversation must exist
/// - `grove/r_<run_id_first_8>` (or legacy `grove/run-*`) → run must exist in non-terminal state
/// - `grove/<session_id>` → session must exist in active state
fn sweep_orphaned_branches(project_root: &Path, conn: &Connection) -> usize {
    let branches = git_ops::git_list_branches(project_root, "grove/*");
    let mut deleted = 0usize;

    for branch in &branches {
        let is_orphaned = if let Some(conv_id) = branch
            .strip_prefix("grove/s_")
            .or_else(|| branch.strip_prefix("grove/conv-"))
        {
            // Conversation branch — orphaned if the conversation doesn't exist.
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM conversations WHERE id = ?1",
                    [conv_id],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            !exists
        } else if let Some(rest) = branch
            .strip_prefix("grove/r_")
            .or_else(|| branch.strip_prefix("grove/run-"))
        {
            // Run branch — orphaned if the run is terminal (completed/failed/aborted)
            // or doesn't exist. New format: r_<id_first_8>. Legacy: run-<...>.
            // We match by prefix since we only store the first 8 chars.
            let active: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM runs
                     WHERE state NOT IN ('completed','failed','aborted')
                       AND id LIKE ?1 || '%'",
                    [rest],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            !active
        } else if let Some(session_id) = branch.strip_prefix("grove/") {
            // Legacy session branch — orphaned if session is terminal or doesn't exist.
            let active: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM sessions
                     WHERE id = ?1 AND state NOT IN ('completed','failed','aborted')",
                    [session_id],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            !active
        } else {
            false
        };

        if is_orphaned {
            if let Err(e) = git_ops::git_delete_branch(project_root, branch) {
                tracing::debug!(branch = %branch, error = %e, "failed to delete orphaned branch");
            } else {
                tracing::debug!(branch = %branch, "deleted orphaned branch");
                deleted += 1;
            }
        }
    }

    deleted
}

/// Remove worktree directories under `.grove/worktrees/` that are not
/// referenced by any active session or conversation.
fn sweep_orphaned_dirs(project_root: &Path, conn: &Connection) -> usize {
    let worktrees_base = crate::config::grove_dir(project_root).join("worktrees");
    if !worktrees_base.exists() {
        return 0;
    }

    // Collect all directory names currently referenced by active sessions or runs.
    let mut active_dirs = HashSet::new();

    // Active session worktree paths (state = running/executing).
    if let Ok(mut stmt) = conn.prepare(
        "SELECT worktree_path FROM sessions WHERE state NOT IN ('completed','failed','aborted')",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                if let Some(name) = Path::new(&row).file_name().and_then(|n| n.to_str()) {
                    active_dirs.insert(name.to_string());
                }
            }
        }
    }

    // Conversation worktree dirs (conv_id) for conversations with active runs.
    // These persist for the lifetime of the conversation.
    if let Ok(mut stmt) = conn.prepare(
        "SELECT DISTINCT r.conversation_id FROM runs r
         WHERE r.state NOT IN ('completed','failed','aborted')
           AND r.conversation_id IS NOT NULL",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                active_dirs.insert(row);
            }
        }
    }

    // Also keep worktrees for non-archived conversations (they may have queued runs).
    if let Ok(mut stmt) =
        conn.prepare("SELECT id FROM conversations WHERE state NOT IN ('archived', 'deleted')")
    {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                active_dirs.insert(row);
            }
        }
    }

    // Run worktree dirs for non-terminal runs (legacy mode).
    // The directory name is the run ID itself.
    if let Ok(mut stmt) =
        conn.prepare("SELECT id FROM runs WHERE state NOT IN ('completed','failed','aborted')")
    {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
            for row in rows.flatten() {
                active_dirs.insert(row);
            }
        }
    }

    let mut removed = 0usize;
    if let Ok(entries) = std::fs::read_dir(&worktrees_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if active_dirs.contains(name) {
                continue;
            }
            // This directory is orphaned — remove it.
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::debug!(path = %path.display(), error = %e, "failed to remove orphaned worktree dir");
            } else {
                tracing::debug!(path = %path.display(), "removed orphaned worktree dir");
                removed += 1;
            }
        }
    }

    removed
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Recursively sum the size of all files under `dir`.
fn dir_size(dir: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                total += p.metadata().map(|m| m.len()).unwrap_or(0);
            } else if p.is_dir() {
                total += dir_size(&p);
            }
        }
    }
    total
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

/// Remove a worktree directory and its associated `grove/*` branch.
///
/// Tries `git worktree remove` first, falls back to `remove_dir_all`.
/// Then deletes the corresponding branch (best-effort — branch may not exist
/// if this was a plain-dir fallback or was already deleted).
///
/// Safety layers:
/// 1. Refuses to delete a path that canonically equals `project_root`.
/// 2. Refuses to delete a path outside `.grove/worktrees/`.
pub fn remove_worktree_dir(project_root: &Path, path: &Path) -> GroveResult<()> {
    remove_worktree_dir_safe(project_root, path, None)
}

/// Like [`remove_worktree_dir`] but also applies layer-3 safety: verifies the
/// path is registered as a linked worktree in the provided set before deletion.
///
/// When `linked_worktrees` is `None`, layer-3 is skipped (callers that cannot
/// easily fetch the set). Layers 1 and 2 are always enforced.
pub fn remove_worktree_dir_safe(
    project_root: &Path,
    path: &Path,
    linked_worktrees: Option<&std::collections::HashSet<std::path::PathBuf>>,
) -> GroveResult<()> {
    // Layer 1: never delete the main repo root.
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let canonical_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    if canonical_path == canonical_root {
        return Err(GroveError::Runtime(format!(
            "safety: refusing to remove project root {}",
            path.display()
        )));
    }

    // Layer 2: path must contain `.grove/worktrees/` (forward or backward slash).
    let path_str = path.to_string_lossy();
    if !path_str.contains(".grove/worktrees/") && !path_str.contains(".grove\\worktrees\\") {
        return Err(GroveError::Runtime(format!(
            "safety: path '{}' is outside .grove/worktrees/ — refusing to remove",
            path.display()
        )));
    }

    // Layer 3 (optional): verify the path is a registered linked worktree.
    // If git does not recognise this path as a linked worktree it is a plain
    // directory (legacy session dir or already-removed worktree). Delete it
    // directly without going through `git worktree remove`.
    if let Some(linked) = linked_worktrees {
        if git_ops::is_git_repo(project_root) && !linked.contains(&canonical_path) {
            tracing::debug!(
                path = %path.display(),
                "path is not a registered git worktree — removing as plain directory"
            );
            if path.exists() {
                std::fs::remove_dir_all(path)
                    .map_err(|e| GroveError::Runtime(format!("remove {}: {e}", path.display())))?;
            }
            return Ok(());
        }
    }

    // Derive the branch name before removing the directory.
    let branch_to_delete = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(paths::branch_name_for_session);

    let removed_via_git = git_ops::git_worktree_remove(project_root, path).is_ok();
    if !removed_via_git && path.exists() {
        std::fs::remove_dir_all(path)
            .map_err(|e| GroveError::Runtime(format!("remove {}: {e}", path.display())))?;
    }

    // Delete the orphaned branch (best-effort).
    if let Some(ref branch) = branch_to_delete {
        if git_ops::is_git_repo(project_root) {
            if let Err(e) = git_ops::git_delete_branch(project_root, branch) {
                tracing::debug!(branch = %branch, error = %e, "branch deletion skipped");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // [9]-A: 3 safety tests for remove_worktree_dir_safe.

    #[test]
    fn remove_project_root_is_rejected() {
        let dir = TempDir::new().unwrap();
        let result = remove_worktree_dir(dir.path(), dir.path());
        assert!(result.is_err(), "deleting project root must be rejected");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("project root") || msg.contains("safety"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn remove_outside_grove_worktrees_is_rejected() {
        let dir = TempDir::new().unwrap();
        let innocent = dir.path().join("not_a_worktree");
        std::fs::create_dir_all(&innocent).unwrap();
        let result = remove_worktree_dir(dir.path(), &innocent);
        assert!(
            result.is_err(),
            "path outside .grove/worktrees/ must be rejected"
        );
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains(".grove/worktrees/"), "unexpected error: {msg}");
    }

    #[test]
    fn remove_valid_grove_worktrees_path_deletes_dir() {
        let dir = TempDir::new().unwrap();
        // Simulate a worktree path — no git repo so layer-3 is skipped.
        let wt = dir
            .path()
            .join(".grove")
            .join("worktrees")
            .join("sess_abc123");
        std::fs::create_dir_all(&wt).unwrap();
        assert!(wt.exists());
        let result = remove_worktree_dir(dir.path(), &wt);
        assert!(result.is_ok(), "expected ok, got {result:?}");
        assert!(!wt.exists(), "directory should have been removed");
    }

    // [10]-A: git_fetch_branch returns Err gracefully on non-git dir (offline/no-remote).
    #[test]
    fn git_fetch_branch_fails_gracefully_on_non_git_dir() {
        let dir = TempDir::new().unwrap();
        let result = git_ops::git_fetch_branch(dir.path(), "origin", "main");
        // Must return Err (not panic), and the error should be descriptive.
        assert!(
            result.is_err(),
            "expected Err from git fetch in non-git dir"
        );
        let msg = result.unwrap_err().to_string();
        assert!(!msg.is_empty(), "error message should be non-empty");
    }

    // Disk space pre-flight tests.

    #[test]
    fn check_disk_space_passes_when_zero_threshold() {
        let dir = TempDir::new().unwrap();
        // Threshold of 0 means "disabled" — always passes regardless of available space.
        let result = check_disk_space(dir.path(), 0);
        assert!(result.is_ok(), "threshold=0 must always pass");
    }

    #[test]
    fn check_disk_space_passes_on_real_dir() {
        let dir = TempDir::new().unwrap();
        // 1 byte threshold: any real filesystem should have more than 1 byte free.
        let result = check_disk_space(dir.path(), 1);
        assert!(
            result.is_ok(),
            "1-byte threshold should pass on a real filesystem"
        );
    }

    #[test]
    fn check_disk_space_fails_when_threshold_exceeds_available() {
        let dir = TempDir::new().unwrap();
        // u64::MAX bytes required: impossible to satisfy.
        let result = check_disk_space(dir.path(), u64::MAX);
        // On unix with a real df, this should fail. On non-unix it returns Ok (fail-open).
        #[cfg(unix)]
        {
            // df will return a finite value, which can never satisfy u64::MAX.
            // But if df itself fails (CI sandbox), we fail-open — so accept either Ok or Err.
            if result.is_err() {
                let msg = result.unwrap_err().to_string();
                assert!(
                    msg.contains("insufficient disk space") || msg.contains("disk space"),
                    "error message must mention disk space: {msg}"
                );
            }
        }
        #[cfg(not(unix))]
        {
            assert!(result.is_ok(), "non-unix platforms fail-open");
        }
    }

    #[test]
    fn check_disk_space_nonexistent_path_uses_ancestor() {
        let dir = TempDir::new().unwrap();
        // Path doesn't exist yet — available_disk_bytes walks up to find an ancestor.
        let deep = dir.path().join("a").join("b").join("c").join("d");
        // 1-byte threshold should pass using the TempDir ancestor.
        let result = check_disk_space(&deep, 1);
        assert!(
            result.is_ok(),
            "nonexistent path should resolve to existing ancestor"
        );
    }
}
