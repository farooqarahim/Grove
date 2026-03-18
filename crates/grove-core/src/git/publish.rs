//! User-initiated publish pipeline: commit → push → PR.
//!
//! Three independent functions that share types and logging. The Tauri IPC
//! layer chains them based on the user's chosen "next step" from the
//! CommitModal (commit-only, commit+push, or commit+push+PR).
//!
//! This module is for **interactive** (GUI-driven) flows. The automated
//! post-run publish pipeline lives in `crate::publish::mod.rs`.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::capability::shell_path;
use crate::errors::{GroveError, GroveResult};
use crate::worktree::git_ops;

// ── Public types ──────────────────────────────────────────────────────────────

/// Which step the user picked in the CommitModal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishStep {
    Commit,
    Push,
    Pr,
}

/// Everything the caller needs for the pipeline.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PublishOpts {
    pub step: PublishStep,
    pub message: String,
    pub include_unstaged: bool,
    /// PR title — falls back to commit message if empty/None.
    pub pr_title: Option<String>,
    /// PR body (markdown). Falls back to a generated default.
    pub pr_body: Option<String>,
}

/// Result from the full pipeline. Fields are populated up to the step reached.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PublishResult {
    pub sha: String,
    pub commit_message: String,
    pub branch: String,
    /// Set after push completes. `None` if step == Commit.
    pub pushed: Option<bool>,
    /// Set after PR step. `None` if step != Pr.
    pub pr: Option<PrInfo>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PrInfo {
    pub url: String,
    pub number: u64,
    pub already_existed: bool,
}

// ── Default branch cache ──────────────────────────────────────────────────────

static DEFAULT_BRANCH_CACHE: Mutex<Option<HashMap<std::path::PathBuf, (String, Instant)>>> =
    Mutex::new(None);
const DEFAULT_BRANCH_TTL: Duration = Duration::from_secs(300);

// ── Core pipeline functions ───────────────────────────────────────────────────

/// Commit staged (and optionally unstaged) changes.
///
/// Delegates to `git_ops::git_commit_user` which handles staging, auto-message
/// generation, and commit creation.
pub fn commit(cwd: &Path, message: &str, include_unstaged: bool) -> GroveResult<PublishResult> {
    let branch = current_branch(cwd)?;
    tracing::info!(%branch, cwd = %cwd.display(), "publish::commit — starting");

    let result = git_ops::git_commit_user(cwd, message, include_unstaged)?;

    tracing::info!(
        sha = %result.sha,
        %branch,
        message = %result.message,
        "publish::commit — success"
    );

    Ok(PublishResult {
        sha: result.sha,
        commit_message: result.message,
        branch,
        pushed: None,
        pr: None,
    })
}

/// Push to origin with auto set-upstream fallback.
///
/// Expects a `PublishResult` from `commit()` so it can carry forward the SHA
/// and branch. On success, sets `pushed = Some(true)`.
pub fn push(cwd: &Path, mut result: PublishResult) -> GroveResult<PublishResult> {
    tracing::info!(
        branch = %result.branch,
        sha = %result.sha,
        "publish::push — starting"
    );

    match git_ops::git_push_auto(cwd) {
        Ok(_stderr) => {
            tracing::info!(branch = %result.branch, "publish::push — success");
            result.pushed = Some(true);
            Ok(result)
        }
        Err(e) => {
            tracing::error!(
                branch = %result.branch,
                error = %e,
                "publish::push — failed"
            );
            Err(e)
        }
    }
}

/// Create (or find existing) pull request via `gh` CLI.
///
/// Expects a `PublishResult` that has already been pushed (`pushed == Some(true)`).
/// Detects the default branch as the PR base. On success, sets `pr = Some(PrInfo)`.
pub fn create_pr(
    cwd: &Path,
    title: &str,
    body: &str,
    mut result: PublishResult,
) -> GroveResult<PublishResult> {
    let base = detect_default_branch(cwd);
    let head = &result.branch;

    tracing::info!(
        %head,
        %base,
        %title,
        "publish::create_pr — starting"
    );

    // Guard: check commits ahead
    let ahead = commits_ahead(cwd, &base);
    if ahead == Some(0) {
        return Err(GroveError::Runtime(
            "No commits ahead of base branch — nothing to create a PR for.".to_string(),
        ));
    }

    // Check for existing PR first
    if let Some(existing) = find_existing_pr(cwd, head, &base)? {
        tracing::info!(
            number = existing.number,
            url = %existing.url,
            "publish::create_pr — PR already exists, pushing updates"
        );
        result.pr = Some(existing);
        return Ok(result);
    }

    // Write body to temp file (preserves newlines better than --body arg)
    let body_file = std::env::temp_dir().join(format!(
        "grove_pr_body_{}.md",
        &result.sha[..8.min(result.sha.len())]
    ));
    std::fs::write(&body_file, body)
        .map_err(|e| GroveError::Runtime(format!("failed to write PR body to temp file: {e}")))?;

    let pr_out = gh_cmd(cwd)
        .args([
            "pr",
            "create",
            "--base",
            &base,
            "--head",
            head,
            "--title",
            title,
            "--body-file",
            body_file.to_str().unwrap_or(""),
        ])
        .output()
        .map_err(|e| GroveError::Runtime(format!("gh pr create failed to spawn: {e}")))?;

    let _ = std::fs::remove_file(&body_file);

    if pr_out.status.success() {
        let url = String::from_utf8_lossy(&pr_out.stdout).trim().to_string();
        let number = url
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        tracing::info!(%url, %number, %head, %base, "publish::create_pr — success");

        result.pr = Some(PrInfo {
            url,
            number,
            already_existed: false,
        });
        return Ok(result);
    }

    let stderr = String::from_utf8_lossy(&pr_out.stderr).trim().to_string();
    tracing::error!(%head, %base, %stderr, "publish::create_pr — gh pr create failed");

    // "already exists" → look up the existing PR
    if stderr.contains("already exists") {
        if let Some(existing) = find_existing_pr(cwd, head, &base)? {
            result.pr = Some(existing);
            return Ok(result);
        }
        return Err(GroveError::Runtime(
            "A pull request already exists for this branch but could not be looked up.".to_string(),
        ));
    }

    if stderr.contains("ORG_AUTH_APP_RESTRICTED") || stderr.contains("organization has enabled") {
        return Err(GroveError::Runtime(
            "PR creation blocked: your organization requires app authorization. \
             Approve the GitHub CLI app in your org settings."
                .to_string(),
        ));
    }

    Err(GroveError::Runtime(format!(
        "gh pr create failed: {stderr}"
    )))
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Detect the repository's default branch (usually `main` or `master`).
///
/// Uses a 5-minute in-memory cache. Tries local symbolic-ref first (no network),
/// then `gh` API, then falls back to `"main"`.
pub fn detect_default_branch(cwd: &Path) -> String {
    // 1. Check cache
    {
        let guard = DEFAULT_BRANCH_CACHE.lock().unwrap();
        if let Some(map) = guard.as_ref() {
            if let Some((branch, fetched_at)) = map.get(cwd) {
                if fetched_at.elapsed() < DEFAULT_BRANCH_TTL {
                    return branch.clone();
                }
            }
        }
    }

    let branch = detect_default_branch_uncached(cwd);

    // Store in cache
    {
        let mut guard = DEFAULT_BRANCH_CACHE.lock().unwrap();
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(cwd.to_path_buf(), (branch.clone(), Instant::now()));
    }

    branch
}

fn detect_default_branch_uncached(cwd: &Path) -> String {
    // Try local git symbolic-ref (fast, no network)
    if let Ok(out) = git_cmd(cwd)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
        .output()
    {
        if out.status.success() {
            let full = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(branch) = full.strip_prefix("origin/") {
                if !branch.is_empty() {
                    return branch.to_string();
                }
            }
            if !full.is_empty() {
                return full;
            }
        }
    }

    // Fall back to gh API (network call)
    if let Ok(out) = gh_cmd(cwd)
        .args([
            "repo",
            "view",
            "--json",
            "defaultBranchRef",
            "--jq",
            ".defaultBranchRef.name",
        ])
        .output()
    {
        if out.status.success() {
            let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !branch.is_empty() {
                return branch;
            }
        }
    }

    "main".to_string()
}

/// Look up an existing open PR for the given head→base branch pair.
pub fn find_existing_pr(cwd: &Path, head: &str, base: &str) -> GroveResult<Option<PrInfo>> {
    let out = gh_cmd(cwd)
        .args([
            "pr",
            "list",
            "--head",
            head,
            "--base",
            base,
            "--state",
            "open",
            "--json",
            "number,url",
            "--limit",
            "1",
        ])
        .output()
        .map_err(|e| GroveError::Runtime(format!("gh pr list failed: {e}")))?;

    if !out.status.success() {
        // Non-fatal — caller can try creating
        return Ok(None);
    }

    let json: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    if let Some(pr) = json.first() {
        let url = pr["url"].as_str().unwrap_or("").to_string();
        let number = pr["number"].as_u64().unwrap_or(0);
        if number > 0 {
            return Ok(Some(PrInfo {
                url,
                number,
                already_existed: true,
            }));
        }
    }

    Ok(None)
}

/// Resolve the current branch name. Returns error on detached HEAD.
fn current_branch(cwd: &Path) -> GroveResult<String> {
    let out = git_cmd(cwd)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| GroveError::Runtime(format!("git rev-parse failed: {e}")))?;

    let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        return Err(GroveError::Runtime(
            "Cannot publish: HEAD is detached (no branch name).".to_string(),
        ));
    }
    Ok(branch)
}

/// Count commits ahead of `origin/{base}`. Returns `None` on any error.
fn commits_ahead(cwd: &Path, base: &str) -> Option<u64> {
    let out = git_cmd(cwd)
        .args(["rev-list", "--count", &format!("origin/{base}..HEAD")])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .ok()
}

/// Build a `git` command with PATH and cwd set.
fn git_cmd(cwd: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd).env("PATH", shell_path());
    cmd
}

/// Build a `gh` command with PATH and cwd set.
fn gh_cmd(cwd: &Path) -> Command {
    let mut cmd = Command::new("gh");
    cmd.current_dir(cwd).env("PATH", shell_path());
    cmd
}
