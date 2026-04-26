//! Pure-Rust git operations via `gix` — no subprocess spawns.
//!
//! Architecture mirrors GitButler's `but-core` diff layer: status, diff, branch
//! info and commit log are all computed in-process using gitoxide. This eliminates
//! the latency and fragility of shelling out to `git` for every query.

pub mod publish;

use std::collections::HashMap;
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use anyhow::{Context, Result};
use gix::bstr::ByteSlice;

// ── Public data types ─────────────────────────────────────────────────────────

/// A single changed file as seen by `git status`.
///
/// Compatible with the `FileDiffEntry` shape used by the Tauri command layer so
/// that `commands.rs` can return these directly without re-mapping.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileChange {
    /// Single-character status: "M", "A", "D", "R", "?" (untracked), "C"
    pub status: String,
    /// Repo-relative path (UTF-8 lossy)
    pub path: String,
    /// `true` = change is already committed (not in the working tree)
    pub committed: bool,
    /// Which diff area to query: "staged", "unstaged", "untracked", or "committed"
    pub area: String,
}

/// Branch status info for a git repository.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BranchInfo {
    pub branch: String,
    pub default_branch: String,
    pub ahead: i32,
    pub behind: i32,
}

/// One commit from the log.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CommitInfo {
    pub hash: String,
    pub subject: String,
    pub body: String,
    pub author: String,
    pub date: String,
    pub is_pushed: bool,
}

// ── Repository helpers ────────────────────────────────────────────────────────

fn open_repo(path: &Path) -> Result<gix::Repository> {
    gix::open(path).with_context(|| format!("failed to open git repo at {}", path.display()))
}

fn resolve_workspace_branch(run_worktree: &Path, run_id: &str) -> String {
    if let Ok(repo) = open_repo(run_worktree) {
        if let Ok(head) = repo.head() {
            if let Some(name) = head.referent_name() {
                let branch = name.shorten().to_str_lossy().into_owned();
                if !branch.is_empty() {
                    return branch;
                }
            }
        }
    }

    format!("grove/r_{}", &run_id[..8.min(run_id.len())])
}

fn ignored_path_set(repo_path: &Path, paths: impl IntoIterator<Item = String>) -> HashSet<String> {
    let paths: Vec<String> = paths.into_iter().filter(|p| !p.is_empty()).collect();
    if paths.is_empty() {
        return HashSet::new();
    }

    let mut child = match std::process::Command::new("git")
        .args(["check-ignore", "--no-index", "--stdin"])
        .current_dir(repo_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return HashSet::new(),
    };

    if let Some(stdin) = child.stdin.as_mut() {
        for path in &paths {
            let _ = writeln!(stdin, "{path}");
        }
    }

    match child.wait_with_output() {
        Ok(output) if output.status.success() || output.status.code() == Some(1) => {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| line.trim().to_string())
                .filter(|line| !line.is_empty())
                .collect()
        }
        _ => HashSet::new(),
    }
}

fn filter_ignored_changes(repo_path: &Path, files: Vec<FileChange>) -> Vec<FileChange> {
    if files.is_empty() {
        return files;
    }
    let ignored = ignored_path_set(repo_path, files.iter().map(|file| file.path.clone()));
    if ignored.is_empty() {
        return files;
    }
    files
        .into_iter()
        .filter(|file| !ignored.contains(&file.path))
        .collect()
}

fn filter_ignored_diffs(
    repo_path: &Path,
    diffs: HashMap<String, String>,
) -> HashMap<String, String> {
    if diffs.is_empty() {
        return diffs;
    }
    let ignored = ignored_path_set(repo_path, diffs.keys().cloned());
    if ignored.is_empty() {
        return diffs;
    }
    diffs
        .into_iter()
        .filter(|(path, _)| !ignored.contains(path))
        .collect()
}

// ── Worktree status ───────────────────────────────────────────────────────────

/// Return all changed files visible in the working tree:
/// staged (index vs HEAD), unstaged (worktree vs index), and untracked.
///
/// This is the in-process equivalent of `git status --porcelain=v1`.
pub fn worktree_status(repo_path: &Path) -> Result<Vec<FileChange>> {
    use gix::dir::walk::EmissionMode;
    use gix::status;
    use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};

    let repo = open_repo(repo_path)?;

    let items: Vec<status::Item> = repo
        .status(gix::progress::Discard)?
        .tree_index_track_renames(status::tree_index::TrackRenames::Disabled)
        .index_worktree_rewrites(None)
        .index_worktree_submodules(gix::status::Submodule::Given {
            ignore: gix::submodule::config::Ignore::Dirty,
            check_dirty: true,
        })
        .index_worktree_options_mut(|opts| {
            if let Some(opts) = opts.dirwalk_options.as_mut() {
                opts.set_emit_ignored(None)
                    .set_emit_pruned(false)
                    .set_emit_tracked(false)
                    .set_emit_untracked(EmissionMode::Matching)
                    .set_emit_collapsed(None);
            }
        })
        .into_iter(None)?
        .filter_map(|item| item.ok())
        .collect();

    let mut changes: Vec<FileChange> = Vec::new();

    for item in items {
        match item {
            // ── Staged (TreeIndex) ─────────────────────────────────────────
            status::Item::TreeIndex(gix::diff::index::Change::Addition { location, .. }) => {
                changes.push(FileChange {
                    status: "A".to_string(),
                    path: location.to_str_lossy().into_owned(),
                    committed: false,
                    area: "staged".to_string(),
                });
            }
            status::Item::TreeIndex(gix::diff::index::Change::Deletion { location, .. }) => {
                changes.push(FileChange {
                    status: "D".to_string(),
                    path: location.to_str_lossy().into_owned(),
                    committed: false,
                    area: "staged".to_string(),
                });
            }
            status::Item::TreeIndex(
                gix::diff::index::Change::Modification { location, .. }
                | gix::diff::index::Change::Rewrite { location, .. },
            ) => {
                changes.push(FileChange {
                    status: "M".to_string(),
                    path: location.to_str_lossy().into_owned(),
                    committed: false,
                    area: "staged".to_string(),
                });
            }

            // ── Unstaged (IndexWorktree) ───────────────────────────────────
            status::Item::IndexWorktree(gix::status::index_worktree::Item::Modification {
                rela_path,
                status: EntryStatus::Change(Change::Removed),
                ..
            }) => {
                changes.push(FileChange {
                    status: "D".to_string(),
                    path: rela_path.to_str_lossy().into_owned(),
                    committed: false,
                    area: "unstaged".to_string(),
                });
            }
            status::Item::IndexWorktree(gix::status::index_worktree::Item::Modification {
                rela_path,
                status: EntryStatus::Change(Change::Modification { .. } | Change::Type { .. }),
                ..
            }) => {
                changes.push(FileChange {
                    status: "M".to_string(),
                    path: rela_path.to_str_lossy().into_owned(),
                    committed: false,
                    area: "unstaged".to_string(),
                });
            }
            status::Item::IndexWorktree(gix::status::index_worktree::Item::Modification {
                rela_path,
                status: EntryStatus::IntentToAdd,
                ..
            }) => {
                changes.push(FileChange {
                    status: "A".to_string(),
                    path: rela_path.to_str_lossy().into_owned(),
                    committed: false,
                    area: "unstaged".to_string(),
                });
            }

            // ── Untracked ──────────────────────────────────────────────────
            status::Item::IndexWorktree(gix::status::index_worktree::Item::DirectoryContents {
                entry:
                    gix::dir::Entry {
                        rela_path,
                        status: gix::dir::entry::Status::Untracked,
                        ..
                    },
                ..
            }) => {
                changes.push(FileChange {
                    status: "?".to_string(),
                    path: rela_path.to_str_lossy().into_owned(),
                    committed: false,
                    area: "untracked".to_string(),
                });
            }

            // Everything else (NeedsUpdate, submodule, tracked, ignored) — skip.
            _ => {}
        }
    }

    Ok(changes)
}

// ── Per-file diff ─────────────────────────────────────────────────────────────
//
// gix 0.70's unified_diff module is not yet public — it was added in 0.80.
// We use `git diff` subprocesses here but invoke the CORRECT command per area
// so that staged/unstaged/untracked files all show real diffs (fixes the empty-
// diff bug in the original implementation which always ran `git diff main...branch`).

/// Compute a unified diff for a single file.
///
/// `area` selects which comparison to make:
/// - `"staged"`    → `git diff --cached -- <file>`  (index vs HEAD)
/// - `"unstaged"`  → `git diff -- <file>`           (worktree vs index)
/// - `"untracked"` → `git diff --no-index /dev/null <file>` (new file vs nothing)
///
/// Returns an empty string if the diff is empty, the command fails, or the file
/// is binary.
pub fn file_diff(repo_path: &Path, file_path: &str, area: &str) -> Result<String> {
    let output = match area {
        "staged" => std::process::Command::new("git")
            .args(["diff", "--cached", "--", file_path])
            .current_dir(repo_path)
            .output()
            .with_context(|| format!("git diff --cached failed for {file_path}"))?,
        "unstaged" => std::process::Command::new("git")
            .args(["diff", "--", file_path])
            .current_dir(repo_path)
            .output()
            .with_context(|| format!("git diff failed for {file_path}"))?,
        "untracked" => {
            // git diff --no-index exits with code 1 when there are differences — that is normal.
            let full_path = repo_path.join(file_path);
            std::process::Command::new("git")
                .args([
                    "diff",
                    "--no-index",
                    "/dev/null",
                    full_path.to_str().unwrap_or(file_path),
                ])
                .current_dir(repo_path)
                .output()
                .with_context(|| format!("git diff --no-index failed for {file_path}"))?
        }
        _ => return Ok(String::new()),
    };
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Compute unified diffs for every staged/unstaged/untracked file in the worktree.
///
/// Returns a map of `file_path → diff_text`. Used by the frontend to pre-warm
/// the diff cache so that clicking a file shows content instantly.
///
/// Batches diffs by area: one `git diff` for all unstaged, one `git diff --cached`
/// for all staged, then per-file only for untracked (`--no-index` can't be batched).
pub fn all_diffs_in_worktree(repo_path: &Path) -> Result<HashMap<String, String>> {
    let changes = worktree_status(repo_path)?;
    Ok(batch_uncommitted_diffs(repo_path, &changes))
}

/// Batch diff computation: runs at most 2 subprocess calls (unstaged + staged)
/// plus one per untracked file, instead of one per changed file.
fn batch_uncommitted_diffs(repo_path: &Path, changes: &[FileChange]) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Batch: all unstaged diffs in one call.
    if changes.iter().any(|c| c.area == "unstaged") {
        if let Ok(out) = std::process::Command::new("git")
            .args(["diff"])
            .current_dir(repo_path)
            .output()
        {
            if !out.stdout.is_empty() {
                map.extend(split_diff_by_file(&String::from_utf8_lossy(&out.stdout)));
            }
        }
    }

    // Batch: all staged diffs in one call.
    if changes.iter().any(|c| c.area == "staged") {
        if let Ok(out) = std::process::Command::new("git")
            .args(["diff", "--cached"])
            .current_dir(repo_path)
            .output()
        {
            if !out.stdout.is_empty() {
                map.extend(split_diff_by_file(&String::from_utf8_lossy(&out.stdout)));
            }
        }
    }

    // Per-file: untracked files (--no-index can't be batched).
    for change in changes.iter().filter(|c| c.area == "untracked") {
        if let Ok(diff) = file_diff(repo_path, &change.path, "untracked") {
            if !diff.is_empty() {
                map.insert(change.path.clone(), diff);
            }
        }
    }

    map
}

/// Compute diffs between two git refs using `git diff <base>..<head>`.
///
/// Returns a map of `file_path → unified_diff_text` for every changed file.
pub fn committed_range_diffs(
    repo_path: &Path,
    base_ref: &str,
    head_ref: &str,
) -> Result<HashMap<String, String>> {
    let range = format!("{base_ref}..{head_ref}");
    let output = std::process::Command::new("git")
        .args(["diff", &range])
        .current_dir(repo_path)
        .output()
        .with_context(|| format!("git diff {range} failed"))?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git diff {range} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(split_diff_by_file(&String::from_utf8_lossy(&output.stdout)))
}

fn split_diff_by_file(diff_text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut current_file: Option<String> = None;
    let mut current_lines: Vec<&str> = Vec::new();

    for line in diff_text.lines() {
        if line.starts_with("diff --git ") {
            if let Some(file) = current_file.take() {
                map.insert(file, current_lines.join("\n"));
            }
            current_lines = vec![line];
            let parts: Vec<&str> = line.splitn(4, ' ').collect();
            if parts.len() == 4 {
                let b_path = parts[3];
                current_file = Some(b_path.strip_prefix("b/").unwrap_or(b_path).to_string());
            }
        } else {
            current_lines.push(line);
        }
    }
    if let Some(file) = current_file {
        map.insert(file, current_lines.join("\n"));
    }
    map
}

// ── Branch info ───────────────────────────────────────────────────────────────

/// Return branch name and ahead/behind counts relative to the upstream/default branch.
pub fn branch_info(repo_path: &Path) -> Result<BranchInfo> {
    let repo = open_repo(repo_path)?;

    // Current branch name.
    let head = repo.head().context("failed to read HEAD")?;
    let branch = match head.referent_name() {
        Some(name) => name.shorten().to_str_lossy().into_owned(),
        None => "HEAD (detached)".to_string(),
    };

    let default_branch = detect_default_branch_gix(&repo);

    let (ahead, behind) = compute_ahead_behind(&repo, &branch, &default_branch);

    Ok(BranchInfo {
        branch,
        default_branch,
        ahead,
        behind,
    })
}

/// Attempt to determine the default branch from the `origin/HEAD` symbolic ref.
/// Falls back to `"main"`.
fn detect_default_branch_gix(repo: &gix::Repository) -> String {
    // Try refs/remotes/origin/HEAD symbolic ref target
    if let Ok(r) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let gix::refs::TargetRef::Symbolic(target) = r.target() {
            let name = target.as_bstr().to_str_lossy().into_owned();
            // Strip "refs/remotes/origin/" prefix
            if let Some(short) = name.strip_prefix("refs/remotes/origin/") {
                return short.to_string();
            }
        }
    }
    // Fallback: check if "main" or "master" exist on origin
    for candidate in ["main", "master"] {
        let refname = format!("refs/remotes/origin/{candidate}");
        if repo.find_reference(refname.as_str()).is_ok() {
            return candidate.to_string();
        }
    }
    "main".to_string()
}

/// Count commits ahead/behind via `git rev-list --left-right --count`.
/// gix's rev_walk doesn't expose `with_hidden()`, so we use a subprocess
/// for this specific operation (it's not on the hot path).
fn compute_ahead_behind(repo: &gix::Repository, branch: &str, default_branch: &str) -> (i32, i32) {
    let workdir = match repo.workdir() {
        Some(d) => d,
        None => return (0, 0),
    };

    for upstream in &[
        format!("refs/remotes/origin/{branch}"),
        format!("refs/remotes/origin/{default_branch}"),
    ] {
        // Only try if the ref actually exists.
        if repo.find_reference(upstream.as_str()).is_err() {
            continue;
        }
        let range = format!("HEAD...{upstream}");
        let out = std::process::Command::new("git")
            .args(["rev-list", "--left-right", "--count", &range])
            .current_dir(workdir)
            .output();
        if let Ok(out) = out {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let parts: Vec<&str> = text.split_whitespace().collect();
                if parts.len() == 2 {
                    let behind = parts[0].parse::<i32>().unwrap_or(0);
                    let ahead = parts[1].parse::<i32>().unwrap_or(0);
                    return (ahead, behind);
                }
            }
        }
    }

    (0, 0)
}

// ── Commit log ────────────────────────────────────────────────────────────────

/// Return the last `n` commits from HEAD with metadata.
///
/// `is_pushed` is determined efficiently via merge-base: find the common ancestor
/// of HEAD and upstream, then walk HEAD — once we reach the merge-base, all
/// subsequent commits are pushed. This avoids walking the entire upstream history
/// into a HashSet (O(repo_size) → O(n)).
pub fn commit_log(repo_path: &Path, n: usize) -> Result<Vec<CommitInfo>> {
    let repo = open_repo(repo_path)?;

    let head_id = repo
        .head_id()
        .context("repository has no commits")?
        .detach();

    // Find the merge-base between HEAD and upstream to determine the pushed boundary.
    // Any commit at or beyond the merge-base (walking backwards from HEAD) is pushed.
    let merge_base_id: Option<gix::ObjectId> = (|| {
        let head = repo.head().ok()?;
        let branch = head.referent_name()?;
        let upstream_ref = format!("refs/remotes/origin/{}", branch.shorten().to_str_lossy());
        let _upstream = repo.find_reference(upstream_ref.as_str()).ok()?;

        let workdir = repo.workdir()?;
        let out = std::process::Command::new("git")
            .args(["merge-base", "HEAD", &upstream_ref])
            .current_dir(workdir)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let hex = String::from_utf8_lossy(&out.stdout).trim().to_string();
        gix::ObjectId::from_hex(hex.as_bytes()).ok()
    })();

    let mut found_merge_base = false;

    let commits: Vec<CommitInfo> = repo
        .rev_walk([head_id])
        .all()?
        .filter_map(|c| c.ok())
        .take(n)
        .filter_map(|info| {
            let commit = repo.find_object(info.id).ok()?.into_commit();
            let decoded = commit.decode().ok()?;
            let hash = info.id.to_hex().to_string();
            let message = decoded.message.to_str_lossy().into_owned();
            let (subject, body) = split_message(&message);
            let author = decoded.author().ok()?;
            let author_name = author.name.to_str_lossy().into_owned();
            let date = chrono::DateTime::from_timestamp(author.time().ok()?.seconds, 0)
                .map(|dt: chrono::DateTime<chrono::Utc>| dt.to_rfc3339())
                .unwrap_or_default();

            // Once we encounter the merge-base, this commit and everything
            // after it (older) is pushed. The merge-base itself is pushed.
            if !found_merge_base {
                if let Some(mb) = &merge_base_id {
                    if info.id == *mb {
                        found_merge_base = true;
                    }
                }
            }
            let is_pushed = found_merge_base;

            Some(CommitInfo {
                hash,
                subject: subject.to_string(),
                body: body.trim().to_string(),
                author: author_name,
                date,
                is_pushed,
            })
        })
        .collect();

    Ok(commits)
}

fn split_message(msg: &str) -> (&str, &str) {
    match msg.find('\n') {
        Some(pos) => (&msg[..pos], &msg[pos + 1..]),
        None => (msg.trim(), ""),
    }
}

// ── Run-specific helpers ──────────────────────────────────────────────────────

/// List all files relevant to a run — combining:
/// 1. Uncommitted changes in the run's worktree (staged + unstaged + untracked)
/// 2. Committed changes on the run branch vs main (ahead files)
///
/// `run_id` is the Grove run UUID; `run_worktree` is the worktree path; `project_root`
/// is the main repo root (used for branch range diffs when the worktree is clean).
pub fn run_files(
    run_worktree: &Path,
    project_root: &Path,
    run_id: &str,
) -> Result<Vec<FileChange>> {
    // 1. Uncommitted changes in the worktree.
    let uncommitted = worktree_status(run_worktree).unwrap_or_default();

    // 2. Ahead-of-remote committed changes.
    let ahead_committed = run_ahead_committed(run_worktree, project_root, run_id);

    // Merge: uncommitted takes priority over committed for the same path.
    // Collect owned Strings to avoid borrowing `uncommitted` while we move it.
    let uncommitted_paths: std::collections::HashSet<String> =
        uncommitted.iter().map(|f| f.path.clone()).collect();

    let mut result = uncommitted;
    for f in ahead_committed {
        if !uncommitted_paths.contains(&f.path) {
            result.push(f);
        }
    }

    Ok(filter_ignored_changes(run_worktree, result))
}

/// Collect files that are committed on the run branch but not yet on the remote.
fn run_ahead_committed(run_worktree: &Path, project_root: &Path, run_id: &str) -> Vec<FileChange> {
    let branch = resolve_workspace_branch(run_worktree, run_id);

    // Try @{upstream}..HEAD first (cheapest).
    if let Ok(repo) = open_repo(run_worktree) {
        if let Ok(head) = repo.head_id() {
            // Try the upstream tracking ref for the run branch directly.
            let upstream_ref = format!("refs/remotes/origin/{branch}");
            if let Ok(up_ref) = repo.find_reference(upstream_ref.as_str()) {
                let up_id = up_ref.id().detach();
                let head_id = head.detach();
                if let Ok(paths) = tree_diff_file_list(&repo, up_id, head_id) {
                    if !paths.is_empty() {
                        return paths
                            .into_iter()
                            .map(|(p, s)| FileChange {
                                status: s,
                                path: p,
                                committed: true,
                                area: "committed".to_string(),
                            })
                            .collect();
                    }
                }
            }
        }
    }

    // Fallback: project_root range diff (main...run-branch).
    if let Ok(repo) = open_repo(project_root) {
        let default_branch = detect_default_branch_gix(&repo);
        for base_ref in [
            format!("refs/remotes/origin/{default_branch}"),
            format!("refs/heads/{default_branch}"),
            "refs/remotes/origin/main".to_string(),
            "refs/heads/main".to_string(),
            "refs/remotes/origin/master".to_string(),
            "refs/heads/master".to_string(),
        ] {
            let head_ref = format!("refs/heads/{branch}");
            if let (Ok(base_id_obj), Ok(head_id_obj)) = (
                repo.find_reference(base_ref.as_str())
                    .map(|r| r.id().detach()),
                repo.find_reference(head_ref.as_str())
                    .map(|r| r.id().detach()),
            ) {
                if let Ok(paths) = tree_diff_file_list(&repo, base_id_obj, head_id_obj) {
                    if !paths.is_empty() {
                        return paths
                            .into_iter()
                            .map(|(p, s)| FileChange {
                                status: s,
                                path: p,
                                committed: true,
                                area: "committed".to_string(),
                            })
                            .collect();
                    }
                }
            }
        }
    }

    vec![]
}

/// Diff two commit OIDs and return `(path, status_char)` pairs.
///
/// Uses `git diff --name-status` subprocess: gix tree diff Platform
/// doesn't expose `track_path()`, which would be needed for path info via the
/// library API.
fn tree_diff_file_list(
    repo: &gix::Repository,
    base_commit: gix::ObjectId,
    head_commit: gix::ObjectId,
) -> Result<Vec<(String, String)>> {
    let workdir = repo.workdir().context("bare repository")?;
    let base_hex = base_commit.to_hex().to_string();
    let head_hex = head_commit.to_hex().to_string();

    let out = std::process::Command::new("git")
        .args(["diff", "--name-status", &base_hex, &head_hex])
        .current_dir(workdir)
        .output()
        .context("git diff --name-status failed")?;

    if !out.status.success() {
        return Ok(vec![]);
    }

    let mut files = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut parts = line.splitn(2, '\t');
        let status = match parts.next().map(|s| s.trim()) {
            Some("A") => "A",
            Some("D") => "D",
            Some("M") => "M",
            Some(s) if s.starts_with('R') => "R",
            _ => continue,
        };
        if let Some(path) = parts.next() {
            files.push((path.trim().to_string(), status.to_string()));
        }
    }

    Ok(files)
}

/// Compute all file diffs for a run: uncommitted files get worktree diffs;
/// committed files get tree-range diffs vs main.
///
/// Uses batched diff commands (2 subprocess calls for staged/unstaged instead of N).
pub fn all_diffs_for_run(
    run_worktree: &Path,
    project_root: &Path,
    run_id: &str,
) -> Result<HashMap<String, String>> {
    // 1. Uncommitted: batched by area (staged, unstaged, untracked).
    let mut map = all_diffs_in_worktree(run_worktree)?;

    // 2. Committed: main...run-branch tree diff (already batched via committed_range_diffs).
    let branch = resolve_workspace_branch(run_worktree, run_id);
    if let Ok(repo) = open_repo(project_root) {
        let default_branch = detect_default_branch_gix(&repo);
        for base_ref in [
            format!("refs/remotes/origin/{default_branch}"),
            format!("refs/heads/{default_branch}"),
            "refs/remotes/origin/main".to_string(),
            "refs/heads/main".to_string(),
            "refs/remotes/origin/master".to_string(),
            "refs/heads/master".to_string(),
        ] {
            if let Ok(committed_diffs) =
                committed_range_diffs(project_root, &base_ref, &format!("refs/heads/{branch}"))
            {
                for (path, diff) in committed_diffs {
                    map.entry(path).or_insert(diff);
                }
                break;
            }
        }
    }

    Ok(filter_ignored_diffs(run_worktree, map))
}

/// Combined function: returns both file list and diffs for a run in one call.
///
/// Shares the `worktree_status` scan between file listing and diff computation,
/// eliminating the redundant second gix index+worktree scan that happened when
/// `run_files` and `all_diffs_for_run` were called separately.
pub fn run_files_and_diffs(
    run_worktree: &Path,
    project_root: &Path,
    run_id: &str,
) -> Result<(Vec<FileChange>, HashMap<String, String>)> {
    // Single worktree_status scan — shared by both file list and diffs.
    let uncommitted = worktree_status(run_worktree).unwrap_or_default();

    // File list: uncommitted + ahead-committed.
    let ahead_committed = run_ahead_committed(run_worktree, project_root, run_id);
    let uncommitted_paths: std::collections::HashSet<String> =
        uncommitted.iter().map(|f| f.path.clone()).collect();
    let mut files: Vec<FileChange> = uncommitted.clone();
    for f in &ahead_committed {
        if !uncommitted_paths.contains(&f.path) {
            files.push(f.clone());
        }
    }
    let files = filter_ignored_changes(run_worktree, files);

    // Diffs: batched uncommitted + committed range.
    let mut diffs = batch_uncommitted_diffs(run_worktree, &uncommitted);
    let branch = resolve_workspace_branch(run_worktree, run_id);
    if let Ok(repo) = open_repo(project_root) {
        let default_branch = detect_default_branch_gix(&repo);
        for base_ref in [
            format!("refs/remotes/origin/{default_branch}"),
            format!("refs/heads/{default_branch}"),
            "refs/remotes/origin/main".to_string(),
            "refs/heads/main".to_string(),
            "refs/remotes/origin/master".to_string(),
            "refs/heads/master".to_string(),
        ] {
            if let Ok(committed_diffs) =
                committed_range_diffs(project_root, &base_ref, &format!("refs/heads/{branch}"))
            {
                for (path, diff) in committed_diffs {
                    diffs.entry(path).or_insert(diff);
                }
                break;
            }
        }
    }

    Ok((files, filter_ignored_diffs(run_worktree, diffs)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temp git repo with an initial commit.
    fn temp_git_repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
        };
        run(&["init", "-b", "main"]);
        run(&["config", "user.email", "test@test.com"]);
        run(&["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("README.md"), "# Test\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "initial"]);
        dir
    }

    #[test]
    fn worktree_status_clean_repo() {
        let dir = temp_git_repo();
        let changes = worktree_status(dir.path()).unwrap();
        assert!(changes.is_empty(), "clean repo should have no changes");
    }

    #[test]
    fn worktree_status_modified_file() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("README.md"), "modified\n").unwrap();

        let changes = worktree_status(dir.path()).unwrap();
        assert!(!changes.is_empty(), "should detect modification");

        let modified = changes.iter().find(|c| c.path == "README.md").unwrap();
        assert_eq!(modified.status, "M");
        assert_eq!(modified.area, "unstaged");
    }

    #[test]
    fn worktree_status_staged_file() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("new.txt"), "new file\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "new.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let changes = worktree_status(dir.path()).unwrap();
        let staged = changes
            .iter()
            .find(|c| c.path == "new.txt" && c.area == "staged");
        assert!(
            staged.is_some(),
            "should detect staged addition: {changes:?}"
        );
        assert_eq!(staged.unwrap().status, "A");
    }

    #[test]
    fn worktree_status_untracked_file() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("untracked.txt"), "hello\n").unwrap();

        let changes = worktree_status(dir.path()).unwrap();
        let untracked = changes.iter().find(|c| c.path == "untracked.txt");
        assert!(
            untracked.is_some(),
            "should detect untracked file: {changes:?}"
        );
        assert_eq!(untracked.unwrap().area, "untracked");
    }

    #[test]
    fn worktree_status_deleted_file() {
        let dir = temp_git_repo();
        std::fs::remove_file(dir.path().join("README.md")).unwrap();

        let changes = worktree_status(dir.path()).unwrap();
        let deleted = changes.iter().find(|c| c.path == "README.md");
        assert!(deleted.is_some(), "should detect deleted file: {changes:?}");
        assert_eq!(deleted.unwrap().status, "D");
    }

    #[test]
    fn branch_info_on_main() {
        let dir = temp_git_repo();
        let info = branch_info(dir.path()).unwrap();
        assert_eq!(info.branch, "main");
        assert_eq!(info.ahead, 0);
        assert_eq!(info.behind, 0);
    }

    #[test]
    fn commit_log_returns_initial_commit() {
        let dir = temp_git_repo();
        let log = commit_log(dir.path(), 10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].subject, "initial");
        assert_eq!(log[0].author, "Test");
        assert!(!log[0].hash.is_empty());
    }

    #[test]
    fn commit_log_respects_limit() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("second.txt"), "two\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "second commit"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let log = commit_log(dir.path(), 1).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].subject, "second commit");
    }

    #[test]
    fn file_diff_unstaged_modification() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("README.md"), "changed content\n").unwrap();

        let diff = file_diff(dir.path(), "README.md", "unstaged").unwrap();
        assert!(!diff.is_empty(), "diff should not be empty");
        assert!(
            diff.contains("changed content"),
            "diff should show the change"
        );
    }

    #[test]
    fn file_diff_staged_addition() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("new.txt"), "new content\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "new.txt"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        let diff = file_diff(dir.path(), "new.txt", "staged").unwrap();
        assert!(!diff.is_empty(), "staged diff should not be empty");
        assert!(
            diff.contains("new content"),
            "diff should show the addition"
        );
    }

    #[test]
    fn file_diff_untracked_file() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("brand_new.txt"), "brand new\n").unwrap();

        let diff = file_diff(dir.path(), "brand_new.txt", "untracked").unwrap();
        assert!(!diff.is_empty(), "untracked diff should not be empty");
        assert!(
            diff.contains("brand new"),
            "diff should show the file content"
        );
    }

    #[test]
    fn all_diffs_in_worktree_returns_all_changes() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("README.md"), "modified\n").unwrap();
        std::fs::write(dir.path().join("untracked.txt"), "new\n").unwrap();

        let diffs = all_diffs_in_worktree(dir.path()).unwrap();
        assert!(
            diffs.contains_key("README.md"),
            "should have diff for modified file"
        );
        assert!(
            diffs.contains_key("untracked.txt"),
            "should have diff for untracked file"
        );
    }

    #[test]
    fn run_files_and_diffs_supports_conversation_branch() {
        let dir = temp_git_repo();
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .output()
                .unwrap()
        };

        run(&["checkout", "-b", "grove/s_conv_test"]);
        std::fs::write(dir.path().join("README.md"), "conversation branch change\n").unwrap();
        run(&["add", "README.md"]);
        run(&["commit", "-m", "conversation change"]);

        let files = run_files(dir.path(), dir.path(), "run_test").unwrap();
        let readme = files.iter().find(|c| c.path == "README.md" && c.committed);
        assert!(
            readme.is_some(),
            "expected committed README diff on conversation branch: {files:?}"
        );

        let (files2, diffs) = run_files_and_diffs(dir.path(), dir.path(), "run_test").unwrap();
        assert!(
            files2.iter().any(|c| c.path == "README.md" && c.committed),
            "expected committed README in combined file list: {files2:?}"
        );
        assert!(
            diffs
                .get("README.md")
                .is_some_and(|d| d.contains("conversation branch change")),
            "expected committed diff for README on conversation branch: {diffs:?}"
        );
    }
}
