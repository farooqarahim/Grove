use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::GroveConfig;
use crate::db::DbHandle;
use crate::db::repositories::runs_repo;
use crate::errors::{GroveError, GroveResult};
use crate::events;
use crate::tracker::write_back::{self, WriteBackContext};
use crate::worktree::git_ops;

const MARKER_PREFIX: &str = "<!-- grove-run:";
const READ_COMMAND_TIMEOUT_SECS: u64 = 30;
const WRITE_COMMAND_TIMEOUT_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishResult {
    pub run_id: String,
    pub publish_status: String,
    pub final_commit_sha: Option<String>,
    pub pr_url: Option<String>,
    pub published_at: Option<String>,
    pub error: Option<String>,
}

enum PublishMode {
    Automatic,
    Retry,
}

pub fn publish_run(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    run_id: &str,
    run_worktree_path: &Path,
    provider: Option<std::sync::Arc<dyn crate::providers::Provider>>,
    model: Option<&str>,
) -> GroveResult<PublishResult> {
    publish_run_inner(
        conn,
        cfg,
        project_root,
        run_id,
        run_worktree_path,
        PublishMode::Automatic,
        provider,
        model,
    )
}

pub fn retry_publish(project_root: &Path, run_id: &str) -> GroveResult<PublishResult> {
    let cfg = GroveConfig::load_or_create(project_root)?;
    let handle = DbHandle::new(project_root);
    let mut conn = handle.connect()?;
    let worktree_path = resolve_run_worktree(&conn, project_root, run_id)?;
    publish_run_inner(
        &mut conn,
        &cfg,
        project_root,
        run_id,
        &worktree_path,
        PublishMode::Retry,
        None, // No provider available in manual retry
        None,
    )
}

pub fn recover_interrupted_publishes(
    conn: &mut Connection,
    project_root: &Path,
    cfg: &GroveConfig,
) -> GroveResult<Vec<PublishResult>> {
    if !cfg.publish.retry_on_startup {
        return Ok(vec![]);
    }

    let run_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT id FROM runs
             WHERE state='publishing'
                OR (publish_status='pending_retry' AND final_commit_sha IS NOT NULL)
             ORDER BY updated_at ASC",
        )?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };

    let mut results = Vec::new();
    for run_id in run_ids {
        let worktree_path = match resolve_run_worktree(conn, project_root, &run_id) {
            Ok(path) => path,
            Err(err) => {
                tracing::warn!(run_id, error = %err, "publish recovery: worktree unavailable");
                continue;
            }
        };
        match publish_run_inner(
            conn,
            cfg,
            project_root,
            &run_id,
            &worktree_path,
            PublishMode::Retry,
            None, // No provider available at startup recovery
            None,
        ) {
            Ok(result) => results.push(result),
            Err(err) => {
                tracing::warn!(run_id, error = %err, "publish recovery failed");
            }
        }
    }

    Ok(results)
}

#[allow(clippy::too_many_arguments)]
fn publish_run_inner(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    run_id: &str,
    run_worktree_path: &Path,
    mode: PublishMode,
    provider: Option<std::sync::Arc<dyn crate::providers::Provider>>,
    model: Option<&str>,
) -> GroveResult<PublishResult> {
    let now = Utc::now().to_rfc3339();
    let should_publish_remote =
        cfg.publish.enabled && (cfg.publish.auto_on_success || matches!(mode, PublishMode::Retry));

    if !git_ops::is_git_repo(run_worktree_path) || !git_ops::has_commits(run_worktree_path) {
        runs_repo::update_publish(
            conn,
            run_id,
            "skipped_no_changes",
            None,
            None,
            None,
            None,
            &now,
        )?;
        return Ok(PublishResult {
            run_id: run_id.to_string(),
            publish_status: "skipped_no_changes".to_string(),
            final_commit_sha: None,
            pr_url: None,
            published_at: None,
            error: None,
        });
    }

    let context = PublishContext::load(conn, run_id)?;
    ensure_publish_preflight(conn, run_id, run_worktree_path, &context)?;

    let existing_sha = context
        .final_commit_sha
        .clone()
        .or_else(|| detect_existing_run_commit(run_worktree_path, run_id));

    let commit_sha = if let Some(sha) = existing_sha {
        sha
    } else {
        let changes = working_tree_changes(run_worktree_path)?;
        if changes.is_empty() {
            // No uncommitted changes. The engine's commit_agent_work() commits each
            // agent session without a run_id tag (format: "grove(agent): ..."), so
            // detect_existing_run_commit() won't find those commits. Fall back to
            // checking whether HEAD has commits ahead of the default branch — if so,
            // those agent commits represent this run's work and should be published.
            if git_ops::branch_has_local_commits(run_worktree_path, &cfg.project.default_branch) {
                git_ops::git_rev_parse_head(run_worktree_path)
                    .map_err(|e| GroveError::Runtime(format!("git rev-parse HEAD failed: {e}")))?
            } else {
                runs_repo::update_publish(
                    conn,
                    run_id,
                    "skipped_no_changes",
                    None,
                    None,
                    context.pr_url.as_deref(),
                    None,
                    &now,
                )?;
                return Ok(PublishResult {
                    run_id: run_id.to_string(),
                    publish_status: "skipped_no_changes".to_string(),
                    final_commit_sha: None,
                    pr_url: context.pr_url,
                    published_at: None,
                    error: None,
                });
            }
        } else {
            ensure_no_conflict_markers(run_worktree_path, &changes)?;
            commit_run_changes(run_worktree_path, run_id, &context)?
        }
    };

    runs_repo::update_publish(
        conn,
        run_id,
        "pending_retry",
        None,
        Some(&commit_sha),
        context.pr_url.as_deref(),
        None,
        &now,
    )?;

    if !should_publish_remote {
        emit_publish_event(
            conn,
            run_id,
            "pending_retry",
            context.pr_url.as_deref(),
            None,
        )?;
        return Ok(PublishResult {
            run_id: run_id.to_string(),
            publish_status: "pending_retry".to_string(),
            final_commit_sha: Some(commit_sha),
            pr_url: context.pr_url,
            published_at: None,
            error: None,
        });
    }

    match publish_remote(
        conn,
        cfg,
        run_id,
        run_worktree_path,
        &context,
        &commit_sha,
        provider.as_deref(),
        model,
    ) {
        Ok(result) => {
            runs_repo::update_publish(
                conn,
                run_id,
                &result.publish_status,
                None,
                result.final_commit_sha.as_deref(),
                result.pr_url.as_deref(),
                result.published_at.as_deref(),
                &Utc::now().to_rfc3339(),
            )?;
            emit_publish_event(
                conn,
                run_id,
                &result.publish_status,
                result.pr_url.as_deref(),
                None,
            )?;
            if cfg.publish.comment_on_issue {
                if let Err(err) = post_issue_write_back(
                    conn,
                    cfg,
                    project_root,
                    run_id,
                    &context,
                    result.pr_url.as_deref(),
                ) {
                    // Issue write-back is best-effort: a GitHub API timeout or
                    // issue-tracker failure must NOT invalidate a successful
                    // push+PR. Log the error and keep the publish as "published".
                    tracing::warn!(
                        run_id,
                        error = %err,
                        "issue write-back failed — publish remains successful"
                    );
                }
            }
            Ok(result)
        }
        Err(err) => {
            let err_msg = err.to_string();
            let pr_url = current_pr_url(conn, run_id)?.or(context.pr_url.clone());
            let updated = Utc::now().to_rfc3339();

            // Use structured classification to determine retryability.
            let push_kind = git_ops::classify_push_error(&err_msg);
            let publish_status = if matches!(push_kind, git_ops::PushFailureKind::PermissionDenied)
            {
                "failed" // Non-retryable
            } else {
                "pending_retry" // Retryable on next startup
            };

            runs_repo::update_publish(
                conn,
                run_id,
                publish_status,
                Some(&err_msg),
                Some(&commit_sha),
                pr_url.as_deref(),
                None,
                &updated,
            )?;
            emit_publish_event(
                conn,
                run_id,
                publish_status,
                pr_url.as_deref(),
                Some(&err_msg),
            )?;
            Ok(PublishResult {
                run_id: run_id.to_string(),
                publish_status: publish_status.to_string(),
                final_commit_sha: Some(commit_sha),
                pr_url,
                published_at: None,
                error: Some(err_msg),
            })
        }
    }
}

#[derive(Debug, Clone)]
struct PublishContext {
    objective: String,
    created_at: String,
    conversation_id: String,
    conversation_branch: String,
    issue_id: Option<String>,
    issue_external_id: Option<String>,
    issue_provider: Option<String>,
    final_commit_sha: Option<String>,
    pr_url: Option<String>,
}

impl PublishContext {
    fn load(conn: &Connection, run_id: &str) -> GroveResult<Self> {
        let row = conn.query_row(
            "SELECT r.objective, r.created_at, r.conversation_id, c.branch_name,
                    r.final_commit_sha, r.pr_url
             FROM runs r
             JOIN conversations c ON r.conversation_id = c.id
             WHERE r.id = ?1",
            [run_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            },
        )?;
        let issue_row: Option<(String, String, String)> = conn
            .query_row(
                "SELECT id, external_id, provider FROM issues WHERE run_id = ?1 LIMIT 1",
                [run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;

        Ok(Self {
            objective: row.0,
            created_at: row.1,
            conversation_id: row.2,
            conversation_branch: row.3.unwrap_or_default(),
            issue_id: issue_row.as_ref().map(|v| v.0.clone()),
            issue_external_id: issue_row.as_ref().map(|v| v.1.clone()),
            issue_provider: issue_row.as_ref().map(|v| v.2.clone()),
            final_commit_sha: row.4,
            pr_url: row.5,
        })
    }
}

fn resolve_run_worktree(
    conn: &Connection,
    project_root: &Path,
    run_id: &str,
) -> GroveResult<PathBuf> {
    let row: (Option<String>, Option<String>) = conn.query_row(
        "SELECT c.worktree_path, r.conversation_id
         FROM runs r
         LEFT JOIN conversations c ON r.conversation_id = c.id
         WHERE r.id = ?1",
        [run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if let Some(path) = row.0.filter(|path| Path::new(path).is_dir()) {
        return Ok(PathBuf::from(path));
    }
    let conv_id = row
        .1
        .ok_or_else(|| GroveError::Runtime(format!("run {run_id} has no conversation")))?;
    let worktrees_base = crate::config::grove_dir(project_root).join("worktrees");
    Ok(crate::worktree::paths::conv_worktree_path(
        &worktrees_base,
        &conv_id,
    ))
}

fn ensure_publish_preflight(
    conn: &Connection,
    run_id: &str,
    run_worktree_path: &Path,
    context: &PublishContext,
) -> GroveResult<()> {
    if !context.conversation_branch.trim().is_empty() {
        let branch = git_ops::git_current_branch(run_worktree_path)?;
        if branch != context.conversation_branch {
            return Err(GroveError::Runtime(format!(
                "publish preflight failed: worktree is on '{branch}', expected '{}'",
                context.conversation_branch
            )));
        }
    }

    let other_active: Option<String> = conn
        .query_row(
            "SELECT id FROM runs
             WHERE conversation_id = ?1
               AND id != ?2
               AND state IN ('planning','executing','waiting_for_gate','verifying','publishing','merging')
             LIMIT 1",
            params![context.conversation_id, run_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(active_id) = other_active {
        return Err(GroveError::Runtime(format!(
            "publish preflight failed: conversation has another active run {active_id}"
        )));
    }

    // Note: conflict markers are checked separately by ensure_no_conflict_markers()
    // which scans file contents directly — more precise than `git diff --check`
    // which also rejects trailing whitespace and causes false positives.

    Ok(())
}

fn ensure_no_conflict_markers(run_worktree_path: &Path, files: &[String]) -> GroveResult<()> {
    const MARKERS: [&[u8]; 3] = [b"<<<<<<<", b"=======", b">>>>>>>"];

    for rel_path in files {
        let path = run_worktree_path.join(rel_path);
        let Ok(contents) = std::fs::read(&path) else {
            continue;
        };
        if MARKERS.iter().any(|marker| {
            contents
                .windows(marker.len())
                .any(|window| window == *marker)
        }) {
            return Err(GroveError::Runtime(format!(
                "publish preflight failed: unresolved conflict markers in {}",
                rel_path
            )));
        }
    }

    Ok(())
}

fn working_tree_changes(run_worktree_path: &Path) -> GroveResult<Vec<String>> {
    let mut files: Vec<String> = git_ops::git_diff_working_tree(run_worktree_path)?
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

fn detect_existing_run_commit(run_worktree_path: &Path, run_id: &str) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.args(["log", "-n", "50", "--format=%H%x1f%s%x1f%b%x1e"])
        .current_dir(run_worktree_path);
    let out = run_command(&mut cmd, "git log").ok()?;
    if !out.status.success() {
        return None;
    }
    let subject_marker = format!("[run: {run_id}");
    let body_marker = format!("Grove-Run: {run_id}");
    let text = String::from_utf8_lossy(&out.stdout);
    for record in text.split('\u{1e}') {
        let trimmed = record.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.split('\u{1f}');
        let Some(sha) = parts.next().map(str::trim) else {
            continue;
        };
        let subject = parts.next().unwrap_or_default();
        let body = parts.next().unwrap_or_default();
        if subject.contains(&subject_marker) || body.contains(&body_marker) {
            return Some(sha.to_string());
        }
    }
    None
}

fn commit_run_changes(
    run_worktree_path: &Path,
    run_id: &str,
    context: &PublishContext,
) -> GroveResult<String> {
    git_ops::git_add_all(run_worktree_path)?;
    let (subject, body) = build_commit_message(run_id, context);
    let mut cmd = Command::new("git");
    cmd.args(["commit", "-m", &subject, "-m", &body])
        .current_dir(run_worktree_path);
    let out = run_command_write(&mut cmd, "git commit")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(GroveError::Runtime(format!("git commit failed: {stderr}")));
    }
    git_ops::git_rev_parse_head(run_worktree_path)
}

fn build_commit_message(run_id: &str, context: &PublishContext) -> (String, String) {
    let prefix = context
        .issue_external_id
        .as_deref()
        .map(|issue| format!("{issue}: "))
        .unwrap_or_else(|| format!("run-{}: ", short_run_id(run_id)));
    let max_subject_len = 72usize.saturating_sub(prefix.len());
    let summary = summarize_subject(&context.objective, max_subject_len);
    let subject = format!("{prefix}{summary}");
    let body = format!(
        "Objective: {}\n\nGrove-Run: {}\nGrove-Conversation: {}\n",
        context.objective, run_id, context.conversation_id
    );
    (subject, body)
}

fn summarize_subject(input: &str, max_len: usize) -> String {
    let compact = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_len {
        compact
    } else {
        compact
            .chars()
            .take(max_len.saturating_sub(1))
            .collect::<String>()
            + "…"
    }
}

#[allow(clippy::too_many_arguments)]
fn publish_remote(
    conn: &mut Connection,
    cfg: &GroveConfig,
    run_id: &str,
    run_worktree_path: &Path,
    context: &PublishContext,
    commit_sha: &str,
    provider: Option<&dyn crate::providers::Provider>,
    model: Option<&str>,
) -> GroveResult<PublishResult> {
    let branch = git_ops::git_current_branch(run_worktree_path)?;
    push_with_recovery(
        run_worktree_path,
        &cfg.publish.remote,
        conn,
        run_id,
        provider,
        cfg,
        model,
    )?;
    let pr_title = build_pr_title(context);
    let pr_body = build_pr_body(conn, run_id, context, &cfg.project.default_branch)?;
    let pr = ensure_pr(
        run_worktree_path,
        &branch,
        &cfg.project.default_branch,
        &pr_title,
        &pr_body,
    )?;
    runs_repo::update_publish(
        conn,
        run_id,
        "pending_retry",
        None,
        Some(commit_sha),
        Some(&pr.url),
        None,
        &Utc::now().to_rfc3339(),
    )?;

    if cfg.publish.comment_on_pr {
        let comment_body =
            build_pr_comment(run_id, context, commit_sha, &pr.url, run_worktree_path)?;
        ensure_pr_comment(run_worktree_path, pr.number, run_id, &comment_body)?;
    }

    Ok(PublishResult {
        run_id: run_id.to_string(),
        publish_status: "published".to_string(),
        final_commit_sha: Some(commit_sha.to_string()),
        pr_url: Some(pr.url),
        published_at: Some(Utc::now().to_rfc3339()),
        error: None,
    })
}

fn post_issue_write_back(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    run_id: &str,
    context: &PublishContext,
    pr_url: Option<&str>,
) -> GroveResult<()> {
    let Some(issue_id) = context.issue_id.clone() else {
        return Ok(());
    };
    let marker = format!("{MARKER_PREFIX}{run_id} -->");

    if let Some(provider) = context.issue_provider.as_deref() {
        if provider == "github" {
            let Some(external_id) = context.issue_external_id.as_deref() else {
                return Ok(());
            };
            if github_issue_has_marker(project_root, external_id, run_id)? {
                return Ok(());
            }
        } else {
            let exists: Option<i64> = conn
                .query_row(
                    "SELECT id FROM issue_comments WHERE issue_id = ?1 AND body LIKE ?2 LIMIT 1",
                    params![issue_id, format!("%{marker}%")],
                    |r| r.get(0),
                )
                .optional()?;
            if exists.is_some() {
                return Ok(());
            }
        }
    }

    let cost_usd: f64 = conn
        .query_row(
            "SELECT cost_used_usd FROM runs WHERE id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap_or(0.0);
    let duration_secs = chrono::DateTime::parse_from_rfc3339(&context.created_at)
        .map(|start| {
            Utc::now()
                .signed_duration_since(start.with_timezone(&Utc))
                .num_seconds()
                .max(0) as u64
        })
        .unwrap_or(0);
    let agent_count: usize = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE run_id = ?1",
            [run_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mut wb_ctx = WriteBackContext {
        run_id: run_id.to_string(),
        issue_id,
        pr_url: pr_url.map(ToOwned::to_owned),
        cost_usd,
        duration_secs,
        agent_count,
        error: None,
    };
    wb_ctx.pr_url = pr_url.map(|v| v.to_string());
    let comment = format!(
        "{}\n\n{}",
        write_back::render_template(&cfg.tracker.write_back.comment_template, &wb_ctx),
        marker
    );
    write_back::post_comment_strict(conn, cfg, project_root, &wb_ctx, &comment, "commented")?;
    Ok(())
}

fn emit_publish_event(
    conn: &Connection,
    run_id: &str,
    publish_status: &str,
    pr_url: Option<&str>,
    error: Option<&str>,
) -> GroveResult<()> {
    events::emit(
        conn,
        run_id,
        None,
        "run_publish_state_changed",
        serde_json::json!({
            "publish_status": publish_status,
            "pr_url": pr_url,
            "error": error,
        }),
    )
}

fn build_pr_title(context: &PublishContext) -> String {
    let prefix = context
        .issue_external_id
        .as_deref()
        .map(|id| format!("{id}: "))
        .unwrap_or_default();
    format!(
        "{prefix}{}",
        summarize_subject(&context.objective, 72usize.saturating_sub(prefix.len()))
    )
}

fn build_pr_body(
    conn: &Connection,
    run_id: &str,
    context: &PublishContext,
    target_branch: &str,
) -> GroveResult<String> {
    let marker = format!(
        "{MARKER_PREFIX}conversation:{} -->",
        context.conversation_id
    );
    let mut stmt = conn.prepare(
        "SELECT id, objective, publish_status, final_commit_sha
         FROM runs
         WHERE conversation_id = ?1 AND state = 'completed'
         ORDER BY created_at DESC
         LIMIT 10",
    )?;
    let recent_runs: Vec<(String, String, String, Option<String>)> = stmt
        .query_map([&context.conversation_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;

    let mut lines = vec![
        format!("# Grove Conversation {}", context.conversation_id),
        String::new(),
        format!("Target branch: `{}`", target_branch),
        format!("Latest run: `{}`", short_run_id(run_id)),
        String::new(),
        "## Recent Runs".to_string(),
    ];
    for (id, objective, status, sha) in recent_runs {
        lines.push(format!(
            "- `{}` [{}] {}{}",
            short_run_id(&id),
            status,
            objective,
            sha.as_deref()
                .map(|value| format!(" (`{}`)", &value[..value.len().min(8)]))
                .unwrap_or_default()
        ));
    }
    lines.push(String::new());
    lines.push(marker);
    Ok(lines.join("\n"))
}

fn build_pr_comment(
    run_id: &str,
    context: &PublishContext,
    commit_sha: &str,
    pr_url: &str,
    run_worktree_path: &Path,
) -> GroveResult<String> {
    let changed_files = changed_files_for_commit(run_worktree_path, commit_sha);
    let marker = format!("{MARKER_PREFIX}{run_id} -->");
    Ok(format!(
        "## Grove Run `{}`\n\n- Objective: {}\n- Commit: `{}`\n- PR: {}\n{}\n\n{}",
        short_run_id(run_id),
        context.objective,
        &commit_sha[..commit_sha.len().min(8)],
        pr_url,
        if changed_files.is_empty() {
            "- Changed files: none".to_string()
        } else {
            format!(
                "- Changed files:\n{}",
                changed_files
                    .into_iter()
                    .map(|path| format!("  - `{path}`"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        },
        marker
    ))
}

fn changed_files_for_commit(run_worktree_path: &Path, commit_sha: &str) -> Vec<String> {
    let mut cmd = Command::new("git");
    cmd.args([
        "diff-tree",
        "--no-commit-id",
        "--name-only",
        "-r",
        commit_sha,
    ])
    .current_dir(run_worktree_path);
    let out = run_command(&mut cmd, "git diff-tree").ok();
    let Some(out) = out else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Push with two-tier recovery:
/// - Tier 1 (mechanical): ff-only pull and retry on non-fast-forward
/// - Tier 2 (agent): invoke push recovery agent if Tier 1 fails
#[allow(clippy::too_many_arguments)]
fn push_with_recovery(
    run_worktree_path: &Path,
    remote: &str,
    conn: &mut Connection,
    run_id: &str,
    provider: Option<&dyn crate::providers::Provider>,
    _cfg: &GroveConfig,
    model: Option<&str>,
) -> GroveResult<()> {
    // Initial push attempt.
    let mut cmd = Command::new("git");
    cmd.args(["push", "--set-upstream", remote, "HEAD"])
        .current_dir(run_worktree_path);
    let out = run_command_write(&mut cmd, "git push")?;
    if out.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let failure_kind = git_ops::classify_push_error(&stderr);

    // Non-retryable errors — fail immediately.
    if matches!(failure_kind, git_ops::PushFailureKind::PermissionDenied) {
        return Err(GroveError::Runtime(format!(
            "git push failed (permission denied — not retryable): {stderr}"
        )));
    }

    // Tier 1: ff-only pull and retry for non-fast-forward.
    if matches!(failure_kind, git_ops::PushFailureKind::NonFastForward) {
        tracing::info!("push rejected (non-fast-forward) — attempting ff-only pull");
        let mut ff_cmd = Command::new("git");
        ff_cmd
            .args(["pull", "--ff-only", remote])
            .current_dir(run_worktree_path);
        let ff_out = run_command_write(&mut ff_cmd, "git pull --ff-only");

        if let Ok(ref ff_result) = ff_out {
            if ff_result.status.success() {
                // Retry push after ff-only pull.
                let mut retry_cmd = Command::new("git");
                retry_cmd
                    .args(["push", "--set-upstream", remote, "HEAD"])
                    .current_dir(run_worktree_path);
                let retry_out = run_command_write(&mut retry_cmd, "git push (retry)")?;
                if retry_out.status.success() {
                    tracing::info!("push succeeded after ff-only pull");
                    return Ok(());
                }
                // Fall through to Tier 2 if retry also failed.
            }
        }
    }

    // Tier 2: agent recovery (push-scoped).
    if let Some(prov) = provider {
        let _ = crate::events::emit(
            conn,
            run_id,
            None,
            crate::events::event_types::GIT_PUSH_RECOVERY_STARTED,
            serde_json::json!({
                "error": stderr,
                "failure_kind": format!("{:?}", failure_kind),
            }),
        );

        let instructions = crate::orchestrator::engine::build_push_recovery_instructions(
            &format!("git push --set-upstream {remote} HEAD"),
            &stderr,
            run_worktree_path,
            &git_ops::git_current_branch(run_worktree_path).unwrap_or_default(),
        );

        let request = crate::providers::ProviderRequest {
            objective: format!("Fix git push failure: {}", &stderr[..stderr.len().min(200)]),
            role: "builder".to_string(),
            worktree_path: run_worktree_path.to_string_lossy().to_string(),
            instructions,
            model: model.map(|m| m.to_string()),
            allowed_tools: crate::agents::AgentType::Builder.allowed_tools(),
            timeout_override: Some(120),
            provider_session_id: None,
            log_dir: None,
            grove_session_id: None,
            input_handle_callback: None,
            mcp_config_path: None,
            conversation_id: None,
        };

        let agent_result = prov.execute(&request);

        // Record cost regardless of success/failure.
        if let Ok(ref response) = agent_result {
            let _ = crate::providers::budget_meter::record(conn, run_id, response);
        }

        if agent_result.is_ok() {
            // Final push retry after agent recovery.
            let mut final_cmd = Command::new("git");
            final_cmd
                .args(["push", "--set-upstream", remote, "HEAD"])
                .current_dir(run_worktree_path);
            let final_out = run_command_write(&mut final_cmd, "git push (post-recovery)")?;
            if final_out.status.success() {
                tracing::info!("push succeeded after agent recovery");
                let _ = crate::events::emit(
                    conn,
                    run_id,
                    None,
                    crate::events::event_types::GIT_PUSH_RECOVERY_COMPLETED,
                    serde_json::json!({
                        "recovery": "agent fixed the issue",
                    }),
                );
                return Ok(());
            }
            let final_stderr = String::from_utf8_lossy(&final_out.stderr)
                .trim()
                .to_string();
            let _ = crate::events::emit(
                conn,
                run_id,
                None,
                crate::events::event_types::GIT_PUSH_RECOVERY_FAILED,
                serde_json::json!({
                    "error": final_stderr,
                    "stage": "post-recovery push",
                }),
            );
            return Err(GroveError::Runtime(format!(
                "git push failed after agent recovery: {final_stderr}"
            )));
        } else {
            let agent_err = agent_result.unwrap_err().to_string();
            let _ = crate::events::emit(
                conn,
                run_id,
                None,
                crate::events::event_types::GIT_PUSH_RECOVERY_FAILED,
                serde_json::json!({
                    "error": agent_err,
                    "stage": "agent execution",
                }),
            );
        }
    }

    Err(GroveError::Runtime(format!("git push failed: {stderr}")))
}

#[derive(Debug, Clone)]
struct PrInfo {
    number: u64,
    url: String,
}

fn ensure_pr(
    run_worktree_path: &Path,
    branch: &str,
    target_branch: &str,
    title: &str,
    body: &str,
) -> GroveResult<PrInfo> {
    if let Some(existing) = find_existing_pr(run_worktree_path, branch, target_branch)? {
        update_pr_body(run_worktree_path, existing.number, body)?;
        return Ok(existing);
    }

    let body_file = std::env::temp_dir().join(format!("grove_pr_{}.md", branch.replace('/', "_")));
    std::fs::write(&body_file, body)?;
    let mut cmd = gh_command();
    cmd.args([
        "pr",
        "create",
        "--base",
        target_branch,
        "--head",
        branch,
        "--title",
        title,
        "--body-file",
        body_file.to_string_lossy().as_ref(),
    ])
    .current_dir(run_worktree_path);
    let out = run_command_write(&mut cmd, "gh pr create")?;
    let _ = std::fs::remove_file(&body_file);
    if out.status.success() {
        return find_existing_pr(run_worktree_path, branch, target_branch)?.ok_or_else(|| {
            GroveError::Runtime("gh pr create succeeded but PR lookup failed".to_string())
        });
    }

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    if stderr.contains("already exists") {
        return find_existing_pr(run_worktree_path, branch, target_branch)?
            .ok_or_else(|| GroveError::Runtime("PR already exists but lookup failed".to_string()));
    }
    Err(GroveError::Runtime(format!(
        "gh pr create failed: {stderr}"
    )))
}

fn find_existing_pr(
    run_worktree_path: &Path,
    branch: &str,
    target_branch: &str,
) -> GroveResult<Option<PrInfo>> {
    let mut cmd = gh_command();
    cmd.args([
        "pr",
        "list",
        "--head",
        branch,
        "--base",
        target_branch,
        "--state",
        "open",
        "--json",
        "number,url",
    ])
    .current_dir(run_worktree_path);
    let out = run_command(&mut cmd, "gh pr list")?;
    if !out.status.success() {
        return Err(GroveError::Runtime(format!(
            "gh pr list failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let parsed: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| GroveError::Runtime(format!("failed to parse gh pr list output: {e}")))?;
    let Some(first) = parsed.as_array().and_then(|items| items.first()) else {
        return Ok(None);
    };
    Ok(Some(PrInfo {
        number: first["number"].as_u64().unwrap_or(0),
        url: first["url"].as_str().unwrap_or_default().to_string(),
    }))
}

fn update_pr_body(run_worktree_path: &Path, number: u64, body: &str) -> GroveResult<()> {
    let body_file = std::env::temp_dir().join(format!("grove_pr_body_{number}.md"));
    std::fs::write(&body_file, body)?;
    let mut cmd = gh_command();
    cmd.args([
        "pr",
        "edit",
        &number.to_string(),
        "--body-file",
        body_file.to_string_lossy().as_ref(),
    ])
    .current_dir(run_worktree_path);
    let out = run_command_write(&mut cmd, "gh pr edit")?;
    let _ = std::fs::remove_file(&body_file);
    if !out.status.success() {
        return Err(GroveError::Runtime(format!(
            "gh pr edit failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

fn ensure_pr_comment(
    run_worktree_path: &Path,
    pr_number: u64,
    run_id: &str,
    body: &str,
) -> GroveResult<()> {
    if pr_has_marker(run_worktree_path, pr_number, run_id)? {
        return Ok(());
    }
    let mut cmd = gh_command();
    cmd.args(["pr", "comment", &pr_number.to_string(), "--body", body])
        .current_dir(run_worktree_path);
    let out = run_command_write(&mut cmd, "gh pr comment")?;
    if !out.status.success() {
        return Err(GroveError::Runtime(format!(
            "gh pr comment failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

fn pr_has_marker(run_worktree_path: &Path, pr_number: u64, run_id: &str) -> GroveResult<bool> {
    let mut cmd = gh_command();
    cmd.args(["pr", "view", &pr_number.to_string(), "--json", "comments"])
        .current_dir(run_worktree_path);
    let out = run_command(&mut cmd, "gh pr view")?;
    if !out.status.success() {
        return Ok(false);
    }
    let parsed: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| GroveError::Runtime(format!("failed to parse gh pr view comments: {e}")))?;
    Ok(parsed["comments"]
        .as_array()
        .map(|comments| {
            comments.iter().any(|comment| {
                comment["body"]
                    .as_str()
                    .map(|body| body.contains(&format!("{MARKER_PREFIX}{run_id} -->")))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false))
}

fn github_issue_has_marker(
    project_root: &Path,
    external_id: &str,
    run_id: &str,
) -> GroveResult<bool> {
    let mut cmd = gh_command();
    cmd.args(["issue", "view", external_id, "--json", "comments"])
        .current_dir(project_root);
    let out = run_command(&mut cmd, "gh issue view")?;
    if !out.status.success() {
        return Ok(false);
    }
    let parsed: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| GroveError::Runtime(format!("failed to parse gh issue comments: {e}")))?;
    Ok(parsed["comments"]
        .as_array()
        .map(|comments| {
            comments.iter().any(|comment| {
                comment["body"]
                    .as_str()
                    .map(|body| body.contains(&format!("{MARKER_PREFIX}{run_id} -->")))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false))
}

fn short_run_id(run_id: &str) -> &str {
    &run_id[..run_id.len().min(8)]
}

fn current_pr_url(conn: &Connection, run_id: &str) -> GroveResult<Option<String>> {
    conn.query_row("SELECT pr_url FROM runs WHERE id = ?1", [run_id], |r| {
        r.get::<_, Option<String>>(0)
    })
    .optional()
    .map(|value| value.flatten())
    .map_err(Into::into)
}

/// Run a subprocess with the read timeout (30s). Use for queries like git log, gh pr list, etc.
fn run_command(cmd: &mut Command, label: &str) -> GroveResult<Output> {
    run_command_with_timeout(cmd, label, READ_COMMAND_TIMEOUT_SECS)
}

/// Run a subprocess with the write timeout (60s). Use for push, PR create, commit, etc.
fn run_command_write(cmd: &mut Command, label: &str) -> GroveResult<Output> {
    run_command_with_timeout(cmd, label, WRITE_COMMAND_TIMEOUT_SECS)
}

fn command_path() -> String {
    let shell = crate::capability::shell_path();
    // Include the process PATH so test-injected binaries (e.g. fake `gh`)
    // and PATH modifications made by the host process are honored, with
    // the login-shell PATH appended as a fallback for GUI contexts
    // (e.g. Tauri) where the process PATH may be minimal.
    let process = std::env::var("PATH").unwrap_or_default();
    if !process.is_empty() {
        let paths = std::env::split_paths(&process).chain(std::env::split_paths(shell));
        if let Ok(joined) = std::env::join_paths(paths) {
            return joined.to_string_lossy().to_string();
        }
        return process;
    }
    shell.to_string()
}

fn resolve_command_on_path(binary: &str) -> PathBuf {
    let path = command_path();

    #[cfg(windows)]
    {
        for dir in std::env::split_paths(&path) {
            for suffix in [".exe", ".cmd", ".bat", ".com"] {
                let candidate = dir.join(format!("{binary}{suffix}"));
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }

    which::which_in(binary, Some(&path), ".").unwrap_or_else(|_| PathBuf::from(binary))
}

fn gh_command() -> Command {
    Command::new(resolve_command_on_path("gh"))
}

fn run_command_with_timeout(
    cmd: &mut Command,
    label: &str,
    timeout_secs: u64,
) -> GroveResult<Output> {
    cmd.env("PATH", command_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .map_err(|e| GroveError::Runtime(format!("{label} failed to start: {e}")))?;
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);

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
                        "{label} timed out after {timeout_secs}s",
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{LazyLock, Mutex};

    use super::*;
    use crate::config::{TrackerMode, defaults::default_config};
    use crate::db::{self, DbHandle};
    use tempfile::TempDir;

    static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    struct EnvVarGuard {
        key: &'static str,
        old_value: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: std::ffi::OsString) -> Self {
            let old_value = std::env::var_os(key);
            unsafe {
                std::env::set_var(key, value);
            }
            Self { key, old_value }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(value) = &self.old_value {
                    std::env::set_var(self.key, value);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    #[test]
    fn commit_message_uses_issue_prefix_when_present() {
        let ctx = PublishContext {
            objective: "implement the auth endpoint with tests".to_string(),
            created_at: Utc::now().to_rfc3339(),
            conversation_id: "conv1".to_string(),
            conversation_branch: "grove/s_1".to_string(),
            issue_id: Some("github:123".to_string()),
            issue_external_id: Some("123".to_string()),
            issue_provider: Some("github".to_string()),
            final_commit_sha: None,
            pr_url: None,
        };
        let (subject, body) = build_commit_message("run_12345678", &ctx);
        assert!(subject.starts_with("123: "));
        assert!(body.contains("Grove-Run: run_12345678"));
        assert!(body.contains("Grove-Conversation: conv1"));
    }

    #[test]
    fn subject_is_truncated() {
        let short = summarize_subject(
            "a very long objective that should be truncated to fit commit subject limits",
            20,
        );
        assert!(short.chars().count() <= 20);
    }

    #[test]
    fn gh_command_resolves_process_path_shim_first() {
        let _guard = ENV_LOCK.lock().unwrap();
        let bin_dir = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        let gh_path = bin_dir.path().join("gh");
        #[cfg(windows)]
        let gh_path = bin_dir.path().join("gh.cmd");
        fs::write(&gh_path, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&gh_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&gh_path, perms).unwrap();
        }

        let old_path = std::env::var_os("PATH");
        let mut paths = vec![bin_dir.path().to_path_buf()];
        if let Some(old_path) = old_path.as_ref() {
            paths.extend(std::env::split_paths(old_path));
        }
        let test_path = std::env::join_paths(paths).expect("join PATH");
        let _path_guard = EnvVarGuard::set("PATH", test_path);

        assert_eq!(resolve_command_on_path("gh"), gh_path);
    }

    #[test]
    fn auto_publish_disabled_still_creates_local_commit() {
        let repo = TestRepo::new(false);
        let (mut conn, run_id) = repo.seed_run(None);
        fs::write(repo.repo.path().join("feature.txt"), "new work\n").unwrap();

        let mut cfg = default_config();
        cfg.publish.enabled = true;
        cfg.publish.auto_on_success = false;

        let result = publish_run(
            &mut conn,
            &cfg,
            repo.repo.path(),
            &run_id,
            repo.repo.path(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(result.publish_status, "pending_retry");
        assert!(result.final_commit_sha.is_some());
        assert_eq!(
            repo.git_stdout(&["status", "--porcelain", "--", "feature.txt"])
                .trim(),
            ""
        );

        let row: (String, Option<String>) = conn
            .query_row(
                "SELECT publish_status, final_commit_sha FROM runs WHERE id = ?1",
                [&run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, "pending_retry");
        assert_eq!(row.1, result.final_commit_sha);
    }

    #[test]
    fn existing_subject_style_run_commit_is_not_treated_as_no_changes() {
        let repo = TestRepo::new(false);
        let (mut conn, run_id) = repo.seed_run(None);

        fs::write(repo.repo.path().join("feature.txt"), "new work\n").unwrap();
        git_ok(repo.repo.path(), &["add", "feature.txt"]);
        git_ok(
            repo.repo.path(),
            &[
                "commit",
                "-m",
                &format!(
                    "grove: builder [run: {}, session: sess_test_12345678]",
                    run_id
                ),
            ],
        );

        let mut cfg = default_config();
        cfg.publish.enabled = true;
        cfg.publish.auto_on_success = false;

        let result = publish_run(
            &mut conn,
            &cfg,
            repo.repo.path(),
            &run_id,
            repo.repo.path(),
            None,
            None,
        )
        .unwrap();

        assert_eq!(result.publish_status, "pending_retry");
        assert!(result.final_commit_sha.is_some());
    }

    #[test]
    fn conflict_markers_in_untracked_files_block_publish() {
        let repo = TestRepo::new(false);
        let (mut conn, run_id) = repo.seed_run(None);
        fs::write(
            repo.repo.path().join("broken.txt"),
            "<<<<<<< ours\nbroken\n=======\nother\n>>>>>>> theirs\n",
        )
        .unwrap();

        let mut cfg = default_config();
        cfg.publish.enabled = true;
        cfg.publish.auto_on_success = false;

        let err = publish_run(
            &mut conn,
            &cfg,
            repo.repo.path(),
            &run_id,
            repo.repo.path(),
            None,
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("unresolved conflict markers"));
        let stored: (String, Option<String>) = conn
            .query_row(
                "SELECT publish_status, final_commit_sha FROM runs WHERE id = ?1",
                [&run_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored.0, "pending_retry");
        assert_eq!(stored.1, None);
    }

    #[test]
    fn issue_comment_failure_preserves_published_status_and_pr_url() {
        let _guard = ENV_LOCK.lock().unwrap();
        let repo = TestRepo::new(true);
        let (mut conn, run_id) = repo.seed_run(Some(("github", "123")));
        fs::write(
            repo.repo.path().join("feature.txt"),
            "remote publish work\n",
        )
        .unwrap();

        let bin_dir = tempfile::tempdir().unwrap();
        let pr_file = bin_dir.path().join("open_pr");
        #[cfg(unix)]
        let gh_path = bin_dir.path().join("gh");
        #[cfg(windows)]
        let gh_path = bin_dir.path().join("gh.cmd");
        #[cfg(unix)]
        fs::write(
            &gh_path,
            format!(
                "#!/bin/sh\n\
case \"$1 $2\" in\n\
  \"pr list\")\n\
    if [ -f \"{}\" ]; then\n\
      printf '[{{\"number\":1,\"url\":\"https://example.test/pr/1\"}}]'\n\
    else\n\
      printf '[]'\n\
    fi\n\
    ;;\n\
  \"pr create\")\n\
    touch \"{}\"\n\
    printf 'https://example.test/pr/1\\n'\n\
    ;;\n\
  \"pr edit\") exit 0 ;;\n\
  \"pr view\") printf '{{\"comments\":[]}}' ;;\n\
  \"pr comment\") exit 0 ;;\n\
  \"issue view\") printf '{{\"comments\":[]}}' ;;\n\
  \"issue comment\") echo 'issue comment failed' >&2; exit 1 ;;\n\
  *) echo \"unsupported gh invocation: $1 $2\" >&2; exit 1 ;;\n\
esac\n",
                pr_file.display(),
                pr_file.display(),
            ),
        )
        .unwrap();
        #[cfg(windows)]
        fs::write(
            &gh_path,
            format!(
                "@echo off\r\n\
if \"%1 %2\"==\"pr list\" (\r\n\
  if exist \"{}\" (\r\n\
    echo [{{\"number\":1,\"url\":\"https://example.test/pr/1\"}}]\r\n\
  ) else (\r\n\
    echo []\r\n\
  )\r\n\
  exit /b 0\r\n\
)\r\n\
if \"%1 %2\"==\"pr create\" (\r\n\
  type nul > \"{}\"\r\n\
  echo https://example.test/pr/1\r\n\
  exit /b 0\r\n\
)\r\n\
if \"%1 %2\"==\"pr edit\" exit /b 0\r\n\
if \"%1 %2\"==\"pr view\" (\r\n\
  echo {{\"comments\":[]}}\r\n\
  exit /b 0\r\n\
)\r\n\
if \"%1 %2\"==\"pr comment\" exit /b 0\r\n\
if \"%1 %2\"==\"issue view\" (\r\n\
  echo {{\"comments\":[]}}\r\n\
  exit /b 0\r\n\
)\r\n\
if \"%1 %2\"==\"issue comment\" (\r\n\
  echo issue comment failed 1>&2\r\n\
  exit /b 1\r\n\
)\r\n\
echo unsupported gh invocation: %1 %2 1>&2\r\n\
exit /b 1\r\n",
                pr_file.display(),
                pr_file.display(),
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&gh_path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&gh_path, perms).unwrap();
        }

        let old_path = std::env::var_os("PATH");
        let mut paths = vec![bin_dir.path().to_path_buf()];
        if let Some(old_path) = old_path.as_ref() {
            paths.extend(std::env::split_paths(old_path));
        }
        let test_path = std::env::join_paths(paths).expect("join PATH");
        let _path_guard = EnvVarGuard::set("PATH", test_path);

        let mut cfg = default_config();
        cfg.publish.enabled = true;
        cfg.publish.auto_on_success = true;
        cfg.publish.comment_on_issue = true;
        cfg.publish.comment_on_pr = true;
        cfg.tracker.mode = TrackerMode::GitHub;
        cfg.tracker.github.enabled = true;

        let result = publish_run(
            &mut conn,
            &cfg,
            repo.repo.path(),
            &run_id,
            repo.repo.path(),
            None,
            None,
        )
        .unwrap();

        // Issue write-back is best-effort: even when `gh issue comment` fails,
        // the publish should succeed because the push + PR creation worked.
        assert_eq!(result.publish_status, "published");
        assert_eq!(result.pr_url.as_deref(), Some("https://example.test/pr/1"));
        assert!(result.final_commit_sha.is_some());
        assert!(
            result.error.is_none(),
            "error should be None for best-effort write-back"
        );

        let row: (String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT publish_status, pr_url, final_commit_sha FROM runs WHERE id = ?1",
                [&run_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row.0, "published");
        assert_eq!(row.1.as_deref(), Some("https://example.test/pr/1"));
        assert_eq!(row.2, result.final_commit_sha);
    }

    struct TestRepo {
        repo: TempDir,
        _remote: Option<TempDir>,
        branch: String,
        conversation_id: String,
        run_id: String,
    }

    impl TestRepo {
        fn new(with_remote: bool) -> Self {
            let repo = tempfile::tempdir().unwrap();
            git_ok(repo.path(), &["init", "--initial-branch=main"]);
            git_ok(repo.path(), &["config", "user.email", "grove@example.test"]);
            git_ok(repo.path(), &["config", "user.name", "Grove Tests"]);
            fs::write(repo.path().join("README.md"), "seed\n").unwrap();
            git_ok(repo.path(), &["add", "README.md"]);
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

            Self {
                repo,
                _remote: remote,
                branch,
                conversation_id: "conv_test".to_string(),
                run_id: "run_test_12345678".to_string(),
            }
        }

        fn seed_run(&self, issue: Option<(&str, &str)>) -> (Connection, String) {
            db::initialize(self.repo.path()).unwrap();
            let handle = DbHandle::new(self.repo.path());
            let conn = handle.connect().unwrap();
            let now = Utc::now().to_rfc3339();

            conn.execute(
                "INSERT INTO conversations (id, project_id, title, state, branch_name, worktree_path, created_at, updated_at)
                 VALUES (?1, 'proj1', 'Test Conversation', 'active', ?2, ?3, ?4, ?4)",
                params![
                    self.conversation_id,
                    self.branch,
                    self.repo.path().to_string_lossy().to_string(),
                    now,
                ],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, publish_status, conversation_id, created_at, updated_at)
                 VALUES (?1, 'Implement publish flow', 'publishing', 0, 0, 'pending_retry', ?2, ?3, ?3)",
                params![self.run_id, self.conversation_id, now],
            )
            .unwrap();

            if let Some((provider, external_id)) = issue {
                let issue_id = format!("{provider}:{external_id}");
                conn.execute(
                    "INSERT INTO issues (
                        id, project_id, title, body, status, provider, external_id, run_id, created_at, updated_at, raw_json
                     ) VALUES (?1, 'proj1', 'Linked issue', NULL, 'open', ?2, ?3, ?4, ?5, ?5, '{}')",
                    params![issue_id, provider, external_id, self.run_id, now],
                )
                .unwrap();
            }

            (conn, self.run_id.clone())
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
}
