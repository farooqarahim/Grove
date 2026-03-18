use std::path::Path;
use std::time::Instant;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::{GroveConfig, TrackerMode};
use crate::db::repositories::issues_repo;
use crate::tracker::linter;
use crate::tracker::registry::TrackerRegistry;
use crate::tracker::{SyncCursor, TrackerBackend};

// ── Public result types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub provider: String,
    pub new_count: usize,
    pub updated_count: usize,
    pub closed_count: usize,
    pub errors: Vec<String>,
    pub duration_ms: u64,
    pub synced_at: DateTime<Utc>,
}

impl SyncResult {
    fn empty(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
            new_count: 0,
            updated_count: 0,
            closed_count: 0,
            errors: Vec::new(),
            duration_ms: 0,
            synced_at: Utc::now(),
        }
    }

    fn error(provider: &str, msg: String) -> Self {
        let mut r = Self::empty(provider);
        r.errors.push(msg);
        r
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSyncResult {
    pub results: Vec<SyncResult>,
    pub total_new: usize,
    pub total_updated: usize,
    pub total_errors: usize,
}

impl MultiSyncResult {
    fn from_results(results: Vec<SyncResult>) -> Self {
        let total_new = results.iter().map(|r| r.new_count).sum();
        let total_updated = results.iter().map(|r| r.updated_count).sum();
        let total_errors = results.iter().map(|r| r.errors.len()).sum();
        Self {
            results,
            total_new,
            total_updated,
            total_errors,
        }
    }
}

// ── Core sync functions ───────────────────────────────────────────────────────

/// Sync issues from a single provider backend into the local `issues` table.
///
/// **Debounce**: if this provider was synced within `debounce_secs` ago (from
/// `issue_sync_state`), the function returns an empty result immediately.
///
/// **Incremental vs full**: when `incremental = true` the cursor passes
/// `last_synced_at` so the provider only returns recently-updated issues.
/// A full sync fetches all issues and additionally detects ones that were
/// closed/deleted on the provider side.
pub fn sync_provider(
    conn: &mut Connection,
    backend: &dyn TrackerBackend,
    project_id: &str,
    incremental: bool,
    debounce_secs: u64,
) -> SyncResult {
    let provider = backend.provider_name().to_string();
    let timer = Instant::now();

    // ── Debounce check ────────────────────────────────────────────────────────
    if let Ok(states) = issues_repo::get_sync_states(conn, project_id) {
        if let Some(state) = states.iter().find(|s| s.provider == provider) {
            if let Some(last) = &state.last_synced_at {
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(last) {
                    let age = Utc::now()
                        .signed_duration_since(parsed.with_timezone(&Utc))
                        .num_seconds();
                    if age >= 0 && age < debounce_secs as i64 {
                        tracing::debug!(
                            provider = %provider,
                            age_secs = age,
                            debounce_secs,
                            "skipping sync — debounce window active"
                        );
                        return SyncResult::empty(&provider);
                    }
                }
            }
        }
    }

    // ── Build sync cursor ─────────────────────────────────────────────────────
    let cursor = if incremental {
        let since = issues_repo::get_sync_states(conn, project_id)
            .ok()
            .and_then(|states| states.into_iter().find(|s| s.provider == provider))
            .and_then(|s| s.last_synced_at)
            .and_then(|ts| chrono::DateTime::parse_from_rfc3339(&ts).ok())
            .map(|dt| dt.with_timezone(&Utc));
        SyncCursor {
            since,
            limit: 100,
            offset: 0,
            after_cursor: None,
        }
    } else {
        SyncCursor {
            since: None,
            limit: 200,
            offset: 0,
            after_cursor: None,
        }
    };

    // ── Fetch from provider ───────────────────────────────────────────────────
    let remote_issues = match backend.list_paginated(&cursor) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!("failed to fetch from {provider}: {e}");
            tracing::error!(provider = %provider, error = %e, "provider sync fetch failed");
            let duration_ms = timer.elapsed().as_millis() as u64;
            let _ = issues_repo::update_sync_state(
                conn,
                &provider,
                project_id,
                0,
                Some(&msg),
                duration_ms,
            );
            return SyncResult::error(&provider, msg);
        }
    };

    // ── Upsert with change detection ──────────────────────────────────────────
    let mut new_count = 0usize;
    let mut updated_count = 0usize;
    let mut closed_count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    // Build a set of remote external_ids for closed-issue detection on full sync.
    let remote_ids: std::collections::HashSet<String> = remote_issues
        .iter()
        .map(|i| i.external_id.clone())
        .collect();

    for issue in &remote_issues {
        let existing =
            issues_repo::get_by_external(conn, &provider, &issue.external_id, project_id);
        match existing {
            Ok(None) => {
                if let Err(e) = issues_repo::upsert(conn, issue, project_id) {
                    errors.push(format!(
                        "upsert {}:{} failed: {e}",
                        provider, issue.external_id
                    ));
                } else {
                    new_count += 1;
                    let issue_id = format!("{}:{}", provider, issue.external_id);
                    let _ =
                        issues_repo::record_event(conn, &issue_id, "synced", "grove", None, None);
                }
            }
            Ok(Some(existing_issue)) => {
                if existing_issue.status != issue.status {
                    let issue_id = format!("{}:{}", provider, issue.external_id);
                    if let Err(e) = issues_repo::update_status(
                        conn,
                        &issue_id,
                        &issue.status,
                        crate::tracker::status::normalize(&provider, &issue.status),
                    ) {
                        errors.push(format!("update_status {issue_id} failed: {e}"));
                    } else {
                        updated_count += 1;
                    }
                } else {
                    // Still upsert to refresh metadata (title, labels, body).
                    if let Err(e) = issues_repo::upsert(conn, issue, project_id) {
                        errors.push(format!(
                            "re-upsert {}:{} failed: {e}",
                            provider, issue.external_id
                        ));
                    }
                }
            }
            Err(e) => {
                errors.push(format!(
                    "lookup {}:{} failed: {e}",
                    provider, issue.external_id
                ));
            }
        }
    }

    // ── Closed-issue detection (full sync only) ───────────────────────────────
    if !incremental {
        let all_local = issues_repo::list(
            conn,
            project_id,
            &issues_repo::IssueFilter {
                provider: Some(provider.clone()),
                limit: 2000,
                ..Default::default()
            },
        );
        if let Ok(local_issues) = all_local {
            for local in local_issues {
                if !remote_ids.contains(&local.external_id)
                    && local.status != "closed"
                    && local.status != "done"
                    && local.status != "cancelled"
                {
                    let issue_id = format!("{}:{}", provider, local.external_id);
                    if let Ok(c) = conn.transaction() {
                        let now = Utc::now().to_rfc3339();
                        let _ = c.execute(
                            "UPDATE issues SET status='closed', canonical_status='done', updated_at=?1 WHERE id=?2",
                            rusqlite::params![now, issue_id],
                        );
                        let _ = c.execute(
                            "INSERT INTO issue_events (issue_id, event_type, old_value, new_value, created_at)
                             VALUES (?1, 'status_changed', ?2, 'closed', ?3)",
                            rusqlite::params![issue_id, local.status, now],
                        );
                        let _ = c.commit();
                        closed_count += 1;
                    }
                }
            }
        }
    }

    // ── Persist sync state ────────────────────────────────────────────────────
    let duration_ms = timer.elapsed().as_millis() as u64;
    let total_synced = new_count + updated_count;
    let error_str = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };

    let _ = issues_repo::update_sync_state(
        conn,
        &provider,
        project_id,
        total_synced,
        error_str.as_deref(),
        duration_ms,
    );

    SyncResult {
        provider,
        new_count,
        updated_count,
        closed_count,
        errors,
        duration_ms,
        synced_at: Utc::now(),
    }
}

/// Sync all configured and enabled providers for a project.
///
/// Providers that fail are captured in `SyncResult.errors`; other providers
/// continue regardless.
pub fn sync_all(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    project_id: &str,
    incremental: bool,
) -> MultiSyncResult {
    let debounce = cfg.tracker.sync.debounce_secs;
    let mut results = Vec::new();

    match cfg.tracker.mode {
        TrackerMode::Disabled => {}
        TrackerMode::Multi => {
            let registry = TrackerRegistry::from_config(cfg, project_root);
            for backend in registry.all_backends() {
                let r = sync_provider(conn, backend.as_ref(), project_id, incremental, debounce);
                results.push(r);
            }
        }
        _ => match crate::tracker::build_backend(cfg, project_root) {
            Ok(backend) => {
                let r = sync_provider(conn, backend.as_ref(), project_id, incremental, debounce);
                results.push(r);
            }
            Err(e) => {
                results.push(SyncResult::error("unknown", e.to_string()));
            }
        },
    }

    // Always include linter sync if linter is configured.
    if cfg.linter.enabled && !cfg.linter.commands.is_empty() {
        let lint_result = sync_lint_issues(conn, cfg, project_root, project_id);
        results.push(lint_result);
    }

    MultiSyncResult::from_results(results)
}

/// Run configured linters and upsert their output as `provider = "linter"` issues.
///
/// External ID format: `{linter_name}:{file}:{line}` — repeated runs on the
/// same source location upsert the existing row rather than creating duplicates.
pub fn sync_lint_issues(
    conn: &mut Connection,
    cfg: &GroveConfig,
    project_root: &Path,
    project_id: &str,
) -> SyncResult {
    let timer = Instant::now();
    let mut new_count = 0usize;
    let mut updated_count = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for lint_cmd in &cfg.linter.commands {
        match linter::run_linter(lint_cmd, project_root) {
            Ok(result) => {
                for lint_issue in &result.issues {
                    let tracker_issue = lint_issue.to_tracker_issue(&result.linter);
                    let existing = issues_repo::get_by_external(
                        conn,
                        "linter",
                        &tracker_issue.external_id,
                        project_id,
                    );
                    match existing {
                        Ok(None) => {
                            if let Err(e) = issues_repo::upsert(conn, &tracker_issue, project_id) {
                                errors.push(format!("linter upsert failed: {e}"));
                            } else {
                                new_count += 1;
                            }
                        }
                        Ok(Some(_)) => {
                            if let Err(e) = issues_repo::upsert(conn, &tracker_issue, project_id) {
                                errors.push(format!("linter re-upsert failed: {e}"));
                            } else {
                                updated_count += 1;
                            }
                        }
                        Err(e) => {
                            errors.push(format!("linter lookup failed: {e}"));
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(linter = %lint_cmd.name, error = %e, "linter run failed");
                errors.push(format!("linter '{}' failed: {e}", lint_cmd.name));
            }
        }
    }

    let duration_ms = timer.elapsed().as_millis() as u64;
    let error_str = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };
    let _ = issues_repo::update_sync_state(
        conn,
        "linter",
        project_id,
        new_count + updated_count,
        error_str.as_deref(),
        duration_ms,
    );

    SyncResult {
        provider: "linter".to_string(),
        new_count,
        updated_count,
        closed_count: 0,
        errors,
        duration_ms,
        synced_at: Utc::now(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::DbHandle;
    use crate::errors::GroveResult;
    use crate::tracker::{Issue, IssueUpdate, SyncCursor, TrackerBackend};

    struct MockBackend {
        provider: &'static str,
        issues: Vec<Issue>,
    }

    impl TrackerBackend for MockBackend {
        fn provider_name(&self) -> &str {
            self.provider
        }
        fn create(&self, _: &str, _: &str) -> GroveResult<Issue> {
            Err(crate::errors::GroveError::Runtime("mock".into()))
        }
        fn show(&self, _: &str) -> GroveResult<Issue> {
            Err(crate::errors::GroveError::Runtime("mock".into()))
        }
        fn list(&self) -> GroveResult<Vec<Issue>> {
            Ok(self.issues.clone())
        }
        fn close(&self, _: &str) -> GroveResult<()> {
            Ok(())
        }
        fn ready(&self) -> GroveResult<Vec<Issue>> {
            Ok(vec![])
        }
        fn list_paginated(&self, _: &SyncCursor) -> GroveResult<Vec<Issue>> {
            Ok(self.issues.clone())
        }
        fn update(&self, _: &str, _: &IssueUpdate) -> GroveResult<Issue> {
            Err(crate::errors::GroveError::Runtime("mock".into()))
        }
    }

    fn setup() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        db::initialize(dir.path()).unwrap();
        let handle = DbHandle::new(dir.path());
        let conn = handle.connect().unwrap();
        (dir, conn)
    }

    fn make_issue(id: &str, title: &str, status: &str) -> Issue {
        Issue {
            external_id: id.into(),
            provider: "mock".into(),
            title: title.into(),
            status: status.into(),
            labels: vec![],
            body: None,
            url: None,
            assignee: None,
            raw_json: serde_json::json!({}),
            provider_native_id: None,
            provider_scope_type: None,
            provider_scope_key: None,
            provider_scope_name: None,
            provider_metadata: serde_json::json!({}),
            id: None,
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

    // ── Full sync inserts all issues ──────────────────────────────────────────

    #[test]
    fn full_sync_inserts_new_issues() {
        let (_dir, mut conn) = setup();
        let backend = MockBackend {
            provider: "mock",
            issues: vec![
                make_issue("1", "Issue one", "open"),
                make_issue("2", "Issue two", "open"),
                make_issue("3", "Issue three", "open"),
            ],
        };

        let result = sync_provider(&mut conn, &backend, "proj-1", false, 0);
        assert_eq!(result.new_count, 3);
        assert_eq!(result.updated_count, 0);
        assert!(result.errors.is_empty());
    }

    // ── Incremental sync only updates changed issues ──────────────────────────

    #[test]
    fn incremental_sync_detects_status_change() {
        let (_dir, mut conn) = setup();

        // Full sync first
        let backend = MockBackend {
            provider: "mock",
            issues: vec![make_issue("10", "Bug", "open")],
        };
        sync_provider(&mut conn, &backend, "proj-1", false, 0);

        // Now provider says issue is closed
        let backend2 = MockBackend {
            provider: "mock",
            issues: vec![make_issue("10", "Bug", "closed")],
        };
        let result = sync_provider(&mut conn, &backend2, "proj-1", true, 0);
        assert_eq!(result.updated_count, 1);

        let issue = issues_repo::get(&conn, "mock:10").unwrap().unwrap();
        assert_eq!(issue.status, "closed");
    }

    // ── Full sync closes issues missing from provider response ────────────────

    #[test]
    fn full_sync_closes_missing_issues() {
        let (_dir, mut conn) = setup();

        // Initial sync: 2 issues
        let backend1 = MockBackend {
            provider: "mock",
            issues: vec![
                make_issue("A", "Issue A", "open"),
                make_issue("B", "Issue B", "open"),
            ],
        };
        sync_provider(&mut conn, &backend1, "proj-1", false, 0);

        // Second full sync: only issue A remains
        let backend2 = MockBackend {
            provider: "mock",
            issues: vec![make_issue("A", "Issue A", "open")],
        };
        let result = sync_provider(&mut conn, &backend2, "proj-1", false, 0);
        assert_eq!(
            result.closed_count, 1,
            "issue B should be detected as closed"
        );

        let b = issues_repo::get(&conn, "mock:B").unwrap().unwrap();
        assert_eq!(b.status, "closed");
    }

    // ── Debounce skips sync within window ─────────────────────────────────────

    #[test]
    fn debounce_skips_within_window() {
        let (_dir, mut conn) = setup();
        let backend = MockBackend {
            provider: "mock",
            issues: vec![make_issue("1", "A", "open")],
        };

        // First sync succeeds
        let r1 = sync_provider(&mut conn, &backend, "p1", false, 0);
        assert_eq!(r1.new_count, 1);

        // Second call with 300s debounce — must skip because last_synced_at was just now
        let r2 = sync_provider(&mut conn, &backend, "p1", false, 300);
        assert_eq!(r2.new_count, 0);
        assert_eq!(r2.updated_count, 0);
    }

    // ── Failed backend does not corrupt DB ───────────────────────────────────

    #[test]
    fn failed_backend_returns_error_result() {
        struct FailBackend;
        impl TrackerBackend for FailBackend {
            fn provider_name(&self) -> &str {
                "fail"
            }
            fn create(&self, _: &str, _: &str) -> GroveResult<Issue> {
                Err(crate::errors::GroveError::Runtime("x".into()))
            }
            fn show(&self, _: &str) -> GroveResult<Issue> {
                Err(crate::errors::GroveError::Runtime("x".into()))
            }
            fn list(&self) -> GroveResult<Vec<Issue>> {
                Err(crate::errors::GroveError::Runtime("x".into()))
            }
            fn close(&self, _: &str) -> GroveResult<()> {
                Err(crate::errors::GroveError::Runtime("x".into()))
            }
            fn ready(&self) -> GroveResult<Vec<Issue>> {
                Err(crate::errors::GroveError::Runtime("x".into()))
            }
        }

        let (_dir, mut conn) = setup();
        let result = sync_provider(&mut conn, &FailBackend, "p1", false, 0);
        assert_eq!(result.new_count, 0);
        assert!(!result.errors.is_empty());
    }
}
