//! Git operations for the Grove Graph agentic loop.
//!
//! Provides three lifecycle functions:
//! - [`create_graph_branch`] — creates a dedicated git branch for the graph run
//! - [`commit_phase`] — commits work produced by a completed phase
//! - [`finalize_graph`] — pushes the branch and optionally opens a PR via `gh`
//!
//! All git failures are best-effort: the functions degrade gracefully and never
//! propagate git errors as `Err` — only DB errors can cause an `Err` return.

use rusqlite::Connection;
use std::path::Path;
use std::process::Command;
use tracing::{info, warn};

use crate::db::repositories::grove_graph_repo;
use crate::errors::GroveResult;

// ── Public Result Type ───────────────────────────────────────────────────────

/// Outcome of [`finalize_graph`].
#[derive(Debug, Clone, serde::Serialize)]
pub struct GitFinalizeResult {
    /// The branch that was pushed, if available.
    pub branch: Option<String>,
    /// The HEAD commit SHA after push, if available.
    pub commit_sha: Option<String>,
    /// The URL of the pull request created by `gh pr create`, if successful.
    pub pr_url: Option<String>,
    /// Push/merge outcome: `"pending"`, `"merged"`, `"skipped"`, or `"failed"`.
    pub merge_status: String,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Convert an arbitrary string into a git-safe slug.
///
/// Rules: lowercase, keep alphanumeric and hyphens, collapse repeated hyphens,
/// strip leading/trailing hyphens, truncate to `max_len` chars.
fn slugify(s: &str, max_len: usize) -> String {
    let lowered = s.to_lowercase();
    let mut slug = String::with_capacity(lowered.len());
    let mut last_was_hyphen = false;

    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_hyphen = false;
        } else if !last_was_hyphen && !slug.is_empty() {
            slug.push('-');
            last_was_hyphen = true;
        }
    }

    // Strip trailing hyphen that may have been appended.
    let slug = slug.trim_end_matches('-');

    // Truncate at a char boundary.
    let truncated = if slug.len() > max_len {
        // Walk backwards from max_len to find a char boundary.
        let mut end = max_len;
        while !slug.is_char_boundary(end) {
            end -= 1;
        }
        &slug[..end]
    } else {
        slug
    };

    // Strip any trailing hyphen that became the last char after truncation.
    truncated.trim_end_matches('-').to_string()
}

/// Run `git rev-parse HEAD` in `dir` and return the short SHA (7 chars), or
/// `None` if git is unavailable or the repo has no commits yet.
fn head_sha(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
}

// ── detect_current_branch ────────────────────────────────────────────────────

/// Detect the current git branch in the given directory.
///
/// Runs `git rev-parse --abbrev-ref HEAD` and returns `Some(branch_name)`
/// on success, or `None` if git is unavailable or the directory is not a
/// git repository.
pub fn detect_current_branch(dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        None
    } else {
        Some(branch)
    }
}

// ── create_graph_branch ──────────────────────────────────────────────────────

/// Create a dedicated git branch for a graph run and persist it to the DB.
///
/// Branch name format: `grove-graph/{id_short}/{title_slug}`
/// - `id_short` is the first 8 characters of `graph_id`
/// - `title_slug` is the graph title slugified to max 40 chars
///
/// If git is unavailable or the directory is not a git repository the function
/// logs a warning and returns `Ok("no-git")` — it never returns `Err` for git
/// failures.  DB errors are propagated normally.
pub fn create_graph_branch(
    conn: &Connection,
    project_root: &Path,
    graph_id: &str,
) -> GroveResult<String> {
    let graph = grove_graph_repo::get_graph(conn, graph_id)?;

    let id_short = &graph_id[..8.min(graph_id.len())];
    let title_slug = slugify(&graph.title, 40);
    let branch = format!("grove-graph/{id_short}/{title_slug}");

    let output = Command::new("git")
        .args(["checkout", "-b", &branch])
        .current_dir(project_root)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            info!(
                graph_id,
                branch = branch.as_str(),
                "created graph git branch"
            );
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            warn!(
                graph_id,
                branch = branch.as_str(),
                stderr = stderr.as_ref(),
                "git checkout -b failed — skipping branch creation"
            );
            return Ok("no-git".to_string());
        }
        Err(e) => {
            warn!(
                graph_id,
                error = %e,
                "git not available — skipping branch creation"
            );
            return Ok("no-git".to_string());
        }
    }

    grove_graph_repo::set_graph_git_branch(conn, graph_id, &branch)?;

    Ok(branch)
}

// ── commit_step ──────────────────────────────────────────────────────────────

/// Stage all changes and create a commit recording a completed step.
///
/// Returns:
/// - `Ok(Some(sha))` — commit was created
/// - `Ok(None)` — nothing to commit (working tree was clean), or git failed
///
/// DB errors are propagated; git errors are logged and swallowed.
pub fn commit_step(
    conn: &Connection,
    project_root: &Path,
    graph_id: &str,
    step_id: &str,
) -> GroveResult<Option<String>> {
    let step = grove_graph_repo::get_step(conn, step_id)?;
    let _graph = grove_graph_repo::get_graph(conn, graph_id)?;

    let grade_display = step
        .grade
        .map(|g| g.to_string())
        .unwrap_or_else(|| "N/A".to_string());

    let commit_message = format!(
        "[Grove Graph] Step: {}\n\nType: {}\nGrade: {}/10\nIteration: {}/{}",
        step.task_name, step.step_type, grade_display, step.run_iteration, step.max_iterations,
    );

    // Stage everything.
    let add_out = Command::new("git")
        .args(["add", "-A"])
        .current_dir(project_root)
        .output();

    match add_out {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            warn!(
                step_id,
                stderr = stderr.as_ref(),
                "git add -A failed — skipping step commit"
            );
            return Ok(None);
        }
        Err(e) => {
            warn!(
                step_id,
                error = %e,
                "git not available — skipping step commit"
            );
            return Ok(None);
        }
    }

    // Attempt to commit.
    let commit_out = Command::new("git")
        .args(["commit", "-m", &commit_message])
        .current_dir(project_root)
        .output();

    match commit_out {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            if stdout.contains("nothing to commit")
                || stderr.contains("nothing to commit")
                || stdout.contains("nothing added to commit")
                || stderr.contains("nothing added to commit")
            {
                info!(step_id, "no changes to commit for step");
                return Ok(None);
            }
            warn!(
                step_id,
                stderr = stderr.as_ref(),
                "git commit failed — skipping step commit"
            );
            return Ok(None);
        }
        Err(e) => {
            warn!(
                step_id,
                error = %e,
                "git not available — skipping step commit"
            );
            return Ok(None);
        }
    }

    let sha = match head_sha(project_root) {
        Some(s) => s,
        None => {
            warn!(step_id, "could not read HEAD SHA after step commit");
            return Ok(None);
        }
    };

    info!(
        step_id,
        sha = sha.as_str(),
        step_name = step.task_name.as_str(),
        "step commit created"
    );

    Ok(Some(sha))
}

// ── commit_chunk ─────────────────────────────────────────────────────────────

/// Commit all worktree changes from a completed chunk of steps.
///
/// Similar to `commit_step` but creates a single commit for multiple steps.
///
/// Returns:
/// - `Ok(Some(sha))` — commit was created
/// - `Ok(None)` — nothing to commit (working tree was clean), or git failed
pub fn commit_chunk(
    conn: &Connection,
    project_root: &Path,
    graph_id: &str,
    step_ids: &[String],
) -> GroveResult<Option<String>> {
    if step_ids.is_empty() {
        return Ok(None);
    }

    // Build commit message from completed steps.
    let step_names: Vec<String> = step_ids
        .iter()
        .filter_map(|id| {
            grove_graph_repo::get_step(conn, id).ok().map(|s| {
                let grade_str = s
                    .grade
                    .map(|g| format!(" (grade {}/10)", g))
                    .unwrap_or_default();
                format!("  - {}{}", s.task_name, grade_str)
            })
        })
        .collect();

    let _graph = grove_graph_repo::get_graph(conn, graph_id)?;
    let message = format!(
        "[Grove Graph] Chunk: {} steps completed\n\n{}\nGraph: {}",
        step_ids.len(),
        step_names.join("\n"),
        graph_id,
    );

    // Stage everything.
    let output = Command::new("git")
        .args(["add", "-A"])
        .current_dir(project_root)
        .output();

    match output {
        Ok(out) if !out.status.success() => {
            warn!(
                stderr = %String::from_utf8_lossy(&out.stderr),
                "git add failed in commit_chunk"
            );
            return Ok(None);
        }
        Err(e) => {
            warn!(error = %e, "git add failed to launch in commit_chunk");
            return Ok(None);
        }
        _ => {}
    }

    // Check if there are staged changes.
    let diff_output = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .current_dir(project_root)
        .output();

    // If diff --cached --quiet exits 0, there are no staged changes.
    if let Ok(ref o) = diff_output {
        if o.status.success() {
            info!("commit_chunk: no changes to commit");
            return Ok(None);
        }
    }

    let commit_output = Command::new("git")
        .args(["commit", "-m", &message])
        .current_dir(project_root)
        .output();

    match commit_output {
        Ok(o) if o.status.success() => {
            let sha = head_sha(project_root).unwrap_or_default();
            info!(sha = %sha, steps = step_ids.len(), "committed chunk");
            Ok(Some(sha))
        }
        Ok(o) => {
            warn!(
                stderr = %String::from_utf8_lossy(&o.stderr),
                "git commit failed in commit_chunk"
            );
            Ok(None)
        }
        Err(e) => {
            warn!(error = %e, "git commit failed in commit_chunk");
            Ok(None)
        }
    }
}

// ── commit_phase ─────────────────────────────────────────────────────────────

/// Stage all changes and create a commit recording a completed phase.
///
/// The commit message includes the phase name, ordinal, completed step count,
/// and the judge grade so the history is self-documenting.
///
/// Returns:
/// - `Ok(Some(sha))` — commit was created and SHA stored on the phase record
/// - `Ok(None)` — nothing to commit (working tree was clean), or git failed
///
/// DB errors are propagated; git errors are logged and swallowed.
pub fn commit_phase(
    conn: &Connection,
    project_root: &Path,
    graph_id: &str,
    phase_id: &str,
) -> GroveResult<Option<String>> {
    let phase = grove_graph_repo::get_phase(conn, phase_id)?;
    let _graph = grove_graph_repo::get_graph(conn, graph_id)?;

    // Count steps with status "closed" (i.e. successfully completed).
    let steps = grove_graph_repo::list_steps(conn, phase_id)?;
    let completed_count = steps.iter().filter(|s| s.status == "closed").count();

    let grade_display = phase
        .grade
        .map(|g| g.to_string())
        .unwrap_or_else(|| "N/A".to_string());

    let commit_message = format!(
        "[Grove Graph] Phase {}: {}\n\nSteps completed: {}\nPhase grade: {}/10\nGraph: {}",
        phase.ordinal, phase.task_name, completed_count, grade_display, graph_id,
    );

    // Stage everything.
    let add_out = Command::new("git")
        .args(["add", "-A"])
        .current_dir(project_root)
        .output();

    match add_out {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            warn!(
                phase_id,
                stderr = stderr.as_ref(),
                "git add -A failed — skipping phase commit"
            );
            return Ok(None);
        }
        Err(e) => {
            warn!(
                phase_id,
                error = %e,
                "git not available — skipping phase commit"
            );
            return Ok(None);
        }
    }

    // Attempt to commit.
    let commit_out = Command::new("git")
        .args(["commit", "-m", &commit_message])
        .current_dir(project_root)
        .output();

    match commit_out {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            // "nothing to commit" exits with code 1 and prints to stdout.
            if stdout.contains("nothing to commit")
                || stderr.contains("nothing to commit")
                || stdout.contains("nothing added to commit")
                || stderr.contains("nothing added to commit")
            {
                info!(phase_id, "no changes to commit for phase");
                return Ok(None);
            }
            warn!(
                phase_id,
                stderr = stderr.as_ref(),
                "git commit failed — skipping phase commit"
            );
            return Ok(None);
        }
        Err(e) => {
            warn!(
                phase_id,
                error = %e,
                "git not available — skipping phase commit"
            );
            return Ok(None);
        }
    }

    // Read the new HEAD SHA.
    let sha = match head_sha(project_root) {
        Some(s) => s,
        None => {
            warn!(phase_id, "could not read HEAD SHA after phase commit");
            return Ok(None);
        }
    };

    grove_graph_repo::set_phase_git_commit(conn, phase_id, &sha)?;

    info!(
        phase_id,
        sha = sha.as_str(),
        phase_name = phase.task_name.as_str(),
        "phase commit created"
    );

    Ok(Some(sha))
}

// ── finalize_graph ───────────────────────────────────────────────────────────

/// Push the graph branch to origin and optionally open a pull request via `gh`.
///
/// Behaviour is controlled by the graph's `GraphConfig` flags:
/// - `git_push`: if false, push is skipped entirely.
/// - `git_create_pr`: if false (or `git_push` is false), PR is skipped.
///
/// All git and GitHub CLI failures are tolerated — the function degrades
/// gracefully. Only DB errors are returned as `Err`.
///
/// The PR body includes a summary of all phases and their grades.
pub fn finalize_graph(
    conn: &Connection,
    project_root: &Path,
    graph_id: &str,
) -> GroveResult<GitFinalizeResult> {
    let graph = grove_graph_repo::get_graph(conn, graph_id)?;
    let config = grove_graph_repo::get_graph_config(conn, graph_id)?;
    let phases = grove_graph_repo::list_phases(conn, graph_id)?;

    let branch = match &graph.git_branch {
        Some(b) if !b.is_empty() => b.clone(),
        _ => {
            warn!(graph_id, "no git branch recorded — skipping finalize");
            return Ok(GitFinalizeResult {
                branch: None,
                commit_sha: None,
                pr_url: None,
                merge_status: "failed".to_string(),
            });
        }
    };

    let commit_sha = head_sha(project_root);

    // ── Push branch (if enabled) ─────────────────────────────────────────────

    let push_succeeded = if !config.git_push {
        info!(graph_id, "git_push disabled in config — skipping push");
        false
    } else {
        let push_out = Command::new("git")
            .args(["push", "origin", &branch])
            .current_dir(project_root)
            .output();

        match push_out {
            Ok(out) if out.status.success() => {
                info!(
                    graph_id,
                    branch = branch.as_str(),
                    "pushed graph branch to origin"
                );
                true
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                warn!(
                    graph_id,
                    branch = branch.as_str(),
                    stderr = stderr.as_ref(),
                    "git push failed"
                );
                false
            }
            Err(e) => {
                warn!(graph_id, error = %e, "git not available — push skipped");
                false
            }
        }
    };

    if !push_succeeded {
        let merge_status = if config.git_push { "failed" } else { "pending" };
        let result = GitFinalizeResult {
            branch: Some(branch),
            commit_sha,
            pr_url: None,
            merge_status: merge_status.to_string(),
        };

        grove_graph_repo::set_graph_git_final(
            conn,
            graph_id,
            result.commit_sha.as_deref().unwrap_or(""),
            None,
            &result.merge_status,
        )?;

        return Ok(result);
    }

    // ── PR creation via `gh` (if enabled) ────────────────────────────────────

    let pr_url = if config.git_create_pr {
        let pr_body = build_pr_body(&graph.title, &phases);
        let pr_title = format!("[Grove Graph] {}", graph.title);
        attempt_gh_pr_create(project_root, &pr_title, &pr_body)
    } else {
        info!(graph_id, "git_create_pr disabled in config — skipping PR");
        None
    };

    let merge_status = "pending".to_string();

    let result = GitFinalizeResult {
        branch: Some(branch),
        commit_sha: commit_sha.clone(),
        pr_url: pr_url.clone(),
        merge_status: merge_status.clone(),
    };

    grove_graph_repo::set_graph_git_final(
        conn,
        graph_id,
        commit_sha.as_deref().unwrap_or(""),
        pr_url.as_deref(),
        &merge_status,
    )?;

    info!(
        graph_id,
        merge_status = merge_status.as_str(),
        pr_url = pr_url.as_deref().unwrap_or("none"),
        "graph finalized"
    );

    Ok(result)
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Build the PR body markdown summarising all phases and their grades.
fn build_pr_body(graph_title: &str, phases: &[grove_graph_repo::GraphPhaseRow]) -> String {
    let mut body = format!("## Grove Graph: {graph_title}\n\n### Phase Summary\n\n");

    if phases.is_empty() {
        body.push_str("No phases recorded.\n");
    } else {
        for phase in phases {
            let grade = phase
                .grade
                .map(|g| format!("{g}/10"))
                .unwrap_or_else(|| "N/A".to_string());
            let status = &phase.status;
            body.push_str(&format!(
                "- **Phase {}**: {} — Grade: {} ({})\n",
                phase.ordinal, phase.task_name, grade, status
            ));
        }
    }

    body.push_str("\n---\n*Generated by Grove Graph agentic loop.*\n");
    body
}

/// Attempt to create a GitHub pull request using the `gh` CLI.
///
/// Returns `Some(url)` on success, `None` if `gh` is unavailable or returns
/// a non-zero exit code.
fn attempt_gh_pr_create(project_root: &Path, title: &str, body: &str) -> Option<String> {
    let output = Command::new("gh")
        .args([
            "pr",
            "create",
            "--title",
            title,
            "--body",
            body,
            "--fill-first",
        ])
        .current_dir(project_root)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if url.is_empty() { None } else { Some(url) }
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            warn!(
                pr_title = title,
                stderr = stderr.as_ref(),
                "gh pr create failed — PR skipped"
            );
            None
        }
        Err(e) => {
            warn!(
                error = %e,
                "gh CLI not available — PR creation skipped"
            );
            None
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── slugify ──────────────────────────────────────────────────────────────

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello World", 40), "hello-world");
    }

    #[test]
    fn slugify_special_chars() {
        assert_eq!(slugify("Fix: Auth & API (v2)", 40), "fix-auth-api-v2");
    }

    #[test]
    fn slugify_truncates_at_max_len() {
        let long = "abcdefghij".repeat(10); // 100 chars
        let slug = slugify(&long, 40);
        assert!(slug.len() <= 40);
    }

    #[test]
    fn slugify_no_trailing_hyphen_after_truncate() {
        // Force a case where the 40th char is a separator.
        let s = "a".repeat(39) + "  extra";
        let slug = slugify(&s, 40);
        assert!(!slug.ends_with('-'));
    }

    #[test]
    fn slugify_empty_string() {
        assert_eq!(slugify("", 40), "");
    }

    #[test]
    fn slugify_already_slug() {
        assert_eq!(slugify("already-good", 40), "already-good");
    }

    #[test]
    fn git_finalize_result_is_debug_clone_serialize() {
        let r = GitFinalizeResult {
            branch: Some("grove-graph/abc12345/my-feature".into()),
            commit_sha: Some("deadbeef".into()),
            pr_url: Some("https://github.com/org/repo/pull/1".into()),
            merge_status: "pending".into(),
        };
        let r2 = r.clone();
        assert_eq!(r2.merge_status, "pending");
        assert!(r2.branch.is_some());
        // Verify serde serialization doesn't panic.
        let json = serde_json::to_string(&r2).unwrap();
        assert!(json.contains("pending"));
    }

    // ── DB-backed tests ──────────────────────────────────────────────────────

    fn test_db() -> rusqlite::Connection {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        crate::db::DbHandle::new(dir.path()).connect().unwrap()
    }

    fn seed_conversation(conn: &rusqlite::Connection, id: &str) {
        conn.execute(
            "INSERT INTO conversations (id, project_id, state, conversation_kind, \
             remote_registration_state, created_at, updated_at) \
             VALUES (?1, 'proj1', 'active', 'run', 'none', \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [id],
        )
        .unwrap();
    }

    fn seed_graph(conn: &rusqlite::Connection) -> String {
        seed_conversation(conn, "conv_git_test");
        grove_graph_repo::insert_graph(conn, "conv_git_test", "My Feature Graph", "desc", None)
            .unwrap()
    }

    fn seed_phase(conn: &rusqlite::Connection, graph_id: &str, ordinal: i64) -> String {
        grove_graph_repo::insert_phase(
            conn,
            graph_id,
            &format!("Phase {ordinal}"),
            "Build things",
            ordinal,
            "[]",
            false,
            None,
        )
        .unwrap()
    }

    #[test]
    fn create_graph_branch_returns_no_git_when_not_in_repo() {
        let conn = test_db();
        let graph_id = seed_graph(&conn);

        // Use a non-git directory so git checkout -b will fail.
        let tmp = tempfile::TempDir::new().unwrap();
        let result = create_graph_branch(&conn, tmp.path(), &graph_id).unwrap();

        // Should return "no-git" and NOT update the DB branch.
        assert_eq!(result, "no-git");
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).unwrap();
        assert!(graph.git_branch.is_none());
    }

    #[test]
    fn create_graph_branch_builds_correct_branch_name() {
        // We can't run git here, but we can verify the slug logic by inspecting
        // what slugify produces for a known title.
        let id_short = "abcd1234";
        let title = "My Feature Graph";
        let slug = slugify(title, 40);
        let branch = format!("grove-graph/{id_short}/{slug}");
        assert_eq!(branch, "grove-graph/abcd1234/my-feature-graph");
    }

    #[test]
    fn commit_phase_returns_none_when_not_in_repo() {
        let conn = test_db();
        let graph_id = seed_graph(&conn);
        let phase_id = seed_phase(&conn, &graph_id, 0);

        let tmp = tempfile::TempDir::new().unwrap();
        let result = commit_phase(&conn, tmp.path(), &graph_id, &phase_id).unwrap();

        // Should return None gracefully.
        assert!(result.is_none());
        let phase = grove_graph_repo::get_phase(&conn, &phase_id).unwrap();
        assert!(phase.git_commit_sha.is_none());
    }

    #[test]
    fn finalize_graph_returns_failed_when_no_branch_set() {
        let conn = test_db();
        let graph_id = seed_graph(&conn);

        let tmp = tempfile::TempDir::new().unwrap();
        let result = finalize_graph(&conn, tmp.path(), &graph_id).unwrap();

        assert_eq!(result.merge_status, "failed");
        assert!(result.branch.is_none());
        assert!(result.pr_url.is_none());
    }

    #[test]
    fn finalize_graph_returns_failed_when_push_fails() {
        let conn = test_db();
        let graph_id = seed_graph(&conn);

        // Enable git_push so finalize actually attempts it.
        let mut config = crate::grove_graph::GraphConfig::default();
        config.git_push = true;
        grove_graph_repo::set_graph_config(&conn, &graph_id, &config).unwrap();

        // Manually set a branch name on the graph so finalize attempts push.
        grove_graph_repo::set_graph_git_branch(
            &conn,
            &graph_id,
            "grove-graph/abc12345/test-branch",
        )
        .unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let result = finalize_graph(&conn, tmp.path(), &graph_id).unwrap();

        // Push will fail (not a git repo) — should degrade to "failed".
        assert_eq!(result.merge_status, "failed");
        assert_eq!(
            result.branch.as_deref(),
            Some("grove-graph/abc12345/test-branch")
        );
    }

    #[test]
    fn build_pr_body_includes_phase_names() {
        let phases = vec![grove_graph_repo::GraphPhaseRow {
            id: "p1".into(),
            graph_id: "g1".into(),
            task_name: "Setup".into(),
            task_objective: "Set up the project".into(),
            outcome: None,
            ai_comments: None,
            grade: Some(8),
            reference_doc_path: None,
            ref_required: false,
            status: "closed".into(),
            validation_status: "passed".into(),
            ordinal: 0,
            depends_on_json: "[]".into(),
            git_commit_sha: None,
            conversation_id: None,
            created_run_id: None,
            executed_run_id: None,
            validator_run_id: None,
            judge_run_id: None,
            execution_agent: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }];

        let body = build_pr_body("My Graph", &phases);
        assert!(body.contains("My Graph"));
        assert!(body.contains("Setup"));
        assert!(body.contains("8/10"));
        assert!(body.contains("closed"));
    }
}
