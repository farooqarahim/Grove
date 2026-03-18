use std::path::Path;
use std::process::Command;

use crate::errors::{GroveError, GroveResult};

/// Return `true` if `path` is inside a git repository (`.git` dir/file exists).
pub fn is_git_repo(path: &Path) -> bool {
    // Walk up looking for .git — mirrors how git itself discovers the repo root.
    let mut dir = path;
    loop {
        if dir.join(".git").exists() {
            return true;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return false,
        }
    }
}

/// Initialise a new git repository at `path`.
pub fn git_init(path: &Path) -> GroveResult<()> {
    run_git(path, &["init", "-b", "main"])
}

/// Return the name of the current branch in `repo_root`.
pub fn git_current_branch(repo_root: &Path) -> GroveResult<String> {
    let out = git_cmd(repo_root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(GroveError::Runtime(
            "git rev-parse --abbrev-ref HEAD failed".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Return `true` if `branch` exists in `repo_root`.
pub fn git_branch_exists(repo_root: &Path, branch: &str) -> GroveResult<bool> {
    let out = git_cmd(repo_root)
        .args(["branch", "--list", branch])
        .output()?;
    Ok(!String::from_utf8_lossy(&out.stdout).trim().is_empty())
}

/// Create a new branch at `start_point` without checking it out.
///
/// Used to create `grove/s_<id>` branches at conversation start.
/// No-ops if the branch already exists (returns Ok).
pub fn git_create_branch(repo_root: &Path, branch: &str, start_point: &str) -> GroveResult<()> {
    if git_branch_exists(repo_root, branch)? {
        return Ok(());
    }
    run_git(repo_root, &["branch", branch, start_point])
}

/// Checkout an existing branch in a worktree.
///
/// Used to switch a worktree to a different branch.
pub fn git_checkout_branch(worktree_path: &Path, branch: &str) -> GroveResult<()> {
    run_git(worktree_path, &["checkout", branch])
}

/// Run `git gc --auto` to let git clean its object store if needed.
pub fn git_gc_auto(repo_root: &Path) -> GroveResult<()> {
    run_git(repo_root, &["gc", "--auto"])
}

/// Add a git worktree at `worktree_path` on a new branch `branch`.
/// The branch is created from `HEAD`.
pub fn git_worktree_add(repo_root: &Path, worktree_path: &Path, branch: &str) -> GroveResult<()> {
    let path_str = worktree_path.to_string_lossy().to_string();
    run_git(repo_root, &["worktree", "add", "-b", branch, &path_str])
}

/// Add a detached git worktree at `worktree_path` (no branch, HEAD detached).
/// Used when a detached-HEAD worktree is needed (the caller checks out later).
pub fn git_worktree_add_detached(repo_root: &Path, worktree_path: &Path) -> GroveResult<()> {
    let path_str = worktree_path.to_string_lossy().to_string();
    run_git(repo_root, &["worktree", "add", "--detach", &path_str])
}

/// Add a detached git worktree at `worktree_path` starting from `start`.
///
/// Useful for temporary merge/review worktrees where the caller wants the tree
/// contents of a specific branch or commit without checking out that branch.
pub fn git_worktree_add_detached_at(
    repo_root: &Path,
    worktree_path: &Path,
    start: &str,
) -> GroveResult<()> {
    let path_str = worktree_path.to_string_lossy().to_string();
    run_git(
        repo_root,
        &["worktree", "add", "--detach", &path_str, start],
    )
}

/// Remove a git worktree at `worktree_path`.
pub fn git_worktree_remove(repo_root: &Path, worktree_path: &Path) -> GroveResult<()> {
    let path_str = worktree_path.to_string_lossy().to_string();
    run_git(repo_root, &["worktree", "remove", "--force", &path_str])
}

/// Prune stale worktree administrative files from `.git/worktrees/`.
pub fn git_worktree_prune(repo_root: &Path) -> GroveResult<()> {
    run_git(repo_root, &["worktree", "prune"])
}

/// Merge `branch` into the current branch in `repo_root` using `--no-ff`.
/// Returns the merged branch name on success.
pub fn git_merge(repo_root: &Path, branch: &str) -> GroveResult<String> {
    run_git(repo_root, &["merge", "--no-ff", branch])?;
    Ok(branch.to_string())
}

/// Delete a local branch unconditionally.
///
/// Uses `-D` (force) because Grove branches are ephemeral orchestration branches
/// that may not have been merged (e.g., after a conflict abort).
pub fn git_delete_branch(repo_root: &Path, branch: &str) -> GroveResult<()> {
    run_git(repo_root, &["branch", "-D", branch])
}

/// Stage all changes in the working directory.
pub fn git_add_all(cwd: &Path) -> GroveResult<()> {
    run_git(cwd, &["add", "-A"])
}

/// Commit staged changes. Uses `--allow-empty` so it never errors on clean worktrees.
pub fn git_commit(cwd: &Path, message: &str) -> GroveResult<()> {
    run_git(cwd, &["commit", "-m", message, "--allow-empty"])
}

/// Create a worktree on a new branch starting from a specific commit/branch.
///
/// Unlike `git_worktree_add` (which always branches from HEAD), this branches
/// from `start_point`. Used to chain sequential agents: each agent branches
/// from the previous agent's committed branch tip.
pub fn git_worktree_add_from(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
    start_point: &str,
) -> GroveResult<()> {
    let path_str = worktree_path.to_string_lossy().to_string();
    run_git(
        repo_root,
        &["worktree", "add", "-b", branch, &path_str, start_point],
    )
}

/// Return `true` if the repo has at least one commit (HEAD is resolvable).
/// Returns `false` for freshly `git init`'d repos with no commits.
pub fn has_commits(repo_root: &Path) -> bool {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// List local branches matching a glob pattern (e.g., `"grove/*"`).
///
/// Returns branch names with leading whitespace and `*` (current-branch marker) stripped.
pub fn git_list_branches(repo_root: &Path, pattern: &str) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["branch", "--list", pattern])
        .current_dir(repo_root)
        .output();
    let Ok(out) = output else { return vec![] };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| line.trim().trim_start_matches("* ").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// ── Change detection ─────────────────────────────────────────────────────────

/// Status of a file change between two git states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeStatus {
    Added,
    Modified,
    Deleted,
}

/// Returns `(status, relative_path)` pairs for files changed between two commits.
///
/// Uses `git diff-tree` which is extremely fast — it compares tree objects
/// without touching the working directory. This is the primary fast path.
pub fn git_diff_names(
    repo_root: &Path,
    base_ref: &str,
    head_ref: &str,
) -> GroveResult<Vec<(FileChangeStatus, String)>> {
    let out = Command::new("git")
        .args([
            "diff-tree",
            "-r",
            "--name-status",
            "--no-commit-id",
            "-z",
            base_ref,
            head_ref,
        ])
        .current_dir(repo_root)
        .output()?;
    if !out.status.success() {
        return Err(GroveError::Runtime(format!(
            "git diff-tree failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    parse_diff_name_status_nul(&out.stdout)
}

/// Returns changed files in the working tree relative to HEAD.
///
/// Catches uncommitted agent work when `commit_agent_work()` failed.
/// Also detects untracked files via `git ls-files --others`.
pub fn git_diff_working_tree(worktree_path: &Path) -> GroveResult<Vec<(FileChangeStatus, String)>> {
    let out = git_cmd(worktree_path)
        .args(["diff", "--name-status", "-z", "HEAD"])
        .output()?;
    if !out.status.success() {
        return Err(GroveError::Runtime(format!(
            "git diff HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }

    let mut results = parse_diff_name_status_nul(&out.stdout)?;

    // Also check for untracked files (new files that were never git-added)
    let untracked = git_cmd(worktree_path)
        .args(["ls-files", "--others", "--exclude-standard", "-z"])
        .output()?;
    if untracked.status.success() {
        for path in untracked.stdout.split(|&b| b == 0) {
            let s = String::from_utf8_lossy(path);
            let s = s.trim();
            if !s.is_empty() {
                results.push((FileChangeStatus::Added, s.to_string()));
            }
        }
    }

    Ok(results)
}

/// Returns `true` if the current branch has commits that are not in `base_branch`.
///
/// Checks `origin/{base_branch}` first (most precise), then falls back to the
/// local `base_branch`. Used in the publish flow to detect when
/// `commit_agent_work()` has already committed agent changes without a run_id
/// trailer, so those commits should be pushed rather than skipping as
/// "no changes".
pub fn branch_has_local_commits(cwd: &Path, base_branch: &str) -> bool {
    for reference in [format!("origin/{base_branch}"), base_branch.to_string()] {
        let out = git_cmd(cwd)
            .args(["rev-list", "--count", &format!("{reference}..HEAD")])
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let count: u32 = String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse()
                    .unwrap_or(0);
                return count > 0;
            }
        }
    }
    false
}

/// Parse NUL-delimited `--name-status -z` output from git diff/diff-tree.
///
/// Format: `<status>\0<path>\0` for each entry. Renames appear as
/// `R<score>\0<old_path>\0<new_path>\0` — we split them into delete + add.
fn parse_diff_name_status_nul(output: &[u8]) -> GroveResult<Vec<(FileChangeStatus, String)>> {
    let text = String::from_utf8_lossy(output);
    let mut parts = text.split('\0').peekable();
    let mut results = Vec::new();

    while let Some(status_part) = parts.next() {
        let status_part = status_part.trim();
        if status_part.is_empty() {
            continue;
        }

        let status_char = &status_part[..1];
        let path = match parts.next() {
            Some(p) => p.to_string(),
            None => break,
        };
        if path.is_empty() {
            continue;
        }

        match status_char {
            "A" => results.push((FileChangeStatus::Added, path)),
            "M" => results.push((FileChangeStatus::Modified, path)),
            "D" => results.push((FileChangeStatus::Deleted, path)),
            "R" | "C" => {
                // Rename/Copy: next field is the new path.
                // Split into delete(old) + add(new).
                let new_path = match parts.next() {
                    Some(p) if !p.is_empty() => p.to_string(),
                    _ => continue,
                };
                results.push((FileChangeStatus::Deleted, path));
                results.push((FileChangeStatus::Added, new_path));
            }
            _ => {
                // Unknown status (T for type change, etc.) — treat as modified
                results.push((FileChangeStatus::Modified, path));
            }
        }
    }
    Ok(results)
}

// ── 3-way file merge ─────────────────────────────────────────────────────────

/// Result of a 3-way merge on a single file via `git merge-file`.
#[derive(Debug)]
pub enum MergeFileResult {
    /// No conflicts — merged content is ready to write.
    Clean(Vec<u8>),
    /// Conflicts found — content has standard conflict markers,
    /// `conflict_count` is the number of conflict regions.
    Conflict {
        merged_with_markers: Vec<u8>,
        conflict_count: usize,
    },
}

/// Perform a 3-way merge of a single file using `git merge-file`.
///
/// Takes three file paths on disk:
/// - `ours`: the version already in the merge destination (previous agent's changes)
/// - `base`: the common ancestor (original version before any agent touched it)
/// - `theirs`: the current agent's version
///
/// Uses `-p` to write merged output to stdout (does not mutate any input file).
/// Uses `--diff3` to include the base version in conflict markers for clarity.
///
/// Returns `MergeFileResult::Clean` if the merge succeeded without conflicts,
/// or `MergeFileResult::Conflict` with the number of conflict regions.
///
/// Returns `Err` only on hard failures (git not found, I/O error), never on
/// merge conflicts — those are returned as `Ok(Conflict { .. })`.
pub fn git_merge_file(ours: &Path, base: &Path, theirs: &Path) -> GroveResult<MergeFileResult> {
    let ours_str = ours.to_string_lossy();
    let base_str = base.to_string_lossy();
    let theirs_str = theirs.to_string_lossy();

    let output = Command::new("git")
        .args([
            "merge-file",
            "-p",      // write merged result to stdout
            "--diff3", // show base content in conflict markers
            &ours_str,
            &base_str,
            &theirs_str,
        ])
        .output()
        .map_err(|e| GroveError::Runtime(format!("git merge-file exec: {e}")))?;

    let exit_code = output.status.code().unwrap_or(-1);

    match exit_code {
        0 => Ok(MergeFileResult::Clean(output.stdout)),
        n if n > 0 => {
            // Positive exit code = number of conflict regions
            Ok(MergeFileResult::Conflict {
                merged_with_markers: output.stdout,
                conflict_count: n as usize,
            })
        }
        _ => Err(GroveError::Runtime(format!(
            "git merge-file failed with exit code {exit_code}: {}",
            String::from_utf8_lossy(&output.stderr)
        ))),
    }
}

// ── Sparse checkout ─────────────────────────────────────────────────────────

/// Returns `true` if the repo at `repo_root` uses git submodules.
pub fn has_submodules(repo_root: &Path) -> bool {
    repo_root.join(".gitmodules").exists()
}

/// Initialise sparse checkout in `--no-cone` mode.
///
/// `--no-cone` supports both directory patterns (`src/`) and file patterns
/// (`Cargo.toml`), unlike `--cone` which only supports directories.
pub fn git_sparse_checkout_init(worktree_path: &Path) -> GroveResult<()> {
    run_git(worktree_path, &["sparse-checkout", "init", "--no-cone"])
}

/// Set the sparse checkout patterns for a worktree.
///
/// Replaces all existing patterns. Each pattern follows gitignore syntax.
pub fn git_sparse_checkout_set(worktree_path: &Path, patterns: &[&str]) -> GroveResult<()> {
    let mut args = vec!["sparse-checkout", "set"];
    args.extend(patterns);
    run_git(worktree_path, &args)
}

/// Disable sparse checkout and restore the full working tree.
pub fn git_sparse_checkout_disable(worktree_path: &Path) -> GroveResult<()> {
    run_git(worktree_path, &["sparse-checkout", "disable"])
}

/// List the patterns currently configured for sparse checkout.
///
/// Returns the raw pattern strings from `git sparse-checkout list`.
/// Returns an empty vec if sparse checkout is not enabled.
pub fn git_sparse_checkout_list(worktree_path: &Path) -> GroveResult<Vec<String>> {
    let out = Command::new("git")
        .args(["sparse-checkout", "list"])
        .current_dir(worktree_path)
        .output()?;
    if !out.status.success() {
        // Not a fatal error — sparse checkout may not be enabled.
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Check if a file path matches any of the sparse checkout patterns.
///
/// Uses a simple prefix/glob match: directory patterns (ending with `/`)
/// match any file under that directory; file patterns must match exactly.
pub fn matches_sparse_patterns(rel_path: &str, patterns: &[String]) -> bool {
    for pat in patterns {
        if pat.ends_with('/') {
            // Directory pattern: file must be under this directory.
            if rel_path.starts_with(pat) || rel_path.starts_with(pat.trim_end_matches('/')) {
                return true;
            }
            // Also handle pattern without trailing slash (e.g., "src" matches "src/lib.rs").
            let dir_prefix = pat.trim_end_matches('/');
            if rel_path == dir_prefix || rel_path.starts_with(&format!("{dir_prefix}/")) {
                return true;
            }
        } else {
            // File pattern: exact match or glob-like prefix.
            if rel_path == pat {
                return true;
            }
            // Support wildcard patterns like "*.toml".
            if pat.contains('*') {
                let prefix = pat.split('*').next().unwrap_or("");
                let suffix = pat.split('*').last().unwrap_or("");
                if rel_path.starts_with(prefix) && rel_path.ends_with(suffix) {
                    return true;
                }
            }
        }
    }
    false
}

/// Return the full commit SHA that HEAD currently points to.
///
/// Used to record the base commit at worktree creation time so the merge layer
/// can identify the common ancestor without an extra `git merge-base` call.
pub fn git_rev_parse_head(repo_root: &Path) -> GroveResult<String> {
    git_rev_parse(repo_root, "HEAD")
}

/// Return the full commit SHA that `revspec` resolves to.
pub fn git_rev_parse(repo_root: &Path, revspec: &str) -> GroveResult<String> {
    let out = git_cmd(repo_root).args(["rev-parse", revspec]).output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(GroveError::Runtime(format!(
            "git rev-parse {revspec} failed: {stderr}"
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// List all files changed since `since_ref`: committed changes, staged/unstaged
/// working-tree changes, and untracked files.
///
/// This covers three areas to prevent scope bypasses:
/// 1. `git diff --name-only <since_ref>..HEAD` — files committed since the snapshot
/// 2. `git diff --name-only HEAD` — staged + unstaged working-tree changes
/// 3. `git ls-files --others --exclude-standard` — new untracked files
///
/// If `since_ref` is `None`, only areas 2 and 3 are checked.
pub fn changed_files_since(worktree: &Path, since_ref: Option<&str>) -> GroveResult<Vec<String>> {
    let mut files: Vec<String> = Vec::new();

    // 1. Committed changes since the snapshot ref (if we have one)
    if let Some(r) = since_ref {
        let committed = std::process::Command::new("git")
            .args(["diff", "--name-only", &format!("{r}..HEAD")])
            .current_dir(worktree)
            .output()
            .map_err(|e| GroveError::Runtime(format!("git diff {r}..HEAD failed: {e}")))?;
        let stdout = String::from_utf8_lossy(&committed.stdout);
        files.extend(stdout.lines().filter(|l| !l.is_empty()).map(String::from));
    }

    // 2. Working-tree changes (staged + unstaged) vs HEAD
    let working = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(worktree)
        .output()
        .map_err(|e| GroveError::Runtime(format!("git diff HEAD failed: {e}")))?;
    let working_stdout = String::from_utf8_lossy(&working.stdout);
    files.extend(
        working_stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from),
    );

    // 3. Untracked files
    let untracked = std::process::Command::new("git")
        .args(["ls-files", "--others", "--exclude-standard"])
        .current_dir(worktree)
        .output()
        .map_err(|e| GroveError::Runtime(format!("git ls-files failed: {e}")))?;
    let untracked_stdout = String::from_utf8_lossy(&untracked.stdout);
    files.extend(
        untracked_stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from),
    );

    // Deduplicate
    files.sort();
    files.dedup();
    Ok(files)
}

// ── Worktree state management ────────────────────────────────────────────────

/// Reset the worktree to a clean state between sequential agent handoffs.
///
/// Order matters: `checkout .` first restores tracked files to their committed
/// state (undoes modifications/deletions), then `clean -fd` removes untracked
/// files. Reversing the order risks removing an untracked file before checkout
/// can notice the tracked file it replaced is missing.
pub fn git_clean_worktree(worktree_path: &Path) -> GroveResult<()> {
    // `git checkout .` fails with "pathspec '.' did not match any file(s) known
    // to git" on repos that have commits but no tracked files (e.g. right after
    // `git commit --allow-empty`). This is not an error — the working tree is
    // already clean w.r.t. tracked files. Swallow only that specific message.
    if let Err(e) = run_git(worktree_path, &["checkout", "."]) {
        let msg = e.to_string();
        if !msg.contains("pathspec '.' did not match any file(s)") {
            return Err(e);
        }
    }
    run_git(worktree_path, &["clean", "-fd"])
}

/// Like `git_clean_worktree` but verifies the result via `git status --porcelain`.
///
/// If stale files remain (e.g. locked by another process), escalates to
/// `checkout --force .` + `clean -fdx` and logs a warning. Returns the list
/// of stale files that were detected (empty on clean first pass).
pub fn git_clean_worktree_verified(worktree_path: &Path) -> GroveResult<Vec<String>> {
    git_clean_worktree(worktree_path)?;

    let status_output = git_status_porcelain(worktree_path)?;
    if status_output.is_empty() {
        return Ok(Vec::new());
    }

    // Parse stale file names for logging.
    let stale_files: Vec<String> = status_output
        .lines()
        .map(|line| line.get(3..).unwrap_or(line).to_string())
        .collect();

    tracing::warn!(
        stale_count = stale_files.len(),
        files = %stale_files.join(", "),
        "git clean left stale files — escalating to force clean"
    );

    // Escalate: --force checkout + clean -fdx (removes gitignored files too).
    run_git(worktree_path, &["checkout", "--force", "."])?;
    run_git(worktree_path, &["clean", "-fdx"])?;

    Ok(stale_files)
}

/// Run `git status --porcelain` and return the raw output.
///
/// Empty string means the worktree is clean.
pub fn git_status_porcelain(worktree_path: &Path) -> GroveResult<String> {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(worktree_path)
        .output()?;
    if !out.status.success() {
        return Err(GroveError::Runtime(
            "git status --porcelain failed".to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Hard-reset the worktree to a specific commit SHA.
///
/// Used for rollback on agent failure: resets to the last good checkpoint
/// so the next retry or the next agent starts from a known-good state.
pub fn git_reset_hard(worktree_path: &Path, commit_sha: &str) -> GroveResult<()> {
    run_git(worktree_path, &["reset", "--hard", commit_sha])
}

// ── Fetch / tracking-branch helpers ──────────────────────────────────────────

/// Fetch a specific branch from a remote.
///
/// Used to ensure the local repo has the latest commits before creating a run
/// worktree. Returns `Ok(())` on success or if the remote doesn't exist
/// (local-only repos). On network failure, callers should log a warning and
/// proceed — never block a run on a fetch failure.
pub fn git_fetch_branch(repo_root: &Path, remote: &str, branch: &str) -> GroveResult<()> {
    let out = git_cmd(repo_root)
        .args(["fetch", remote, branch])
        .output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(GroveError::Runtime(format!(
            "git fetch {remote} {branch} failed: {stderr}"
        )));
    }
    Ok(())
}

/// Return `(remote, branch)` for the upstream tracking branch of HEAD.
///
/// Uses `git rev-parse --abbrev-ref --symbolic-full-name @{u}`. Returns `None`
/// if HEAD has no upstream (local-only branches, detached HEAD, etc.).
pub fn git_resolve_tracking_branch(repo_root: &Path) -> Option<(String, String)> {
    let out = git_cmd(repo_root)
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let full = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // full = "origin/main" or "upstream/feature-branch"
    let (remote, branch) = full.split_once('/')?;
    Some((remote.to_string(), branch.to_string()))
}

/// Return the set of paths registered as linked worktrees for `repo_root`.
///
/// Parses `git worktree list --porcelain`. The first entry (main worktree) is
/// excluded. Returns an empty set on any failure (safe default).
pub fn git_list_linked_worktrees(
    repo_root: &Path,
) -> std::collections::HashSet<std::path::PathBuf> {
    let out = match Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
    {
        Ok(o) => o,
        Err(_) => return std::collections::HashSet::new(),
    };
    if !out.status.success() {
        return std::collections::HashSet::new();
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut paths = std::collections::HashSet::new();
    let mut is_first = true;

    for block in text.split("\n\n") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        if is_first {
            is_first = false;
            continue; // skip main worktree
        }
        for line in block.lines() {
            if let Some(path_str) = line.strip_prefix("worktree ") {
                paths.insert(std::path::PathBuf::from(path_str.trim()));
                break;
            }
        }
    }
    paths
}

/// Rename a local branch.
pub fn git_branch_rename(repo_root: &Path, old_name: &str, new_name: &str) -> GroveResult<()> {
    run_git(repo_root, &["branch", "-m", old_name, new_name])
}

/// Push a local branch to `origin`.
///
/// Used by the GitHub promotion strategy to upload a run branch before
/// creating a pull request. Returns `Err` on network failure or if the
/// remote refuses the push (e.g. protected branch).
pub fn git_push_branch(repo_root: &Path, branch: &str) -> GroveResult<()> {
    run_git(repo_root, &["push", "origin", branch])
}

/// Delete a remote tracking branch (opt-in remote cleanup).
///
/// Returns `Ok(())` if the branch doesn't exist on the remote. Never propagates
/// network errors — callers should log at warn level and continue.
pub fn git_push_delete_branch(repo_root: &Path, remote: &str, branch: &str) -> GroveResult<()> {
    let out = Command::new("git")
        .args(["push", remote, "--delete", branch])
        .current_dir(repo_root)
        .output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        // "remote ref does not exist" is not an error condition.
        if stderr.contains("remote ref does not exist")
            || stderr.contains("error: unable to delete")
        {
            return Ok(());
        }
        return Err(GroveError::Runtime(format!(
            "git push {remote} --delete {branch} failed: {stderr}"
        )));
    }
    Ok(())
}

// ── Pre-flight conflict prediction ───────────────────────────────────────────

/// Predict which files are likely to conflict when merging `branch` into HEAD.
///
/// Returns the intersection of files changed on both sides relative to the
/// common ancestor (`git merge-base HEAD <branch>`). An empty Vec means the
/// merge is unlikely to conflict — though git may still find conflicts at
/// merge time that this heuristic misses.
///
/// Fail-open: returns an empty Vec if git is unavailable or the merge-base
/// cannot be computed (e.g. unrelated histories, no common ancestor).
pub fn git_preflight_conflict_check(repo_root: &Path, branch: &str) -> Vec<String> {
    let base_sha = match run_git_output(repo_root, &["merge-base", "HEAD", branch]) {
        Some(s) => s,
        None => return vec![],
    };
    let head_files = diff_name_only(repo_root, &base_sha, "HEAD");
    let branch_files = diff_name_only(repo_root, &base_sha, branch);
    let branch_set: std::collections::HashSet<_> = branch_files.into_iter().collect();
    head_files
        .into_iter()
        .filter(|f| branch_set.contains(f))
        .collect()
}

// ── Stale base detection ──────────────────────────────────────────────────────

/// How far a conversation branch has fallen behind its upstream.
#[derive(Debug, Clone)]
pub struct StaleBranchInfo {
    /// Common ancestor SHA of the branch and `upstream`.
    pub merge_base: String,
    /// Current HEAD SHA of `upstream`.
    pub upstream_head: String,
    /// Number of commits in `upstream` not yet in the branch.
    pub commits_behind: usize,
}

/// Detect whether `branch` has fallen behind `upstream` (e.g. `main`).
///
/// Returns `Some(StaleBranchInfo)` when the branch is stale, `None` when it
/// is already up-to-date or when git is unavailable / the branch doesn't
/// exist yet.
pub fn git_detect_stale_base(
    repo_root: &Path,
    branch: &str,
    upstream: &str,
) -> Option<StaleBranchInfo> {
    let upstream_head = run_git_output(repo_root, &["rev-parse", upstream])?;
    let merge_base = run_git_output(repo_root, &["merge-base", branch, upstream])?;

    if merge_base == upstream_head {
        return None; // branch is up-to-date
    }

    let spec = format!("{merge_base}..{upstream}");
    let commits_behind = run_git_output(repo_root, &["rev-list", "--count", &spec])
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);

    Some(StaleBranchInfo {
        merge_base,
        upstream_head,
        commits_behind,
    })
}

// ── Rebase ────────────────────────────────────────────────────────────────────

/// Outcome of `git_rebase`.
#[derive(Debug)]
pub enum RebaseOutcome {
    /// Rebase completed cleanly.
    Success,
    /// Rebase hit conflicts; the rebase has been aborted and the branch is unchanged.
    Conflict { conflicting_files: Vec<String> },
}

/// Rebase HEAD of `repo_root` onto `upstream`.
///
/// On conflict, the rebase is aborted automatically so the branch is left in
/// its pre-rebase state. Never leaves the repository mid-rebase.
pub fn git_rebase(repo_root: &Path, upstream: &str) -> GroveResult<RebaseOutcome> {
    let out = Command::new("git")
        .args(["rebase", upstream])
        .current_dir(repo_root)
        .output()?;

    if out.status.success() {
        return Ok(RebaseOutcome::Success);
    }

    // Collect conflicting files before aborting.
    let conflict_files: Vec<String> = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_root)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();

    let _ = Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(repo_root)
        .output();

    if conflict_files.is_empty() {
        // Rebase failed for a non-conflict reason (e.g., diverged history,
        // missing upstream, or the rebase machinery itself errored). Surface
        // the actual git error instead of a misleading "0 file(s)" conflict.
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let msg = if !stderr.is_empty() { stderr } else { stdout };
        return Err(GroveError::Runtime(format!("git rebase failed: {msg}")));
    }

    Ok(RebaseOutcome::Conflict {
        conflicting_files: conflict_files,
    })
}

// ── Merge ─────────────────────────────────────────────────────────────────────

/// Outcome of `git_merge_upstream_into`.
#[derive(Debug)]
pub enum MergeUpstreamOutcome {
    /// The branch already includes everything from upstream.
    UpToDate,
    /// Merge completed cleanly — a merge commit was created.
    Success { merge_commit_sha: String },
    /// Merge has conflicts. The merge is left in-progress (NOT aborted) so a
    /// resolver can edit the conflicted files and call `git_merge_continue`.
    Conflict { conflicting_files: Vec<String> },
}

/// Merge `upstream` (e.g. `"origin/main"`) INTO the branch currently checked
/// out in `worktree_path`. On conflict, the merge is left in-progress — the
/// caller is responsible for resolving or aborting.
pub fn git_merge_upstream_into(
    worktree_path: &Path,
    upstream: &str,
    commit_message: &str,
) -> GroveResult<MergeUpstreamOutcome> {
    // Fast check: is upstream already an ancestor of HEAD?
    let ancestor_check = Command::new("git")
        .args(["merge-base", "--is-ancestor", upstream, "HEAD"])
        .current_dir(worktree_path)
        .output()?;
    if ancestor_check.status.success() {
        return Ok(MergeUpstreamOutcome::UpToDate);
    }

    // Attempt the merge.
    let out = Command::new("git")
        .args(["merge", "--no-ff", upstream, "-m", commit_message])
        .current_dir(worktree_path)
        .output()?;

    if out.status.success() {
        let sha = git_rev_parse_head(worktree_path)?;
        return Ok(MergeUpstreamOutcome::Success {
            merge_commit_sha: sha,
        });
    }

    // Merge failed — check for conflicts. Do NOT abort; leave markers for resolver.
    let conflict_files: Vec<String> = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(worktree_path)
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();

    if conflict_files.is_empty() {
        // Non-conflict failure (e.g. unrelated histories, missing ref).
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let msg = if !stderr.is_empty() { stderr } else { stdout };
        // Abort anything left in-progress.
        let _ = Command::new("git")
            .args(["merge", "--abort"])
            .current_dir(worktree_path)
            .output();
        return Err(GroveError::Runtime(format!("git merge failed: {msg}")));
    }

    Ok(MergeUpstreamOutcome::Conflict {
        conflicting_files: conflict_files,
    })
}

/// Finalize an in-progress merge after all conflicts have been resolved.
///
/// Validates that no conflict markers remain in the worktree before
/// committing. Returns the merge commit SHA on success.
pub fn git_merge_continue(worktree_path: &Path) -> GroveResult<String> {
    // Validate: no conflict markers remain.
    let marker_check = Command::new("grep")
        .args([
            "-rn",
            r#"<<<<<<< \|=======$\|>>>>>>> "#,
            ".",
            "--include=*.rs",
            "--include=*.ts",
            "--include=*.tsx",
            "--include=*.js",
            "--include=*.jsx",
            "--include=*.py",
            "--include=*.toml",
            "--include=*.json",
            "--include=*.yaml",
            "--include=*.yml",
            "--include=*.css",
            "--include=*.html",
            "--include=*.md",
        ])
        .current_dir(worktree_path)
        .output();

    if let Ok(ref out) = marker_check {
        if out.status.success() {
            // grep found matches — conflict markers still present
            let markers = String::from_utf8_lossy(&out.stdout);
            let first_lines: String = markers.lines().take(10).collect::<Vec<_>>().join("\n");
            return Err(GroveError::Runtime(format!(
                "conflict markers still present in worktree:\n{first_lines}"
            )));
        }
    }

    // Stage all changes and finalize the merge commit.
    run_git(worktree_path, &["add", "-A"])?;

    let out = Command::new("git")
        .args(["-c", "core.editor=true", "commit", "--no-edit"])
        .current_dir(worktree_path)
        .output()?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(GroveError::Runtime(format!(
            "git merge commit failed: {stderr}"
        )));
    }

    git_rev_parse_head(worktree_path)
}

/// Abort an in-progress merge, restoring the branch to its pre-merge state.
pub fn git_merge_abort(worktree_path: &Path) -> GroveResult<()> {
    run_git(worktree_path, &["merge", "--abort"])
}

// ── Pre-publish pull ──────────────────────────────────────────────────────────

/// Outcome of pulling the remote conversation branch before publish.
#[derive(Debug)]
pub enum PullOutcome {
    /// Remote branch doesn't exist — first push, nothing to pull.
    NoRemote,
    /// Local already includes everything from remote.
    UpToDate,
    /// Fast-forward or merge succeeded.
    Merged { merge_commit_sha: String },
    /// Merge has conflicts — left in-progress for resolver.
    Conflict { conflicting_files: Vec<String> },
}

/// Fetch the remote conversation branch and merge it into the local worktree.
///
/// Ensures the local branch includes all remote commits so the subsequent
/// push is guaranteed to be fast-forward. On conflict, the merge is left
/// in-progress for a resolver agent.
pub fn git_pull_conv_branch(
    worktree_path: &Path,
    remote: &str,
    branch: &str,
) -> GroveResult<PullOutcome> {
    // 1. Check if remote branch exists.
    if !git_remote_branch_exists(worktree_path, remote, branch) {
        return Ok(PullOutcome::NoRemote);
    }

    // 2. Fetch latest from remote.
    git_fetch_branch(worktree_path, remote, branch)?;

    // 3. Check if local already includes remote.
    let remote_ref = format!("{remote}/{branch}");
    let ancestor_check = Command::new("git")
        .args(["merge-base", "--is-ancestor", &remote_ref, "HEAD"])
        .current_dir(worktree_path)
        .output()?;
    if ancestor_check.status.success() {
        return Ok(PullOutcome::UpToDate);
    }

    // 4. Attempt merge.
    let merge_msg = format!("grove: integrate remote conv branch ({remote}/{branch})");
    let out = Command::new("git")
        .args(["merge", "--no-ff", &remote_ref, "-m", &merge_msg])
        .current_dir(worktree_path)
        .output()?;

    if out.status.success() {
        let sha = git_rev_parse_head(worktree_path)?;
        return Ok(PullOutcome::Merged {
            merge_commit_sha: sha,
        });
    }

    // 5. Merge failed — collect conflict files. Do NOT abort.
    let conflict_out = Command::new("git")
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(worktree_path)
        .output()?;
    let conflicting_files: Vec<String> = String::from_utf8_lossy(&conflict_out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(PullOutcome::Conflict { conflicting_files })
}

/// Check out an *existing* branch into a new worktree at `worktree_path`.
///
/// Unlike `git_worktree_add` / `git_worktree_add_from` (which create a new
/// branch with `-b`), this attaches the worktree to a branch that already
/// exists. Used to create temporary worktrees for rebase operations.
pub fn git_worktree_checkout_existing(
    repo_root: &Path,
    worktree_path: &Path,
    branch: &str,
) -> GroveResult<()> {
    let path_str = worktree_path.to_string_lossy().to_string();
    run_git(repo_root, &["worktree", "add", &path_str, branch])
}

// ── Default branch detection ──────────────────────────────────────────────────

/// Detect the default branch for `repo_root` by inspecting `refs/remotes/origin/HEAD`.
///
/// Falls back to `"main"` if git is unavailable, the repo has no remote, or the
/// symbolic ref cannot be resolved.
pub fn detect_default_branch(repo_root: &Path) -> GroveResult<String> {
    let out = git_cmd(repo_root)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            let full = String::from_utf8_lossy(&o.stdout).trim().to_string();
            // Output is e.g. "refs/remotes/origin/main" — strip the prefix.
            if let Some(branch) = full.strip_prefix("refs/remotes/origin/") {
                return Ok(branch.to_string());
            }
        }
    }
    Ok("main".to_string())
}

/// Directly update a git ref to point at `new_sha` (`git update-ref <ref> <sha>`).
///
/// Unlike `git branch -f`, this works even when the target branch is currently
/// checked out in another worktree.
pub fn git_update_ref(repo_root: &Path, refname: &str, new_sha: &str) -> GroveResult<()> {
    run_git(repo_root, &["update-ref", refname, new_sha])
}

/// Return commits in `range` (e.g. `"main..HEAD"`) as one-line summaries.
///
/// Returns an empty Vec when the range is empty or git is unavailable.
pub fn git_log_oneline(cwd: &Path, range: &str) -> GroveResult<Vec<String>> {
    let out = Command::new("git")
        .args(["log", range, "--oneline"])
        .current_dir(cwd)
        .output()?;
    if !out.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

/// Ensure `.grove/` is listed in `project_root/.gitignore` if that file exists.
///
/// Only modifies `.gitignore` when it already exists — we never create it on
/// behalf of the user. If `.grove/` (or `.grove`) is already present, no-op.
pub fn git_ensure_grove_in_gitignore(project_root: &Path) -> GroveResult<()> {
    let gitignore_path = project_root.join(".gitignore");
    if !gitignore_path.exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string(&gitignore_path)?;
    if contents
        .lines()
        .any(|l| l.trim() == ".grove/" || l.trim() == ".grove")
    {
        return Ok(());
    }

    let prefix = if contents.is_empty() || contents.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let entry = format!("{prefix}# Grove internals — never commit\n.grove/\n");

    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&gitignore_path)?;
    file.write_all(entry.as_bytes())?;

    tracing::info!(path = %gitignore_path.display(), "added .grove/ to .gitignore");
    Ok(())
}

/// Ensure `.grove/` is listed in `project_root/.git/info/exclude`.
///
/// This is a repo-local exclude that applies to every linked worktree
/// (conversation worktrees, staging, etc.) without touching the user's
/// `.gitignore`. It prevents agents from accidentally staging a `.grove/`
/// directory they create inside a worktree.
///
/// Idempotent — no-op if the entry is already present.
pub fn git_ensure_grove_excluded(project_root: &Path) -> GroveResult<()> {
    let info_dir = project_root.join(".git").join("info");
    std::fs::create_dir_all(&info_dir)?;

    let exclude_path = info_dir.join("exclude");
    let existing = if exclude_path.exists() {
        std::fs::read_to_string(&exclude_path)?
    } else {
        String::new()
    };

    if existing
        .lines()
        .any(|l| l.trim() == ".grove/" || l.trim() == ".grove")
    {
        return Ok(());
    }

    let prefix = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let entry = format!("{prefix}# Grove internals — never commit from worktrees\n.grove/\n");

    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude_path)?;
    file.write_all(entry.as_bytes())?;

    Ok(())
}

// ── User-facing git operations ────────────────────────────────────────────────
//
// These are higher-level operations designed for the GUI command layer.
// They handle edge cases (set-upstream, friendly errors, untracked files)
// that the orchestration-level functions above do not.

/// Result of a user-initiated commit.
#[derive(Debug, Clone)]
pub struct CommitResult {
    pub sha: String,
    pub message: String,
}

/// Commit staged changes, optionally staging all first.
///
/// Unlike `git_commit` (orchestration), this:
/// - Does NOT use `--allow-empty`
/// - Auto-generates a message if empty
/// - Returns the commit SHA
pub fn git_commit_user(
    cwd: &Path,
    message: &str,
    include_unstaged: bool,
) -> GroveResult<CommitResult> {
    if include_unstaged {
        git_add_all(cwd)?;
    }

    let commit_msg = if message.trim().is_empty() {
        let out = git_cmd(cwd)
            .args(["diff", "--cached", "--stat"])
            .output()
            .ok();
        let file_count = out
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .count()
                    .saturating_sub(1)
            })
            .unwrap_or(0);
        format!(
            "Update {} file{}",
            file_count,
            if file_count == 1 { "" } else { "s" }
        )
    } else {
        message.to_string()
    };

    let out = git_cmd(cwd).args(["commit", "-m", &commit_msg]).output()?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stderr.contains("nothing to commit") || stdout.contains("nothing to commit") {
            return Err(GroveError::Runtime(
                "nothing to commit — working tree clean".to_string(),
            ));
        }
        return Err(GroveError::Runtime(format!("git commit failed: {stderr}")));
    }

    let sha = git_rev_parse_head(cwd)?;
    Ok(CommitResult {
        sha,
        message: commit_msg,
    })
}

/// Push to origin with auto set-upstream fallback.
///
/// Returns the git stderr output (which contains progress info) on success.
pub fn git_push_auto(cwd: &Path) -> GroveResult<String> {
    let output = git_cmd(cwd).args(["push"]).output()?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if stderr.contains("no upstream branch")
        || stderr.contains("has no upstream")
        || stderr.contains("--set-upstream")
    {
        let retry = git_cmd(cwd)
            .args(["push", "--set-upstream", "origin", "HEAD"])
            .output()?;

        if retry.status.success() {
            return Ok(String::from_utf8_lossy(&retry.stderr).to_string());
        }

        let retry_err = String::from_utf8_lossy(&retry.stderr).to_string();
        return Err(GroveError::Runtime(friendly_push_error(&retry_err)));
    }

    Err(GroveError::Runtime(friendly_push_error(&stderr)))
}

// ── Push error classification ─────────────────────────────────────────────────

/// Structured classification of git push failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushFailureKind {
    /// Remote has commits the local branch doesn't — needs pull first.
    NonFastForward,
    /// Authentication or authorization failure — not recoverable by agent.
    PermissionDenied,
    /// DNS / connectivity failure.
    NetworkError,
    /// The ref being pushed doesn't match any remote ref.
    RefNotFound,
    /// Unrecognized error.
    Unknown(String),
}

/// Classify a git push stderr into a structured failure kind.
pub fn classify_push_error(stderr: &str) -> PushFailureKind {
    if stderr.contains("non-fast-forward") || stderr.contains("[rejected]") {
        PushFailureKind::NonFastForward
    } else if stderr.contains("Permission denied")
        || stderr.contains("403")
        || stderr.contains("could not read Username")
    {
        PushFailureKind::PermissionDenied
    } else if stderr.contains("Could not resolve host") || stderr.contains("Connection refused") {
        PushFailureKind::NetworkError
    } else if stderr.contains("does not match any") {
        PushFailureKind::RefNotFound
    } else {
        PushFailureKind::Unknown(stderr.to_string())
    }
}

fn friendly_push_error(stderr: &str) -> String {
    match classify_push_error(stderr) {
        PushFailureKind::NonFastForward => {
            "Push rejected: remote has changes you don't have locally. Pull first, then push again."
                .to_string()
        }
        PushFailureKind::PermissionDenied => {
            if stderr.contains("could not read Username") {
                "Push failed: authentication required. Run `gh auth login` or configure git credentials.".to_string()
            } else {
                "Push failed: permission denied. Check your git credentials.".to_string()
            }
        }
        PushFailureKind::NetworkError => {
            "Push failed: network error. Check your internet connection.".to_string()
        }
        PushFailureKind::RefNotFound => {
            format!("Push failed: ref not found. {stderr}")
        }
        PushFailureKind::Unknown(_) => {
            format!("git push failed: {stderr}")
        }
    }
}

/// Full branch status with fallback chain:
/// 1. `@{upstream}` (tracking branch)
/// 2. `origin/<branch>` (conventional remote)
/// 3. `git status -sb` (last resort)
#[derive(Debug, Clone)]
pub struct BranchStatusInfo {
    pub branch: String,
    pub default_branch: String,
    pub ahead: i32,
    pub behind: i32,
    pub has_upstream: bool,
    pub remote_branch_exists: bool,
    pub comparison_mode: String,
}

pub fn git_branch_status_full(cwd: &Path) -> GroveResult<BranchStatusInfo> {
    let branch = git_current_branch(cwd)?;
    let default_branch = detect_default_branch(cwd)?;
    let upstream = git_upstream_ref(cwd);
    let remote_branch_exists = git_remote_branch_exists(cwd, "origin", &branch);

    let (ahead, behind, comparison_mode) = if let Some(upstream_ref) =
        upstream.as_ref().filter(|s| !s.trim().is_empty())
    {
        if let Some((ahead, behind)) = git_ahead_behind(cwd, upstream_ref, "HEAD") {
            (ahead, behind, "upstream".to_string())
        } else {
            let (ahead, behind) = parse_status_sb_ahead_behind(cwd);
            (ahead, behind, "status".to_string())
        }
    } else if remote_branch_exists {
        if let Some((ahead, behind)) = git_ahead_behind(cwd, &format!("origin/{branch}"), "HEAD") {
            (ahead, behind, "remote_branch".to_string())
        } else {
            let (ahead, behind) = parse_status_sb_ahead_behind(cwd);
            (ahead, behind, "status".to_string())
        }
    } else if let Some((ahead, behind)) =
        git_ahead_behind(cwd, &format!("origin/{default_branch}"), "HEAD")
    {
        (ahead, behind, "base_branch".to_string())
    } else {
        let (ahead, behind) = parse_status_sb_ahead_behind(cwd);
        let mode = if ahead > 0 || behind > 0 {
            "status"
        } else {
            "none"
        };
        (ahead, behind, mode.to_string())
    };

    Ok(BranchStatusInfo {
        branch,
        default_branch,
        ahead,
        behind,
        has_upstream: upstream.is_some(),
        remote_branch_exists,
        comparison_mode,
    })
}

pub fn git_upstream_ref(cwd: &Path) -> Option<String> {
    run_git_output(cwd, &["rev-parse", "--abbrev-ref", "@{upstream}"])
}

pub fn git_remote_branch_exists(cwd: &Path, remote: &str, branch: &str) -> bool {
    let refname = format!("refs/remotes/{remote}/{branch}");
    Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &refname])
        .current_dir(cwd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn git_remote_exists(cwd: &Path, remote: &str) -> bool {
    Command::new("git")
        .args(["remote", "get-url", remote])
        .current_dir(cwd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn git_register_branch_remote(cwd: &Path, remote: &str, branch: &str) -> GroveResult<String> {
    let refspec = format!("{branch}:refs/heads/{branch}");
    let push = Command::new("git")
        .args(["push", remote, &refspec])
        .current_dir(cwd)
        .output()?;

    if !push.status.success() {
        let stderr = String::from_utf8_lossy(&push.stderr).to_string();
        return Err(GroveError::Runtime(friendly_push_error(&stderr)));
    }

    let upstream_ref = format!("{remote}/{branch}");
    let _ = Command::new("git")
        .args(["branch", "--set-upstream-to", &upstream_ref, branch])
        .current_dir(cwd)
        .output();

    let stdout = String::from_utf8_lossy(&push.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&push.stderr).trim().to_string();
    if !stdout.is_empty() && !stderr.is_empty() {
        Ok(format!("{stdout}\n{stderr}"))
    } else if !stderr.is_empty() {
        Ok(stderr)
    } else {
        Ok(stdout)
    }
}

pub fn git_ref_contains_commit(cwd: &Path, commit_sha: &str, refname: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", commit_sha, refname])
        .current_dir(cwd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn git_detect_merge_conflict_files(
    repo_root: &Path,
    target_ref: &str,
    source_ref: &str,
) -> Vec<String> {
    let temp_path = std::env::temp_dir().join(format!(
        "grove-pr-conflicts-{}",
        uuid::Uuid::new_v4().simple()
    ));
    if git_worktree_add_detached_at(repo_root, &temp_path, target_ref).is_err() {
        return vec![];
    }

    let result = (|| {
        let merge_out = Command::new("git")
            .args(["merge", "--no-commit", "--no-ff", source_ref])
            .current_dir(&temp_path)
            .output()
            .ok()?;

        if merge_out.status.success() {
            let _ = Command::new("git")
                .args(["merge", "--abort"])
                .current_dir(&temp_path)
                .output();
            return Some(vec![]);
        }

        let files = Command::new("git")
            .args(["diff", "--name-only", "--diff-filter=U"])
            .current_dir(&temp_path)
            .output()
            .ok()
            .map(|out| {
                if out.status.success() {
                    String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .map(ToOwned::to_owned)
                        .collect()
                } else {
                    vec![]
                }
            })
            .unwrap_or_default();
        let _ = Command::new("git")
            .args(["merge", "--abort"])
            .current_dir(&temp_path)
            .output();
        Some(files)
    })()
    .unwrap_or_default();

    let _ = git_worktree_remove(repo_root, &temp_path);
    if temp_path.exists() {
        let _ = std::fs::remove_dir_all(&temp_path);
    }
    let _ = git_worktree_prune(repo_root);
    result
}

fn git_ahead_behind(cwd: &Path, left: &str, right: &str) -> Option<(i32, i32)> {
    let out = Command::new("git")
        .args([
            "rev-list",
            "--left-right",
            "--count",
            &format!("{left}...{right}"),
        ])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() == 2 {
        let behind = parts[0].parse::<i32>().unwrap_or(0);
        let ahead = parts[1].parse::<i32>().unwrap_or(0);
        Some((ahead, behind))
    } else {
        None
    }
}

fn parse_status_sb_ahead_behind(cwd: &Path) -> (i32, i32) {
    let out = Command::new("git")
        .args(["status", "-sb"])
        .current_dir(cwd)
        .output()
        .ok();
    if let Some(out) = out {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(line) = text.lines().next() {
                let mut ahead = 0i32;
                let mut behind = 0i32;
                if let Some(pos) = line.find("ahead ") {
                    let rest = &line[pos + 6..];
                    if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                        ahead = rest[..end].parse().unwrap_or(0);
                    } else {
                        ahead = rest.trim_end_matches(']').parse().unwrap_or(0);
                    }
                }
                if let Some(pos) = line.find("behind ") {
                    let rest = &line[pos + 7..];
                    if let Some(end) = rest.find(|c: char| !c.is_ascii_digit()) {
                        behind = rest[..end].parse().unwrap_or(0);
                    } else {
                        behind = rest.trim_end_matches(']').parse().unwrap_or(0);
                    }
                }
                return (ahead, behind);
            }
        }
    }
    (0, 0)
}

/// Revert specific files, handling both tracked and untracked.
///
/// - Tracked files: `git checkout -- <paths>`
/// - Untracked files: deleted from disk via `fs::remove_file` / `fs::remove_dir_all`
/// - Staged files: unstaged first via `git reset HEAD -- <paths>`, then reverted
pub fn git_revert_paths(cwd: &Path, paths: &[String]) -> GroveResult<()> {
    if paths.is_empty() {
        return Ok(());
    }

    // Use `git ls-files` to determine which paths are tracked by git.
    // This correctly identifies files/directories that git knows about.
    let out = Command::new("git")
        .args(["ls-files", "--error-unmatch", "--"])
        .args(paths)
        .current_dir(cwd)
        .output()?;

    let tracked_set: std::collections::HashSet<String> = if out.status.success() {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    } else {
        // ls-files --error-unmatch fails if ANY path is unknown.
        // Fall back to checking each path individually.
        let mut set = std::collections::HashSet::new();
        for path in paths {
            let check = Command::new("git")
                .args(["ls-files", "--error-unmatch", "--", path])
                .current_dir(cwd)
                .output();
            if let Ok(o) = check {
                if o.status.success() {
                    set.insert(path.clone());
                }
            }
        }
        set
    };

    let mut tracked = Vec::new();
    let mut untracked = Vec::new();

    for path in paths {
        if tracked_set.contains(path) {
            tracked.push(path.clone());
        } else {
            untracked.push(path.clone());
        }
    }

    // Unstage any staged tracked files first so checkout can revert them
    if !tracked.is_empty() {
        let mut reset_args = vec!["reset", "HEAD", "--"];
        let tracked_refs: Vec<&str> = tracked.iter().map(|s| s.as_str()).collect();
        reset_args.extend(&tracked_refs);
        // Ignore errors — file may not be staged
        let _ = Command::new("git")
            .args(&reset_args)
            .current_dir(cwd)
            .output();

        let mut checkout_args = vec!["checkout", "--"];
        checkout_args.extend(&tracked_refs);
        run_git(cwd, &checkout_args)?;
    }

    // Delete untracked files/directories from disk
    for path in &untracked {
        let full_path = cwd.join(path);
        if full_path.is_dir() {
            std::fs::remove_dir_all(&full_path).map_err(|e| {
                GroveError::Runtime(format!(
                    "failed to remove directory {}: {e}",
                    full_path.display()
                ))
            })?;
        } else if full_path.exists() {
            std::fs::remove_file(&full_path).map_err(|e| {
                GroveError::Runtime(format!(
                    "failed to remove file {}: {e}",
                    full_path.display()
                ))
            })?;
        }
    }

    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Run a git command and return trimmed stdout, or `None` on failure / empty output.
fn run_git_output(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = git_cmd(cwd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Return names of files changed between `from` and `to` (exclusive `from..to` range).
fn diff_name_only(repo_root: &Path, from: &str, to: &str) -> Vec<String> {
    let spec = format!("{from}..{to}");
    Command::new("git")
        .args(["diff", "--name-only", &spec])
        .current_dir(repo_root)
        .output()
        .map(|o| {
            if o.status.success() {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect()
            } else {
                vec![]
            }
        })
        .unwrap_or_default()
}

/// Build a `git` Command pre-configured with the user's full shell PATH.
///
/// All git subprocess calls in this module should use this instead of bare
/// `Command::new("git")` so macOS GUI apps can find git on non-system paths.
fn git_cmd(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd)
        .env("PATH", crate::capability::shell_path());
    cmd
}

pub(crate) fn run_git(cwd: &Path, args: &[&str]) -> GroveResult<()> {
    let out = git_cmd(cwd).args(args).output()?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(GroveError::Runtime(format!(
            "git {} failed: {stderr}",
            args.join(" ")
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temp git repo with an initial commit.
    fn temp_git_repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        run_git(dir.path(), &["init", "-b", "main"]).unwrap();
        run_git(dir.path(), &["config", "user.email", "test@test.com"]).unwrap();
        run_git(dir.path(), &["config", "user.name", "Test"]).unwrap();
        std::fs::write(dir.path().join("README.md"), "# Test\n").unwrap();
        run_git(dir.path(), &["add", "."]).unwrap();
        run_git(dir.path(), &["commit", "-m", "initial"]).unwrap();
        dir
    }

    // ── friendly_push_error ───────────────────────────────────────────────────

    #[test]
    fn push_error_non_fast_forward() {
        let msg = friendly_push_error("! [rejected] main -> main (non-fast-forward)");
        assert!(msg.contains("Pull first"), "should suggest pulling: {msg}");
    }

    #[test]
    fn push_error_permission_denied() {
        let msg = friendly_push_error("fatal: Permission denied (publickey).");
        assert!(
            msg.contains("permission denied"),
            "should mention permission: {msg}"
        );
    }

    #[test]
    fn push_error_auth_required() {
        let msg = friendly_push_error("fatal: could not read Username for 'https://github.com'");
        assert!(
            msg.contains("authentication required"),
            "should mention auth: {msg}"
        );
    }

    #[test]
    fn push_error_generic_preserves_stderr() {
        let msg = friendly_push_error("error: something unexpected happened");
        assert!(
            msg.contains("something unexpected"),
            "should preserve original: {msg}"
        );
    }

    // ── git_commit_user ───────────────────────────────────────────────────────

    #[test]
    fn commit_user_nothing_to_commit() {
        let dir = temp_git_repo();
        let result = git_commit_user(dir.path(), "test", false);
        assert!(result.is_err(), "should error when nothing to commit");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("nothing to commit"),
            "error should say nothing to commit: {msg}"
        );
    }

    #[test]
    fn commit_user_with_message() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("new.txt"), "content").unwrap();
        run_git(dir.path(), &["add", "new.txt"]).unwrap();

        let result = git_commit_user(dir.path(), "my commit message", false).unwrap();
        assert!(!result.sha.is_empty(), "sha should not be empty");
        assert_eq!(result.message, "my commit message");
    }

    #[test]
    fn commit_user_auto_generates_message_when_empty() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("a.txt"), "aaa").unwrap();
        std::fs::write(dir.path().join("b.txt"), "bbb").unwrap();
        run_git(dir.path(), &["add", "."]).unwrap();

        let result = git_commit_user(dir.path(), "", false).unwrap();
        assert!(
            result.message.contains("Update"),
            "auto-message should contain 'Update': {}",
            result.message
        );
        assert!(
            result.message.contains("file"),
            "auto-message should contain 'file': {}",
            result.message
        );
    }

    #[test]
    fn commit_user_include_unstaged_stages_and_commits() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("unstaged.txt"), "hello").unwrap();

        let result = git_commit_user(dir.path(), "include unstaged", true).unwrap();
        assert!(!result.sha.is_empty());
        // Verify the file was committed (working tree should be clean)
        let status = git_status_porcelain(dir.path()).unwrap();
        assert!(
            status.is_empty(),
            "working tree should be clean after commit: {status}"
        );
    }

    #[test]
    fn detect_merge_conflict_files_lists_unmerged_paths() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("README.md"), "main branch\n").unwrap();
        run_git(dir.path(), &["add", "README.md"]).unwrap();
        run_git(dir.path(), &["commit", "-m", "main change"]).unwrap();

        run_git(dir.path(), &["checkout", "-b", "feature"]).unwrap();
        std::fs::write(dir.path().join("README.md"), "feature branch\n").unwrap();
        run_git(dir.path(), &["add", "README.md"]).unwrap();
        run_git(dir.path(), &["commit", "-m", "feature change"]).unwrap();
        run_git(dir.path(), &["checkout", "main"]).unwrap();
        std::fs::write(dir.path().join("README.md"), "main branch updated again\n").unwrap();
        run_git(dir.path(), &["add", "README.md"]).unwrap();
        run_git(dir.path(), &["commit", "-m", "main change 2"]).unwrap();

        let conflicts = git_detect_merge_conflict_files(dir.path(), "main", "feature");

        assert_eq!(conflicts, vec!["README.md".to_string()]);
    }

    // ── git_revert_paths ──────────────────────────────────────────────────────

    #[test]
    fn revert_tracked_modified_file() {
        let dir = temp_git_repo();
        let file_path = dir.path().join("README.md");
        std::fs::write(&file_path, "modified content").unwrap();

        git_revert_paths(dir.path(), &["README.md".to_string()]).unwrap();

        let contents = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(
            contents, "# Test\n",
            "file should be reverted to committed version"
        );
    }

    #[test]
    fn revert_untracked_file_deletes_it() {
        let dir = temp_git_repo();
        let new_file = dir.path().join("new_file.txt");
        std::fs::write(&new_file, "untracked").unwrap();
        assert!(new_file.exists());

        git_revert_paths(dir.path(), &["new_file.txt".to_string()]).unwrap();

        assert!(!new_file.exists(), "untracked file should be deleted");
    }

    #[test]
    fn revert_untracked_directory_deletes_it() {
        let dir = temp_git_repo();
        let new_dir = dir.path().join("new_dir");
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("file.txt"), "nested").unwrap();
        assert!(new_dir.exists());

        git_revert_paths(dir.path(), &["new_dir".to_string()]).unwrap();

        assert!(!new_dir.exists(), "untracked directory should be deleted");
    }

    #[test]
    fn revert_mixed_tracked_and_untracked() {
        let dir = temp_git_repo();
        // Modify a tracked file
        std::fs::write(dir.path().join("README.md"), "changed").unwrap();
        // Create an untracked file
        std::fs::write(dir.path().join("extra.txt"), "extra").unwrap();

        git_revert_paths(
            dir.path(),
            &["README.md".to_string(), "extra.txt".to_string()],
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("README.md")).unwrap(),
            "# Test\n",
            "tracked file should be reverted"
        );
        assert!(
            !dir.path().join("extra.txt").exists(),
            "untracked file should be deleted"
        );
    }

    #[test]
    fn revert_empty_paths_is_no_op() {
        let dir = temp_git_repo();
        git_revert_paths(dir.path(), &[]).unwrap();
    }

    #[test]
    fn revert_staged_file_unstages_and_reverts() {
        let dir = temp_git_repo();
        std::fs::write(dir.path().join("README.md"), "staged change").unwrap();
        run_git(dir.path(), &["add", "README.md"]).unwrap();

        git_revert_paths(dir.path(), &["README.md".to_string()]).unwrap();

        let contents = std::fs::read_to_string(dir.path().join("README.md")).unwrap();
        assert_eq!(contents, "# Test\n", "staged file should be reverted");
        let status = git_status_porcelain(dir.path()).unwrap();
        assert!(status.is_empty(), "working tree should be clean: {status}");
    }

    // ── git_branch_status_full ────────────────────────────────────────────────

    #[test]
    fn branch_status_on_local_repo() {
        let dir = temp_git_repo();
        let info = git_branch_status_full(dir.path()).unwrap();
        assert_eq!(info.branch, "main");
        assert_eq!(info.ahead, 0);
        assert_eq!(info.behind, 0);
    }

    // ── git_ensure_grove_in_gitignore ─────────────────────────────────────────

    #[test]
    fn gitignore_no_op_when_file_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        // No .gitignore exists — must succeed without creating one.
        git_ensure_grove_in_gitignore(dir.path()).unwrap();
        assert!(
            !dir.path().join(".gitignore").exists(),
            ".gitignore must not be created"
        );
    }

    #[test]
    fn gitignore_appends_grove_when_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), "node_modules/\n*.log\n").unwrap();

        git_ensure_grove_in_gitignore(dir.path()).unwrap();

        let contents = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(
            contents.contains(".grove/"),
            ".grove/ must be added: {contents}"
        );
        // Original lines must be preserved.
        assert!(contents.contains("node_modules/"));
        assert!(contents.contains("*.log"));
    }

    #[test]
    fn gitignore_no_op_when_grove_slash_already_present() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(".gitignore"),
            "node_modules/\n.grove/\n*.log\n",
        )
        .unwrap();

        git_ensure_grove_in_gitignore(dir.path()).unwrap();

        let contents = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        // Must not duplicate the entry.
        assert_eq!(
            contents.matches(".grove/").count(),
            1,
            "must not duplicate .grove/"
        );
    }

    #[test]
    fn gitignore_no_op_when_grove_without_slash_already_present() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitignore"), ".grove\n").unwrap();

        git_ensure_grove_in_gitignore(dir.path()).unwrap();

        let contents = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert_eq!(
            contents.matches(".grove").count(),
            1,
            "must not duplicate .grove"
        );
    }

    #[test]
    fn gitignore_handles_file_without_trailing_newline() {
        let dir = tempfile::TempDir::new().unwrap();
        // No trailing newline — the appended entry must still start on its own line.
        std::fs::write(dir.path().join(".gitignore"), "*.log").unwrap();

        git_ensure_grove_in_gitignore(dir.path()).unwrap();

        let contents = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        // Must not produce "*.log.grove/" — there must be a newline between them.
        assert!(
            contents.contains("\n.grove/"),
            "entry must be on its own line: {contents}"
        );
    }

    // ── git_ensure_grove_excluded ─────────────────────────────────────────────

    #[test]
    fn exclude_creates_info_dir_and_file_when_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        // Fake a .git directory (file or dir — we only need the info/ sub-path).
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();

        git_ensure_grove_excluded(dir.path()).unwrap();

        let exclude = dir.path().join(".git").join("info").join("exclude");
        assert!(exclude.exists(), "exclude file must be created");
        let contents = std::fs::read_to_string(&exclude).unwrap();
        assert!(
            contents.contains(".grove/"),
            "exclude must contain .grove/: {contents}"
        );
    }

    #[test]
    fn exclude_no_op_when_grove_slash_already_present() {
        let dir = tempfile::TempDir::new().unwrap();
        let info = dir.path().join(".git").join("info");
        std::fs::create_dir_all(&info).unwrap();
        std::fs::write(info.join("exclude"), "# existing\n.grove/\n").unwrap();

        git_ensure_grove_excluded(dir.path()).unwrap();

        let contents = std::fs::read_to_string(info.join("exclude")).unwrap();
        assert_eq!(
            contents.matches(".grove/").count(),
            1,
            "must not duplicate .grove/"
        );
    }

    #[test]
    fn exclude_no_op_when_grove_without_slash_already_present() {
        let dir = tempfile::TempDir::new().unwrap();
        let info = dir.path().join(".git").join("info");
        std::fs::create_dir_all(&info).unwrap();
        std::fs::write(info.join("exclude"), ".grove\n").unwrap();

        git_ensure_grove_excluded(dir.path()).unwrap();

        let contents = std::fs::read_to_string(info.join("exclude")).unwrap();
        assert_eq!(
            contents.matches(".grove").count(),
            1,
            "must not duplicate .grove"
        );
    }

    #[test]
    fn exclude_appends_to_existing_content() {
        let dir = tempfile::TempDir::new().unwrap();
        let info = dir.path().join(".git").join("info");
        std::fs::create_dir_all(&info).unwrap();
        std::fs::write(info.join("exclude"), "# git default excludes\n*.swp\n").unwrap();

        git_ensure_grove_excluded(dir.path()).unwrap();

        let contents = std::fs::read_to_string(info.join("exclude")).unwrap();
        assert!(contents.contains(".grove/"), "exclude must contain .grove/");
        assert!(
            contents.contains("*.swp"),
            "original content must be preserved"
        );
    }

    #[test]
    fn exclude_idempotent_on_repeated_calls() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();

        git_ensure_grove_excluded(dir.path()).unwrap();
        git_ensure_grove_excluded(dir.path()).unwrap();
        git_ensure_grove_excluded(dir.path()).unwrap();

        let contents =
            std::fs::read_to_string(dir.path().join(".git").join("info").join("exclude")).unwrap();
        assert_eq!(
            contents.matches(".grove/").count(),
            1,
            "must not duplicate on repeated calls"
        );
    }
}
