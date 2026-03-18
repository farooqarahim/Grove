use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{ExternalTrackerConfig, GroveConfig, TrackerMode};
use crate::errors::{GroveError, GroveResult};

pub mod ci;
pub mod credentials;
pub mod github;
pub mod jira;
pub mod linear;
pub mod linter;
pub mod registry;
pub mod status;
pub mod sync;
pub mod write_back;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub external_id: String,
    #[serde(default = "default_provider")]
    pub provider: String,
    pub title: String,
    pub status: String,
    pub labels: Vec<String>,
    pub body: Option<String>,
    pub url: Option<String>,
    pub assignee: Option<String>,
    pub raw_json: Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider_native_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider_scope_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider_scope_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider_scope_name: Option<String>,
    #[serde(default = "default_provider_metadata")]
    pub provider_metadata: Value,
    // ── DB-enriched fields (populated when read from local SQLite) ────────────
    /// Composite `{provider}:{external_id}` primary key from the `issues` table.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub canonical_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub priority: Option<String>,
    /// `true` when the issue was created locally in Grove (provider = "grove").
    #[serde(default)]
    pub is_native: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub run_id: Option<String>,
}

/// A project, repository, or team on an external issue tracker.
///
/// Used to populate the "which board?" selector in the Create Issue drawer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProject {
    /// Provider-internal project identifier (e.g. GitHub `nameWithOwner`, Jira numeric id, Linear UUID).
    pub id: String,
    /// Human-readable project / repository / team name.
    pub name: String,
    /// Short key used when creating issues (Jira project key, Linear team key, GitHub `nameWithOwner`).
    pub key: Option<String>,
    /// Web URL to the project or board.
    pub url: Option<String>,
}

/// A status, state, or label available on an issue tracker provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderStatus {
    /// Provider-internal ID (label name for GitHub, UUID for Linear, status id for Jira, canonical key for Grove).
    pub id: String,
    /// Human-readable display name.
    pub name: String,
    /// Normalized category: "backlog" | "todo" | "in_progress" | "done" | "cancelled".
    pub category: String,
    /// Provider-native hex color (without #), if available.
    pub color: Option<String>,
}

fn default_provider() -> String {
    "github".to_string()
}

fn default_provider_metadata() -> Value {
    Value::Object(Default::default())
}

/// Fields that can be updated on an existing issue.
///
/// All fields are optional; `None` means "leave unchanged".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IssueUpdate {
    pub title: Option<String>,
    pub body: Option<String>,
    pub status: Option<String>,
    pub labels: Option<Vec<String>>,
    pub assignee: Option<String>,
    pub priority: Option<String>,
}

/// Pagination cursor for incremental syncs.
///
/// - `since`: fetch issues updated after this timestamp (for timestamp-based APIs).
/// - `offset`: record offset for REST-style pagination (e.g. Jira `startAt`).
/// - `after_cursor`: opaque cursor string for GraphQL cursor pagination (e.g. Linear).
/// - `limit`: maximum number of issues to return per page.
#[derive(Debug, Clone)]
pub struct SyncCursor {
    pub since: Option<DateTime<Utc>>,
    pub limit: usize,
    pub offset: usize,
    pub after_cursor: Option<String>,
}

impl Default for SyncCursor {
    fn default() -> Self {
        Self {
            since: None,
            limit: 50,
            offset: 0,
            after_cursor: None,
        }
    }
}

/// Abstraction over issue trackers.
pub trait TrackerBackend: Send + Sync {
    /// Provider name (e.g. "github", "jira", "linear").
    fn provider_name(&self) -> &str;
    fn create(&self, title: &str, body: &str) -> GroveResult<Issue>;
    fn show(&self, id: &str) -> GroveResult<Issue>;
    fn list(&self) -> GroveResult<Vec<Issue>>;
    fn close(&self, id: &str) -> GroveResult<()>;
    fn ready(&self) -> GroveResult<Vec<Issue>>;

    // ── Extended methods (all have default implementations) ──────────────────

    /// Full-text search across issues.
    fn search(&self, _query: &str, _limit: usize) -> GroveResult<Vec<Issue>> {
        Err(GroveError::Runtime(format!(
            "search is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// Fetch a page of issues using the provided cursor.
    fn list_paginated(&self, _cursor: &SyncCursor) -> GroveResult<Vec<Issue>> {
        Err(GroveError::Runtime(format!(
            "list_paginated is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// Post a comment on an issue.  Returns the comment URL or provider ID.
    fn comment(&self, _id: &str, _body: &str) -> GroveResult<String> {
        Err(GroveError::Runtime(format!(
            "comment is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// Update mutable fields on an existing issue.
    fn update(&self, _id: &str, _update: &IssueUpdate) -> GroveResult<Issue> {
        Err(GroveError::Runtime(format!(
            "update is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// Transition an issue to a new status using the provider's native status string.
    fn transition(&self, _id: &str, _target_status: &str) -> GroveResult<()> {
        Err(GroveError::Runtime(format!(
            "transition is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// Assign an issue to a user (login / display name / email depending on provider).
    fn assign(&self, _id: &str, _assignee: &str) -> GroveResult<()> {
        Err(GroveError::Runtime(format!(
            "assign is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// Re-open a previously closed issue.
    fn reopen(&self, _id: &str) -> GroveResult<()> {
        Err(GroveError::Runtime(format!(
            "reopen is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// List available projects / repositories / teams on this provider.
    ///
    /// Used to populate the board/project selector in the Create Issue UI.
    fn list_projects(&self) -> GroveResult<Vec<ProviderProject>> {
        Err(GroveError::Runtime(format!(
            "list_projects is not supported by the '{}' tracker",
            self.provider_name()
        )))
    }

    /// Create an issue in a specific project / repo / team identified by `project_key`.
    ///
    /// Falls back to `create()` (provider's default project) when not overridden.
    fn create_in_project(&self, title: &str, body: &str, _project_key: &str) -> GroveResult<Issue> {
        self.create(title, body)
    }
}

/// External tracker that shells out to CLI tools (e.g. `gh`).
pub struct ExternalTracker {
    config: ExternalTrackerConfig,
    project_root: PathBuf,
}

impl ExternalTracker {
    pub fn new(config: ExternalTrackerConfig, project_root: &Path) -> Self {
        Self {
            config,
            project_root: project_root.to_owned(),
        }
    }

    fn run_command(&self, template: &str, vars: &[(&str, &str)]) -> GroveResult<String> {
        let cmd_str = substitute_placeholders(template, vars);
        let parts: Vec<&str> = cmd_str.split_whitespace().collect();
        if parts.is_empty() {
            return Err(GroveError::Runtime("empty tracker command".into()));
        }

        let output = Command::new(parts[0])
            .args(&parts[1..])
            .current_dir(&self.project_root)
            .env("PATH", crate::capability::shell_path())
            .output()
            .map_err(|e| GroveError::Runtime(format!("tracker command failed to start: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GroveError::Runtime(format!(
                "tracker command failed (exit {}): {}",
                output.status,
                stderr.chars().take(500).collect::<String>()
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn parse_issue_json(raw: &str) -> GroveResult<Issue> {
        let v: Value = serde_json::from_str(raw.trim())
            .map_err(|e| GroveError::Runtime(format!("failed to parse tracker JSON: {e}")))?;

        let external_id = v
            .get("number")
            .and_then(|n| n.as_i64())
            .map(|n| n.to_string())
            .or_else(|| v.get("id").and_then(|s| s.as_str()).map(|s| s.to_string()))
            .unwrap_or_default();

        let title = v
            .get("title")
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();

        let status = v
            .get("state")
            .and_then(|s| s.as_str())
            .unwrap_or("open")
            .to_string();

        let labels = v
            .get("labels")
            .and_then(|arr| arr.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| {
                        l.get("name")
                            .and_then(|n| n.as_str())
                            .or_else(|| l.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        let body = v
            .get("body")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let url = v.get("url").and_then(|s| s.as_str()).map(|s| s.to_string());
        let assignee = v
            .get("assignees")
            .and_then(|a| a.as_array())
            .and_then(|arr| arr.first())
            .and_then(|a| a.get("login"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());

        Ok(Issue {
            external_id,
            provider: "external".to_string(),
            title,
            status,
            labels,
            body,
            url,
            assignee,
            raw_json: v,
            provider_native_id: None,
            provider_scope_type: None,
            provider_scope_key: None,
            provider_scope_name: None,
            provider_metadata: default_provider_metadata(),
            id: None,
            project_id: None,
            canonical_status: None,
            priority: None,
            is_native: false,
            created_at: None,
            updated_at: None,
            synced_at: None,
            run_id: None,
        })
    }

    fn parse_issue_list_json(raw: &str) -> GroveResult<Vec<Issue>> {
        let arr: Vec<Value> = serde_json::from_str(raw.trim())
            .map_err(|e| GroveError::Runtime(format!("failed to parse tracker JSON list: {e}")))?;

        arr.iter()
            .map(|v| {
                let s = serde_json::to_string(v).unwrap_or_default();
                Self::parse_issue_json(&s)
            })
            .collect()
    }
}

impl TrackerBackend for ExternalTracker {
    fn provider_name(&self) -> &str {
        "external"
    }

    fn create(&self, title: &str, body: &str) -> GroveResult<Issue> {
        let output =
            self.run_command(&self.config.create, &[("{title}", title), ("{body}", body)])?;
        Self::parse_issue_json(&output)
    }

    fn show(&self, id: &str) -> GroveResult<Issue> {
        let output = self.run_command(&self.config.show, &[("{id}", id)])?;
        Self::parse_issue_json(&output)
    }

    fn list(&self) -> GroveResult<Vec<Issue>> {
        let output = self.run_command(&self.config.list, &[])?;
        Self::parse_issue_list_json(&output)
    }

    fn close(&self, id: &str) -> GroveResult<()> {
        let _ = self.run_command(&self.config.close, &[("{id}", id)])?;
        Ok(())
    }

    fn ready(&self) -> GroveResult<Vec<Issue>> {
        let output = self.run_command(&self.config.ready, &[])?;
        Self::parse_issue_list_json(&output)
    }
}

/// Build the appropriate tracker backend based on config.
pub fn build_backend(
    cfg: &GroveConfig,
    project_root: &Path,
) -> GroveResult<Box<dyn TrackerBackend>> {
    match cfg.tracker.mode {
        TrackerMode::Disabled => Err(GroveError::Runtime(
            "issue tracker is disabled — set tracker.mode in grove.yaml (github, jira, linear, multi, or external)".into(),
        )),
        TrackerMode::External => Ok(Box::new(ExternalTracker::new(
            cfg.tracker.external.clone(),
            project_root,
        ))),
        TrackerMode::GitHub => Ok(Box::new(github::GitHubTracker::new(
            project_root,
            &cfg.tracker.github,
        ))),
        TrackerMode::Jira => Ok(Box::new(jira::JiraTracker::new(&cfg.tracker.jira))),
        TrackerMode::Linear => Ok(Box::new(linear::LinearTracker::new(&cfg.tracker.linear))),
        TrackerMode::Multi => {
            // For single-backend API, return the first enabled provider
            if cfg.tracker.github.enabled {
                Ok(Box::new(github::GitHubTracker::new(project_root, &cfg.tracker.github)))
            } else if cfg.tracker.jira.enabled {
                Ok(Box::new(jira::JiraTracker::new(&cfg.tracker.jira)))
            } else if cfg.tracker.linear.enabled {
                Ok(Box::new(linear::LinearTracker::new(&cfg.tracker.linear)))
            } else {
                Err(GroveError::Runtime(
                    "tracker.mode is 'multi' but no providers have enabled: true".into(),
                ))
            }
        }
    }
}

// ── Cache functions ──────────────────────────────────────────────────────────

/// Cache an issue in the local SQLite database, scoped to a project.
///
/// Uses an upsert that preserves `created_at` on subsequent writes so
/// re-syncing the same issue does not reset its creation timestamp.
pub fn cache_issue(conn: &Connection, issue: &Issue, project_id: &str) -> GroveResult<()> {
    crate::db::repositories::issues_repo::upsert(conn, issue, project_id)
}

/// Retrieve a cached issue by external ID, scoped to a project.
pub fn get_cached(
    conn: &Connection,
    external_id: &str,
    project_id: &str,
) -> GroveResult<Option<Issue>> {
    let provider = conn
        .query_row(
            "SELECT provider FROM issues WHERE external_id = ?1 AND project_id = ?2 LIMIT 1",
            params![external_id, project_id],
            |r| r.get::<_, String>(0),
        )
        .optional()?;

    match provider {
        Some(provider) => crate::db::repositories::issues_repo::get_by_external(
            conn,
            &provider,
            external_id,
            project_id,
        ),
        None => Ok(None),
    }
}

/// List cached issues for a specific project.
pub fn list_cached(conn: &Connection, project_id: &str) -> GroveResult<Vec<Issue>> {
    crate::db::repositories::issues_repo::list(
        conn,
        project_id,
        &crate::db::repositories::issues_repo::IssueFilter::new(),
    )
}

/// Link a run to an external issue.
pub fn link_run_to_issue(conn: &Connection, run_id: &str, external_id: &str) -> GroveResult<()> {
    conn.execute(
        "UPDATE issues SET run_id = ?1 WHERE external_id = ?2",
        params![run_id, external_id],
    )?;
    Ok(())
}

/// Substitute `{key}` placeholders in a template string.
pub fn substitute_placeholders(template: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(key, value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::DbHandle;

    fn setup_test_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        db::initialize(dir.path()).unwrap();
        let handle = DbHandle::new(dir.path());
        let conn = handle.connect().unwrap();
        (dir, conn)
    }

    #[test]
    fn test_substitute_placeholders() {
        let result = substitute_placeholders("gh issue view {id} --json title", &[("{id}", "42")]);
        assert_eq!(result, "gh issue view 42 --json title");

        let result = substitute_placeholders(
            "gh issue create --title '{title}' --body '{body}'",
            &[("{title}", "bug fix"), ("{body}", "details here")],
        );
        assert_eq!(
            result,
            "gh issue create --title 'bug fix' --body 'details here'"
        );
    }

    fn test_issue(id: &str, title: &str) -> Issue {
        Issue {
            external_id: id.into(),
            provider: "github".into(),
            title: title.into(),
            status: "open".into(),
            labels: vec![],
            body: None,
            url: None,
            assignee: None,
            raw_json: serde_json::json!({}),
            provider_native_id: None,
            provider_scope_type: None,
            provider_scope_key: None,
            provider_scope_name: None,
            provider_metadata: default_provider_metadata(),
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

    #[test]
    fn test_cache_and_retrieve() {
        let (_dir, conn) = setup_test_db();
        let issue = Issue {
            external_id: "42".into(),
            provider: "github".into(),
            title: "Fix login bug".into(),
            status: "open".into(),
            labels: vec!["bug".into(), "priority-high".into()],
            body: Some("Login fails on mobile".into()),
            url: Some("https://github.com/org/repo/issues/42".into()),
            assignee: Some("octocat".into()),
            raw_json: serde_json::json!({"number": 42}),
            provider_native_id: Some("node-42".into()),
            provider_scope_type: Some("repository".into()),
            provider_scope_key: Some("org/repo".into()),
            provider_scope_name: Some("org/repo".into()),
            provider_metadata: serde_json::json!({"repository": "org/repo"}),
            id: None,
            project_id: None,
            canonical_status: None,
            priority: None,
            is_native: false,
            created_at: None,
            updated_at: None,
            synced_at: None,
            run_id: None,
        };

        cache_issue(&conn, &issue, "proj-1").unwrap();
        let cached = get_cached(&conn, "42", "proj-1").unwrap().unwrap();
        assert_eq!(cached.title, "Fix login bug");
        assert_eq!(cached.provider, "github");
        assert_eq!(cached.labels.len(), 2);
        assert_eq!(cached.body.as_deref(), Some("Login fails on mobile"));
        assert_eq!(
            cached.url.as_deref(),
            Some("https://github.com/org/repo/issues/42")
        );
        assert_eq!(cached.assignee.as_deref(), Some("octocat"));

        // Different project should not see it
        let other = get_cached(&conn, "42", "proj-2").unwrap();
        assert!(other.is_none());
    }

    #[test]
    fn test_list_cached() {
        let (_dir, conn) = setup_test_db();
        for i in 1..=3 {
            let issue = test_issue(&i.to_string(), &format!("Issue #{i}"));
            cache_issue(&conn, &issue, "proj-a").unwrap();
        }
        // Add one for a different project
        cache_issue(&conn, &test_issue("99", "Other project issue"), "proj-b").unwrap();

        let cached_a = list_cached(&conn, "proj-a").unwrap();
        assert_eq!(cached_a.len(), 3);

        let cached_b = list_cached(&conn, "proj-b").unwrap();
        assert_eq!(cached_b.len(), 1);
        assert_eq!(cached_b[0].external_id, "99");
    }

    #[test]
    fn test_link_run_to_issue() {
        let (_dir, conn) = setup_test_db();
        let issue = test_issue("99", "Test issue");
        cache_issue(&conn, &issue, "proj-1").unwrap();

        link_run_to_issue(&conn, "run_abc", "99").unwrap();

        let run_id: Option<String> = conn
            .query_row(
                "SELECT run_id FROM issues WHERE external_id = '99'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(run_id.as_deref(), Some("run_abc"));
    }

    #[test]
    fn test_build_backend_disabled() {
        let cfg: GroveConfig =
            serde_yaml::from_str(crate::config::DEFAULT_CONFIG_YAML).expect("default config");
        // Default mode is Disabled
        let result = build_backend(&cfg, Path::new("/tmp"));
        assert!(result.is_err());
    }
}
