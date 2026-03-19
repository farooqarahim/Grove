use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};

use crate::errors::{GroveError, GroveResult};
use crate::tracker::status::{self, CanonicalStatus};

/// Per-provider workflow transition configuration.
///
/// Controls which status issues are moved to when a run starts, succeeds, or fails.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct WorkflowStepConfig {
    /// Status/label name to apply when a run starts (e.g. "In Progress").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_start: Option<String>,
    /// Status to apply when a run completes successfully (e.g. "Done").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_success: Option<String>,
    /// Status to revert to when a run fails (e.g. "Backlog"). None = leave unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<String>,
    /// Post a comment on the issue when a run fails.
    #[serde(default)]
    pub comment_on_failure: bool,
    /// Post a comment on the issue when a run succeeds.
    #[serde(default)]
    pub comment_on_success: bool,
}

/// Per-project default settings for runs and issue tracker integration.
///
/// Stored as a JSON blob in `projects.settings`. All fields are optional;
/// `None` means "inherit workspace default".
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct ProjectSettings {
    /// Default issue-tracker provider to use (github / jira / linear / grove).
    pub default_provider: Option<String>,
    /// Default project key for the *currently selected* provider (kept for
    /// backward-compat; commands should prefer the provider-specific fields below).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_project_key: Option<String>,
    // ── Per-provider board / repo / team selections ───────────────────────────
    /// GitHub repository in "owner/repo" format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_project_key: Option<String>,
    /// Linear team key (e.g. "ENG").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linear_project_key: Option<String>,
    /// Jira project key (e.g. "PROJ").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira_project_key: Option<String>,
    // ── Per-provider workflow transitions ─────────────────────────────────────
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub github_workflow: Option<WorkflowStepConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linear_workflow: Option<WorkflowStepConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira_workflow: Option<WorkflowStepConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grove_workflow: Option<WorkflowStepConfig>,
    // ── Run defaults ──────────────────────────────────────────────────────────
    /// Maximum number of parallel agents for new runs.
    pub max_parallel_agents: Option<i64>,
    /// Default pipeline name (e.g. "auto", "standard", "quick").
    pub default_pipeline: Option<String>,
    /// Default run budget in USD.
    pub default_budget_usd: Option<f64>,
    /// Default permission mode ("skip_all", "human_gate", "autonomous_gate").
    pub default_permission_mode: Option<String>,
    /// Project-scoped issue board configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_board: Option<IssueBoardConfig>,
    // ── LLM defaults ─────────────────────────────────────────────────────────
    /// Default LLM provider (e.g. "anthropic", "openai").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_llm_provider: Option<String>,
    /// Default LLM model within the selected provider (e.g. "claude-sonnet-4-6").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_llm_model: Option<String>,
}

impl ProjectSettings {
    /// Return the workflow config for a specific provider.
    pub fn workflow_for(&self, provider: &str) -> Option<&WorkflowStepConfig> {
        match provider {
            "github" => self.github_workflow.as_ref(),
            "linear" => self.linear_workflow.as_ref(),
            "jira" => self.jira_workflow.as_ref(),
            "grove" => self.grove_workflow.as_ref(),
            _ => None,
        }
    }

    /// Return the board key for a specific provider, falling back to the
    /// legacy `default_project_key` when the provider matches `default_provider`.
    pub fn project_key_for(&self, provider: &str) -> Option<&str> {
        let specific = match provider {
            "github" => self.github_project_key.as_deref(),
            "linear" => self.linear_project_key.as_deref(),
            "jira" => self.jira_project_key.as_deref(),
            _ => None,
        };
        specific.or_else(|| {
            if self.default_provider.as_deref() == Some(provider) {
                self.default_project_key.as_deref()
            } else {
                None
            }
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct IssueBoardColumnConfig {
    pub id: String,
    pub label: String,
    pub canonical_status: CanonicalStatus,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub match_rules: BTreeMap<String, Vec<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_targets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
pub struct IssueBoardConfig {
    #[serde(default)]
    pub columns: Vec<IssueBoardColumnConfig>,
}

impl IssueBoardConfig {
    pub fn canonical_default() -> Self {
        let providers = ["github", "jira", "linear", "grove", "linter", "external"];
        let columns = CanonicalStatus::ordered()
            .iter()
            .map(|&canonical_status| {
                let mut provider_targets = BTreeMap::new();
                for provider in providers {
                    provider_targets.insert(
                        provider.to_string(),
                        status::denormalize(provider, &canonical_status).to_string(),
                    );
                }

                IssueBoardColumnConfig {
                    id: canonical_status.as_db_str().to_string(),
                    label: canonical_status.display_label().to_string(),
                    canonical_status,
                    match_rules: BTreeMap::new(),
                    provider_targets,
                }
            })
            .collect();

        Self { columns }
    }

    pub fn normalized_or_default(config: Option<Self>) -> Self {
        match config {
            Some(config) if !config.columns.is_empty() => config,
            _ => Self::canonical_default(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default, PartialEq, Eq)]
pub struct ProjectSourceDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_visibility: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitignore_template: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gitignore_entries: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_git: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_remote_path: Option<String>,
}

impl ProjectSourceDetails {
    pub fn is_empty(&self) -> bool {
        self.repo_provider.is_none()
            && self.repo_url.is_none()
            && self.repo_visibility.is_none()
            && self.remote_name.is_none()
            && self.gitignore_template.is_none()
            && self.gitignore_entries.is_empty()
            && self.source_path.is_none()
            && self.preserve_git.is_none()
            && self.ssh_host.is_none()
            && self.ssh_user.is_none()
            && self.ssh_port.is_none()
            && self.ssh_remote_path.is_none()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub workspace_id: String,
    pub name: Option<String>,
    pub root_path: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    /// Optional git ref to branch runs from (e.g. `"origin/main"`).
    /// `None` means branch from HEAD (legacy default).
    pub base_ref: Option<String>,
    #[serde(default = "default_source_kind")]
    pub source_kind: String,
    #[serde(default)]
    pub source_details: Option<ProjectSourceDetails>,
}

fn default_source_kind() -> String {
    "local".to_string()
}

fn deserialize_source_details(
    raw: Option<String>,
) -> rusqlite::Result<Option<ProjectSourceDetails>> {
    match raw {
        Some(json) => serde_json::from_str(&json).map(Some).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
        }),
        None => Ok(None),
    }
}

fn serialize_source_details(details: Option<&ProjectSourceDetails>) -> GroveResult<Option<String>> {
    match details {
        Some(details) if !details.is_empty() => {
            serde_json::to_string(details).map(Some).map_err(|e| {
                GroveError::Runtime(format!("failed to serialize project source metadata: {e}"))
            })
        }
        _ => Ok(None),
    }
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        name: r.get(2)?,
        root_path: r.get(3)?,
        state: r.get(4)?,
        created_at: r.get(5)?,
        updated_at: r.get(6)?,
        base_ref: r.get(7).ok().flatten(),
        source_kind: r
            .get::<_, Option<String>>(8)?
            .unwrap_or_else(default_source_kind),
        source_details: deserialize_source_details(r.get::<_, Option<String>>(9)?)?,
    })
}

pub fn insert(conn: &Connection, row: &ProjectRow) -> GroveResult<()> {
    let source_details_json = serialize_source_details(row.source_details.as_ref())?;
    conn.execute(
        "INSERT INTO projects (
            id, workspace_id, name, root_path, state, created_at, updated_at, base_ref, source_kind, source_details_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            row.id,
            row.workspace_id,
            row.name,
            row.root_path,
            row.state,
            row.created_at,
            row.updated_at,
            row.base_ref,
            row.source_kind,
            source_details_json,
        ],
    )?;
    Ok(())
}

pub fn upsert(conn: &Connection, row: &ProjectRow) -> GroveResult<()> {
    let source_details_json = serialize_source_details(row.source_details.as_ref())?;
    conn.execute(
        "INSERT INTO projects (
            id, workspace_id, name, root_path, state, created_at, updated_at, base_ref, source_kind, source_details_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at",
        params![
            row.id,
            row.workspace_id,
            row.name,
            row.root_path,
            row.state,
            row.created_at,
            row.updated_at,
            row.base_ref,
            row.source_kind,
            source_details_json,
        ],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> GroveResult<ProjectRow> {
    let row = conn
        .query_row(
            "SELECT id, workspace_id, name, root_path, state, created_at, updated_at, base_ref, source_kind, source_details_json
             FROM projects WHERE id=?1",
            [id],
            map_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("project {id}")))
}

pub fn get_by_root_path(conn: &Connection, root_path: &str) -> GroveResult<Option<ProjectRow>> {
    let row = conn
        .query_row(
            "SELECT id, workspace_id, name, root_path, state, created_at, updated_at, base_ref, source_kind, source_details_json
             FROM projects WHERE root_path=?1",
            [root_path],
            map_row,
        )
        .optional()?;
    Ok(row)
}

/// Fetch multiple projects by ID in a single query.
pub fn get_batch(
    conn: &Connection,
    ids: &[&str],
) -> GroveResult<std::collections::HashMap<String, ProjectRow>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT id, workspace_id, name, root_path, state, created_at, updated_at, \
                base_ref, source_kind, source_details_json \
         FROM projects WHERE id IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    let map = rows.into_iter().map(|r| (r.id.clone(), r)).collect();
    Ok(map)
}

pub fn list_for_workspace(
    conn: &Connection,
    workspace_id: &str,
    limit: i64,
) -> GroveResult<Vec<ProjectRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, workspace_id, name, root_path, state, created_at, updated_at, base_ref, source_kind, source_details_json
         FROM projects
         WHERE workspace_id=?1
         ORDER BY updated_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![workspace_id, limit], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn set_state(conn: &Connection, id: &str, state: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE projects SET state=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![state, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("project {id}")));
    }
    Ok(())
}

pub fn update_name(conn: &Connection, id: &str, name: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE projects SET name=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![name, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("project {id}")));
    }
    Ok(())
}

/// Set or clear the `base_ref` for a project.
///
/// `base_ref` is the git ref that runs branch from. `None` clears it
/// (runs will branch from HEAD).
pub fn set_base_ref(conn: &Connection, id: &str, base_ref: Option<&str>) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE projects SET base_ref=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![base_ref, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("project {id}")));
    }
    Ok(())
}

pub fn update_source(
    conn: &Connection,
    id: &str,
    source_kind: &str,
    source_details: Option<&ProjectSourceDetails>,
) -> GroveResult<()> {
    let source_details_json = serialize_source_details(source_details)?;
    let n = conn.execute(
        "UPDATE projects
         SET source_kind=?1, source_details_json=?2, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id=?3",
        params![source_kind, source_details_json, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("project {id}")));
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> GroveResult<()> {
    let n = conn.execute("DELETE FROM projects WHERE id=?1", [id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("project {id}")));
    }
    Ok(())
}

/// Retrieve the per-project settings, or `ProjectSettings::default()` when
/// the column is NULL (project predates migration 0025).
pub fn get_settings(conn: &Connection, id: &str) -> GroveResult<ProjectSettings> {
    let raw: Option<String> = conn
        .query_row("SELECT settings FROM projects WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();

    match raw {
        Some(json) => serde_json::from_str(&json)
            .map_err(|e| GroveError::Runtime(format!("invalid project settings JSON: {e}"))),
        None => Ok(ProjectSettings::default()),
    }
}

/// Persist per-project settings. Overwrites the entire settings blob.
pub fn update_settings(conn: &Connection, id: &str, settings: &ProjectSettings) -> GroveResult<()> {
    let json = serde_json::to_string(settings)
        .map_err(|e| GroveError::Runtime(format!("failed to serialize project settings: {e}")))?;
    let n = conn.execute(
        "UPDATE projects SET settings=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![json, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("project {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::workspaces_repo;
    use chrono::Utc;

    fn test_db() -> Connection {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let conn = crate::db::DbHandle::new(dir.path()).connect().unwrap();
        // Insert a workspace to satisfy FK
        let now = Utc::now().to_rfc3339();
        workspaces_repo::insert(
            &conn,
            &workspaces_repo::WorkspaceRow {
                id: "ws1".to_string(),
                name: None,
                state: "active".to_string(),
                created_at: now.clone(),
                updated_at: now,
                credits_usd: 0.0,
                llm_provider: None,
                llm_model: None,
                llm_auth_mode: "user_key".to_string(),
            },
        )
        .unwrap();
        conn
    }

    fn make_row(id: &str) -> ProjectRow {
        let now = Utc::now().to_rfc3339();
        ProjectRow {
            id: id.to_string(),
            workspace_id: "ws1".to_string(),
            name: Some("Test Project".to_string()),
            root_path: "/tmp/test-project".to_string(),
            state: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
            base_ref: None,
            source_kind: "local".to_string(),
            source_details: None,
        }
    }

    #[test]
    fn insert_and_get() {
        let conn = test_db();
        let row = make_row("proj1");
        insert(&conn, &row).unwrap();
        let got = get(&conn, "proj1").unwrap();
        assert_eq!(got.id, "proj1");
        assert_eq!(got.workspace_id, "ws1");
        assert_eq!(got.name, Some("Test Project".to_string()));
        assert_eq!(got.root_path, "/tmp/test-project");
        assert_eq!(got.state, "active");
        assert_eq!(got.source_kind, "local");
    }

    #[test]
    fn get_not_found() {
        let conn = test_db();
        assert!(get(&conn, "nonexistent").is_err());
    }

    #[test]
    fn upsert_idempotent() {
        let conn = test_db();
        let row = make_row("proj_upsert");
        upsert(&conn, &row).unwrap();
        upsert(&conn, &row).unwrap();
        let got = get(&conn, "proj_upsert").unwrap();
        assert_eq!(got.id, "proj_upsert");
    }

    #[test]
    fn get_by_root_path_found() {
        let conn = test_db();
        insert(&conn, &make_row("proj_path")).unwrap();
        let got = get_by_root_path(&conn, "/tmp/test-project").unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().id, "proj_path");
    }

    #[test]
    fn get_by_root_path_not_found() {
        let conn = test_db();
        let got = get_by_root_path(&conn, "/nonexistent").unwrap();
        assert!(got.is_none());
    }

    #[test]
    fn list_for_workspace_ordering_and_limit() {
        let conn = test_db();
        for i in 0..5 {
            let mut row = make_row(&format!("proj_{i}"));
            row.root_path = format!("/tmp/project-{i}");
            row.updated_at = format!("2024-01-0{}T00:00:00Z", i + 1);
            insert(&conn, &row).unwrap();
        }
        let results = list_for_workspace(&conn, "ws1", 3).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "proj_4");
    }

    #[test]
    fn set_state_works() {
        let conn = test_db();
        insert(&conn, &make_row("proj_state")).unwrap();
        set_state(&conn, "proj_state", "archived").unwrap();
        let got = get(&conn, "proj_state").unwrap();
        assert_eq!(got.state, "archived");
    }

    #[test]
    fn update_name_works() {
        let conn = test_db();
        insert(&conn, &make_row("proj_name")).unwrap();
        update_name(&conn, "proj_name", "Renamed").unwrap();
        let got = get(&conn, "proj_name").unwrap();
        assert_eq!(got.name, Some("Renamed".to_string()));
    }

    #[test]
    fn delete_works() {
        let conn = test_db();
        insert(&conn, &make_row("proj_del")).unwrap();
        delete(&conn, "proj_del").unwrap();
        assert!(get(&conn, "proj_del").is_err());
    }

    #[test]
    fn delete_not_found() {
        let conn = test_db();
        assert!(delete(&conn, "nonexistent").is_err());
    }

    #[test]
    fn update_source_works() {
        let conn = test_db();
        insert(&conn, &make_row("proj_remote")).unwrap();
        update_source(
            &conn,
            "proj_remote",
            "ssh",
            Some(&ProjectSourceDetails {
                ssh_host: Some("devbox.example.com".to_string()),
                ssh_user: Some("farooq".to_string()),
                ssh_port: Some(2222),
                ssh_remote_path: Some("/srv/api".to_string()),
                ..Default::default()
            }),
        )
        .unwrap();

        let got = get(&conn, "proj_remote").unwrap();
        assert_eq!(got.source_kind, "ssh");
        assert_eq!(
            got.source_details,
            Some(ProjectSourceDetails {
                ssh_host: Some("devbox.example.com".to_string()),
                ssh_user: Some("farooq".to_string()),
                ssh_port: Some(2222),
                ssh_remote_path: Some("/srv/api".to_string()),
                ..Default::default()
            })
        );
    }
}
