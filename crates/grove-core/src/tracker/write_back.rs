use std::path::Path;

use rusqlite::Connection;

use crate::config::GroveConfig;
use crate::db::repositories::issues_repo;
use crate::errors::GroveResult;
use crate::tracker::TrackerBackend;
use crate::tracker::registry::TrackerRegistry;
use crate::tracker::status;

// ── Context passed to write-back functions ────────────────────────────────────

/// All fields needed to render write-back comment templates and drive
/// post-run issue transitions.
#[derive(Debug, Clone)]
pub struct WriteBackContext {
    /// Grove run ID (short form used in messages).
    pub run_id: String,
    /// Composite Grove issue ID: `{provider}:{external_id}` or `grove:{uuid}`.
    pub issue_id: String,
    /// Pull-request URL created during the merge, if any.
    pub pr_url: Option<String>,
    /// Estimated cost of the run in USD.
    pub cost_usd: f64,
    /// Wall-clock duration of the run in seconds.
    pub duration_secs: u64,
    /// Number of agent sessions that ran.
    pub agent_count: usize,
    /// Error message if the run failed; `None` for success.
    pub error: Option<String>,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Called when a run finishes successfully.
///
/// If `tracker.write_back.enabled = true` and the run is linked to an issue:
/// 1. Renders the `comment_template` with the run context.
/// 2. Posts the comment via the provider backend.
/// 3. Stores the comment in `issue_comments` (with `posted_to_provider` flag).
/// 4. Records a `commented` event in `issue_events`.
///
/// If the comment fails (auth/network error) the error is logged as a warning
/// but the function still returns `Ok(())` — write-back failure must never
/// fail the run.
pub fn on_run_complete(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    ctx: WriteBackContext,
) -> GroveResult<()> {
    if !cfg.tracker.write_back.enabled {
        return Ok(());
    }
    if !cfg.tracker.write_back.comment_on_complete {
        return Ok(());
    }

    let body = render_template(&cfg.tracker.write_back.comment_template, &ctx);
    let _ = post_comment(conn, cfg, project_root, &ctx, &body, "commented");
    Ok(())
}

/// Called when a run fails.
///
/// Mirrors `on_run_complete` but uses `failure_template` and is gated on
/// `comment_on_failure`.
pub fn on_run_failed(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    ctx: WriteBackContext,
) -> GroveResult<()> {
    if !cfg.tracker.write_back.enabled {
        return Ok(());
    }
    if !cfg.tracker.write_back.comment_on_failure {
        return Ok(());
    }

    let body = render_template(&cfg.tracker.write_back.failure_template, &ctx);
    let _ = post_comment(conn, cfg, project_root, &ctx, &body, "commented_failure");
    Ok(())
}

pub fn post_comment(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    ctx: &WriteBackContext,
    body: &str,
    event_type: &str,
) -> GroveResult<()> {
    post_comment_with_policy(conn, cfg, project_root, ctx, body, event_type, false)?;
    Ok(())
}

pub fn post_comment_strict(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    ctx: &WriteBackContext,
    body: &str,
    event_type: &str,
) -> GroveResult<()> {
    post_comment_with_policy(conn, cfg, project_root, ctx, body, event_type, true)
}

/// Called after a successful merge/promotion.
///
/// If `tracker.write_back.transition_on_merge` is set, looks up the issue
/// linked to the run and transitions it to the target status.
pub fn on_merge_complete(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    run_id: &str,
    pr_url: Option<&str>,
) -> GroveResult<()> {
    if !cfg.tracker.write_back.enabled {
        return Ok(());
    }

    let target_status = match &cfg.tracker.write_back.transition_on_merge {
        Some(s) if !s.is_empty() => s.clone(),
        _ => return Ok(()),
    };

    // Find the issue linked to this run.
    let issue_id: Option<String> = conn
        .query_row(
            "SELECT id FROM issues WHERE run_id = ?1 LIMIT 1",
            [run_id],
            |r| r.get(0),
        )
        .ok();

    let issue_id = match issue_id {
        Some(id) => id,
        None => {
            tracing::debug!(run_id, "no issue linked to run — skipping merge transition");
            return Ok(());
        }
    };

    // Determine provider from the issue ID prefix.
    let provider = issue_id.split(':').next().unwrap_or("github");
    let registry = TrackerRegistry::from_config(cfg, project_root);

    // Find the external_id to pass to the backend.
    let external_id: Option<String> = conn
        .query_row(
            "SELECT external_id FROM issues WHERE id = ?1",
            [&issue_id],
            |r| r.get(0),
        )
        .ok();

    if let Some(ext_id) = external_id {
        let backends = registry.all_backends();
        let backend = backends.iter().find(|b| b.provider_name() == provider);
        if let Some(b) = backend {
            if let Err(e) = b.transition(&ext_id, &target_status) {
                tracing::warn!(
                    issue_id = %issue_id,
                    target = %target_status,
                    error = %e,
                    "merge transition failed (non-fatal)"
                );
            } else {
                let canonical = status::normalize(provider, &target_status);
                let _ = issues_repo::update_status(conn, &issue_id, &target_status, canonical);
                let _ = issues_repo::record_event(
                    conn,
                    &issue_id,
                    "status_changed",
                    "grove",
                    None,
                    Some(&target_status),
                );
            }
        }
    }

    // If a PR was created, append the URL as a comment.
    if let Some(url) = pr_url {
        let pr_body = format!("PR created: {url}");
        let ctx = WriteBackContext {
            run_id: run_id.to_string(),
            issue_id: issue_id.clone(),
            pr_url: Some(url.to_string()),
            cost_usd: 0.0,
            duration_secs: 0,
            agent_count: 0,
            error: None,
        };
        let _ = post_comment_with_policy(
            conn,
            cfg,
            project_root,
            &ctx,
            &pr_body,
            "pushed_to_provider",
            false,
        );
    }

    Ok(())
}

/// Apply project-level workflow transition when a run starts.
///
/// Transitions the linked issue to the `on_start` status configured for
/// the project. Non-fatal — logs warnings on error.
pub fn on_run_started_project(
    conn: &mut Connection,
    workspace_root: &Path,
    cfg: &GroveConfig,
    project_settings: &crate::db::repositories::projects_repo::ProjectSettings,
    issue: &crate::tracker::Issue,
) -> GroveResult<()> {
    let workflow = match project_settings.workflow_for(&issue.provider) {
        Some(w) => w,
        None => return Ok(()),
    };
    let target = match &workflow.on_start {
        Some(s) if !s.is_empty() => s.clone(),
        _ => return Ok(()),
    };
    apply_project_transition(
        conn,
        workspace_root,
        cfg,
        project_settings,
        issue,
        &target,
        "run_started",
    )
}

/// Apply project-level workflow transition + optional comment when a run completes successfully.
pub fn on_run_completed_project(
    conn: &mut Connection,
    workspace_root: &Path,
    cfg: &GroveConfig,
    project_settings: &crate::db::repositories::projects_repo::ProjectSettings,
    ctx: &WriteBackContext,
) -> GroveResult<()> {
    let (provider, external_id) = split_issue_id(&ctx.issue_id);
    if external_id.is_empty() {
        return Ok(());
    }

    let workflow = match project_settings.workflow_for(provider) {
        Some(w) => w,
        None => return Ok(()),
    };

    if let Some(ref target) = workflow.on_success.clone().filter(|s| !s.is_empty()) {
        let issue = stub_issue(&ctx.issue_id, provider, external_id);
        let _ = apply_project_transition(
            conn,
            workspace_root,
            cfg,
            project_settings,
            &issue,
            target,
            "run_completed",
        );
    }

    if workflow.comment_on_success {
        let body = render_template(
            "Grove run `{run_id}` completed in {duration}s (cost ${cost_usd}). {pr_url}",
            ctx,
        );
        let _ = post_comment_with_policy(conn, cfg, workspace_root, ctx, &body, "commented", false);
    }

    Ok(())
}

/// Apply project-level workflow transition + optional comment when a run fails.
pub fn on_run_failed_project(
    conn: &mut Connection,
    workspace_root: &Path,
    cfg: &GroveConfig,
    project_settings: &crate::db::repositories::projects_repo::ProjectSettings,
    ctx: &WriteBackContext,
) -> GroveResult<()> {
    let (provider, external_id) = split_issue_id(&ctx.issue_id);
    if external_id.is_empty() {
        return Ok(());
    }

    let workflow = match project_settings.workflow_for(provider) {
        Some(w) => w,
        None => return Ok(()),
    };

    if let Some(ref target) = workflow.on_failure.clone().filter(|s| !s.is_empty()) {
        let issue = stub_issue(&ctx.issue_id, provider, external_id);
        let _ = apply_project_transition(
            conn,
            workspace_root,
            cfg,
            project_settings,
            &issue,
            target,
            "run_failed",
        );
    }

    if workflow.comment_on_failure {
        let error = ctx.error.as_deref().unwrap_or("unknown error");
        let body = format!(
            "Grove run `{}` failed after {}s: {}\n\nCost: ${:.2}",
            &ctx.run_id[..ctx.run_id.len().min(8)],
            ctx.duration_secs,
            error,
            ctx.cost_usd,
        );
        let _ = post_comment_with_policy(
            conn,
            cfg,
            workspace_root,
            ctx,
            &body,
            "commented_failure",
            false,
        );
    }

    Ok(())
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Post a comment to the external provider and store it locally.
///
/// Errors from the provider are logged as warnings — the function never
/// propagates them so that write-back failures are always non-fatal.
fn post_comment_with_policy(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    ctx: &WriteBackContext,
    body: &str,
    event_type: &str,
    strict: bool,
) -> GroveResult<()> {
    // Determine external_id and provider from the composite issue_id.
    let mut parts = ctx.issue_id.splitn(2, ':');
    let provider = parts.next().unwrap_or("unknown");
    let external_id = parts.next().unwrap_or("");

    if provider == "grove" || external_id.is_empty() {
        // Native Grove issue — store comment locally only, no external push.
        issues_repo::add_comment(conn, &ctx.issue_id, body, "grove", false)?;
        record_comment_event(conn, &ctx.issue_id, event_type)?;
        return Ok(());
    }

    // Try to post to the external provider.
    let registry = TrackerRegistry::from_config(cfg, project_root);
    let backends = registry.all_backends();
    let backend = backends.iter().find(|b| b.provider_name() == provider);

    let posted = if let Some(b) = backend {
        match b.comment(external_id, body) {
            Ok(_) => {
                tracing::info!(
                    issue_id = %ctx.issue_id,
                    event = event_type,
                    "write-back comment posted to provider"
                );
                true
            }
            Err(e) => {
                if strict {
                    return Err(e);
                }
                tracing::warn!(
                    issue_id = %ctx.issue_id,
                    error = %e,
                    "write-back comment failed (stored locally)"
                );
                false
            }
        }
    } else if strict {
        return Err(crate::errors::GroveError::Runtime(format!(
            "no active backend configured for provider '{provider}'"
        )));
    } else {
        tracing::debug!(
            provider = %provider,
            "no active backend for provider — comment stored locally only"
        );
        false
    };

    // Persist the comment in the local DB only after the provider result is known.
    let author = format!("grove/r_{}", &ctx.run_id[..ctx.run_id.len().min(8)]);
    issues_repo::add_comment(conn, &ctx.issue_id, body, &author, posted)?;
    record_comment_event(conn, &ctx.issue_id, event_type)?;
    Ok(())
}

fn record_comment_event(
    conn: &mut Connection,
    issue_id: &str,
    event_type: &str,
) -> GroveResult<()> {
    let tx = conn.transaction()?;
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO issue_events (issue_id, event_type, actor, created_at)
         VALUES (?1, ?2, 'grove', ?3)",
        rusqlite::params![issue_id, event_type, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// Substitute `{placeholder}` variables in a template string.
///
/// Recognised placeholders:
/// - `{run_id}` — first 8 chars of the run ID
/// - `{pr_url}` — PR URL or `(no PR)`
/// - `{cost_usd}` — 2-decimal cost
/// - `{duration}` — seconds
/// - `{agent_count}` — number of agents
/// - `{error}` — error message or empty string
pub fn render_template(template: &str, ctx: &WriteBackContext) -> String {
    let run_id_short = &ctx.run_id[..ctx.run_id.len().min(8)];
    let pr_url = ctx.pr_url.as_deref().unwrap_or("(no PR)");
    let error = ctx.error.as_deref().unwrap_or("");

    template
        .replace("{run_id}", run_id_short)
        .replace("{pr_url}", pr_url)
        .replace("{cost_usd}", &format!("{:.2}", ctx.cost_usd))
        .replace("{duration}", &ctx.duration_secs.to_string())
        .replace("{agent_count}", &ctx.agent_count.to_string())
        .replace("{error}", error)
}

fn split_issue_id(issue_id: &str) -> (&str, &str) {
    let mut parts = issue_id.splitn(2, ':');
    let provider = parts.next().unwrap_or("unknown");
    let external_id = parts.next().unwrap_or("");
    (provider, external_id)
}

fn stub_issue(issue_id: &str, provider: &str, external_id: &str) -> crate::tracker::Issue {
    crate::tracker::Issue {
        external_id: external_id.to_string(),
        provider: provider.to_string(),
        title: String::new(),
        status: String::new(),
        labels: vec![],
        body: None,
        url: None,
        assignee: None,
        raw_json: serde_json::Value::Null,
        provider_native_id: None,
        provider_scope_type: None,
        provider_scope_key: None,
        provider_scope_name: None,
        provider_metadata: serde_json::json!({}),
        id: Some(issue_id.to_string()),
        project_id: None,
        canonical_status: None,
        priority: None,
        is_native: false,
        created_at: None,
        updated_at: None,
        synced_at: None,
        run_id: None,
    }
}

fn apply_project_transition(
    conn: &mut Connection,
    workspace_root: &Path,
    cfg: &GroveConfig,
    project_settings: &crate::db::repositories::projects_repo::ProjectSettings,
    issue: &crate::tracker::Issue,
    target_status: &str,
    event_type: &str,
) -> GroveResult<()> {
    let provider = &issue.provider;
    let external_id = &issue.external_id;

    // For grove-native issues, just update the DB.
    if provider == "grove" {
        let issue_id = issue.id.as_deref().unwrap_or(external_id);
        let canonical = status::normalize(provider, target_status);
        let _ = issues_repo::update_status(conn, issue_id, target_status, canonical);
        let _ = issues_repo::record_event(
            conn,
            issue_id,
            event_type,
            "grove",
            None,
            Some(target_status),
        );
        return Ok(());
    }

    // For external providers, use the project-specific tracker.
    let result = match provider.as_str() {
        "github" => {
            let tracker =
                crate::tracker::github::GitHubTracker::new(workspace_root, &cfg.tracker.github);
            // Use --repo from issue metadata so transitions work regardless of whether
            // workspace_root has a git remote pointing to the right repo.
            // apply_transition_for_repo handles: close/done → close, open → reopen, anything else → add-label.
            let repo = issue.provider_scope_key.as_deref();
            tracker.apply_transition_for_repo(external_id, target_status, repo)
        }
        "jira" => {
            let tracker = crate::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
            tracker.transition(external_id, target_status)
        }
        "linear" => {
            let tracker = crate::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
            tracker.transition(external_id, target_status)
        }
        _ => return Ok(()),
    };

    let _ = project_settings; // used implicitly for future per-project overrides
    match result {
        Ok(()) => {
            // Update local DB
            let fallback_id = format!("{}:{}", provider, external_id);
            let issue_db_id = issue.id.as_deref().unwrap_or(&fallback_id);
            let canonical = status::normalize(provider, target_status);
            let _ = issues_repo::update_status(conn, issue_db_id, target_status, canonical);
            let _ = issues_repo::record_event(
                conn,
                issue_db_id,
                event_type,
                "grove",
                None,
                Some(target_status),
            );
            tracing::info!(
                provider = %provider,
                issue = %external_id,
                status = %target_status,
                "issue workflow transition applied"
            );
        }
        Err(e) => {
            tracing::warn!(
                provider = %provider,
                issue = %external_id,
                target = %target_status,
                error = %e,
                "workflow transition failed (non-fatal)"
            );
        }
    }

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(run_id: &str, issue_id: &str) -> WriteBackContext {
        WriteBackContext {
            run_id: run_id.to_string(),
            issue_id: issue_id.to_string(),
            pr_url: Some("https://github.com/org/repo/pull/42".into()),
            cost_usd: 0.42,
            duration_secs: 37,
            agent_count: 2,
            error: None,
        }
    }

    #[test]
    fn render_template_substitutes_all_placeholders() {
        let template = "Run {run_id} done in {duration}s. Cost: ${cost_usd}. PR: {pr_url}. Agents: {agent_count}.";
        let result = render_template(template, &ctx("abcdef1234", "github:10"));
        assert_eq!(
            result,
            "Run abcdef12 done in 37s. Cost: $0.42. PR: https://github.com/org/repo/pull/42. Agents: 2."
        );
    }

    #[test]
    fn render_template_error_placeholder() {
        let template = "Failed: {error}";
        let ctx = WriteBackContext {
            error: Some("timeout exceeded".into()),
            ..ctx("run1", "github:5")
        };
        let result = render_template(template, &ctx);
        assert_eq!(result, "Failed: timeout exceeded");
    }

    #[test]
    fn render_template_no_pr_url() {
        let template = "PR: {pr_url}";
        let ctx = WriteBackContext {
            pr_url: None,
            ..ctx("run2", "github:6")
        };
        let result = render_template(template, &ctx);
        assert_eq!(result, "PR: (no PR)");
    }

    #[test]
    fn on_run_complete_noop_when_disabled() {
        let cfg: GroveConfig =
            serde_yaml::from_str(crate::config::DEFAULT_CONFIG_YAML).expect("default config");
        // Default has write_back.enabled = false
        let dir = tempfile::tempdir().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let handle = crate::db::DbHandle::new(dir.path());
        let mut conn = handle.connect().unwrap();
        let result = on_run_complete(&mut conn, &cfg, dir.path(), ctx("run-abc", "github:99"));
        assert!(result.is_ok());
    }

    #[test]
    fn on_run_failed_noop_when_disabled() {
        let cfg: GroveConfig =
            serde_yaml::from_str(crate::config::DEFAULT_CONFIG_YAML).expect("default config");
        let dir = tempfile::tempdir().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let handle = crate::db::DbHandle::new(dir.path());
        let mut conn = handle.connect().unwrap();
        let result = on_run_failed(
            &mut conn,
            &cfg,
            dir.path(),
            WriteBackContext {
                error: Some("agent timed out".into()),
                ..ctx("run-xyz", "github:7")
            },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn on_merge_complete_noop_when_disabled() {
        let cfg: GroveConfig =
            serde_yaml::from_str(crate::config::DEFAULT_CONFIG_YAML).expect("default config");
        let dir = tempfile::tempdir().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let handle = crate::db::DbHandle::new(dir.path());
        let mut conn = handle.connect().unwrap();
        let result = on_merge_complete(&mut conn, &cfg, dir.path(), "run-merge-1", None);
        assert!(result.is_ok());
    }
}
