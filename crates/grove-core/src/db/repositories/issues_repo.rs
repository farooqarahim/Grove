use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::db::repositories::projects_repo::IssueBoardConfig;
use crate::errors::GroveResult;
use crate::tracker::status::{self, CanonicalStatus};
use crate::tracker::{Issue, IssueUpdate};

// ── Public filter / result types ─────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct IssueFilter {
    pub provider: Option<String>,
    pub canonical_status: Option<CanonicalStatus>,
    pub label: Option<String>,
    pub assignee: Option<String>,
    pub priority: Option<String>,
    pub run_id: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

impl IssueFilter {
    pub fn new() -> Self {
        Self {
            limit: 100,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueBoard {
    pub columns: Vec<BoardColumn>,
    pub total: usize,
    pub sync_states: Vec<SyncState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardColumn {
    pub id: String,
    pub canonical_status: CanonicalStatus,
    pub label: String,
    pub issues: Vec<Issue>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueComment {
    pub id: i64,
    pub issue_id: String,
    pub body: String,
    pub author: Option<String>,
    pub posted_to_provider: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueEvent {
    pub id: i64,
    pub issue_id: String,
    pub event_type: String,
    pub actor: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub provider: String,
    pub project_id: String,
    pub last_synced_at: Option<String>,
    pub issues_synced: i64,
    pub last_error: Option<String>,
    pub sync_duration_ms: Option<i64>,
}

// ── Core CRUD ─────────────────────────────────────────────────────────────────

/// Insert or update an issue row.
///
/// Uses a composite upsert keyed on `(provider, external_id)` so that syncing
/// the same issue multiple times never creates duplicate rows.  `created_at` is
/// preserved across upserts via the `ON CONFLICT DO UPDATE` clause.
pub fn upsert(conn: &Connection, issue: &Issue, project_id: &str) -> GroveResult<()> {
    let labels_json = serde_json::to_string(&issue.labels).unwrap_or_else(|_| "[]".into());
    let raw_json = serde_json::to_string(&issue.raw_json).unwrap_or_else(|_| "{}".into());
    let provider_metadata_json =
        serde_json::to_string(&issue.provider_metadata).unwrap_or_else(|_| "{}".into());
    let now = Utc::now().to_rfc3339();
    let id = format!("{}:{}", issue.provider, issue.external_id);
    let canonical = status::normalize(&issue.provider, &issue.status).as_db_str();

    conn.execute(
        "INSERT INTO issues (
             id, external_id, provider, project_id,
             title, status, canonical_status, labels_json, body,
             external_url, assignee, synced_at, created_at, updated_at, raw_json,
             provider_native_id, provider_scope_type, provider_scope_key, provider_scope_name, provider_metadata_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
         ON CONFLICT(id) DO UPDATE SET
             title            = excluded.title,
             status           = excluded.status,
             canonical_status = excluded.canonical_status,
             labels_json      = excluded.labels_json,
             body             = excluded.body,
             external_url     = excluded.external_url,
             assignee         = excluded.assignee,
             synced_at        = excluded.synced_at,
             updated_at       = excluded.updated_at,
             raw_json         = excluded.raw_json,
             provider_native_id = excluded.provider_native_id,
             provider_scope_type = excluded.provider_scope_type,
             provider_scope_key = excluded.provider_scope_key,
             provider_scope_name = excluded.provider_scope_name,
             provider_metadata_json = excluded.provider_metadata_json",
        params![
            id,
            issue.external_id,
            issue.provider,
            project_id,
            issue.title,
            issue.status,
            canonical,
            labels_json,
            issue.body,
            issue.url,
            issue.assignee,
            now,
            now,
            now,
            raw_json,
            issue.provider_native_id,
            issue.provider_scope_type,
            issue.provider_scope_key,
            issue.provider_scope_name,
            provider_metadata_json,
        ],
    )?;
    Ok(())
}

/// Create a native Grove issue (not backed by any external tracker).
///
/// Generates a `grove:{uuid}` ID. Returns the newly created issue.
pub fn create_native(
    conn: &mut Connection,
    project_id: &str,
    title: &str,
    body: Option<&str>,
    priority: Option<&str>,
    labels: &[String],
) -> GroveResult<Issue> {
    let uuid = Uuid::new_v4().to_string();
    let id = format!("grove:{uuid}");
    let labels_json = serde_json::to_string(labels).unwrap_or_else(|_| "[]".into());
    let now = Utc::now().to_rfc3339();

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO issues (
             id, external_id, provider, project_id,
             title, status, canonical_status, priority, labels_json, body,
             is_native, created_at, updated_at, raw_json, provider_metadata_json
         ) VALUES (?1, ?2, 'grove', ?3, ?4, 'open', 'open', ?5, ?6, ?7, 1, ?8, ?9, '{}', '{}')",
        params![
            id,
            uuid,
            project_id,
            title,
            priority,
            labels_json,
            body,
            now,
            now
        ],
    )?;
    tx.commit()?;

    Ok(Issue {
        external_id: uuid.clone(),
        provider: "grove".to_string(),
        title: title.to_string(),
        status: "open".to_string(),
        labels: labels.to_vec(),
        body: body.map(|s| s.to_string()),
        url: None,
        assignee: None,
        raw_json: serde_json::json!({}),
        provider_native_id: None,
        provider_scope_type: None,
        provider_scope_key: None,
        provider_scope_name: None,
        provider_metadata: serde_json::json!({}),
        id: Some(id),
        project_id: Some(project_id.to_string()),
        canonical_status: Some("open".to_string()),
        priority: priority.map(|s| s.to_string()),
        is_native: true,
        created_at: Some(now.clone()),
        updated_at: Some(now),
        synced_at: None,
        run_id: None,
    })
}

/// Fetch a single issue by its composite `{provider}:{external_id}` primary key.
pub fn get(conn: &Connection, id: &str) -> GroveResult<Option<Issue>> {
    let row = conn
        .query_row(
            "SELECT external_id, provider, title, status, labels_json, body,
                    external_url, assignee, raw_json,
                    provider_native_id, provider_scope_type, provider_scope_key, provider_scope_name, provider_metadata_json,
                    id, project_id, canonical_status, priority,
                    COALESCE(is_native, 0), created_at, updated_at, synced_at, run_id
             FROM issues WHERE id = ?1",
            [id],
            row_to_issue,
        )
        .optional()?;
    Ok(row)
}

/// Fetch a single issue by provider + external_id + project scope.
pub fn get_by_external(
    conn: &Connection,
    provider: &str,
    external_id: &str,
    project_id: &str,
) -> GroveResult<Option<Issue>> {
    let row = conn
        .query_row(
            "SELECT external_id, provider, title, status, labels_json, body,
                    external_url, assignee, raw_json,
                    provider_native_id, provider_scope_type, provider_scope_key, provider_scope_name, provider_metadata_json,
                    id, project_id, canonical_status, priority,
                    COALESCE(is_native, 0), created_at, updated_at, synced_at, run_id
             FROM issues WHERE provider = ?1 AND external_id = ?2 AND project_id = ?3",
            params![provider, external_id, project_id],
            row_to_issue,
        )
        .optional()?;
    Ok(row)
}

/// List issues for a project, with optional filtering and pagination.
pub fn list(conn: &Connection, project_id: &str, filter: &IssueFilter) -> GroveResult<Vec<Issue>> {
    let (where_clause, bound_values) = build_filter_clause(project_id, filter);
    let limit = if filter.limit == 0 { 100 } else { filter.limit };
    let sql = format!(
        "SELECT external_id, provider, title, status, labels_json, body,
                external_url, assignee, raw_json,
                provider_native_id, provider_scope_type, provider_scope_key, provider_scope_name, provider_metadata_json,
                id, project_id, canonical_status, priority,
                COALESCE(is_native, 0), created_at, updated_at, synced_at, run_id
         FROM issues
         {where_clause}
         ORDER BY updated_at DESC
         LIMIT {limit} OFFSET {}",
        filter.offset
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(
            rusqlite::params_from_iter(bound_values.iter().map(|s| s.as_str())),
            |r| row_to_issue(r),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Return a kanban board view: issues grouped by canonical status column.
pub fn board_view(
    conn: &Connection,
    project_id: &str,
    filter: &IssueFilter,
) -> GroveResult<IssueBoard> {
    let all_issues = list(
        conn,
        project_id,
        &IssueFilter {
            limit: if filter.limit == 0 {
                200
            } else {
                filter.limit.min(500)
            },
            ..filter.clone()
        },
    )?;

    let total = all_issues.len();
    let settings = crate::db::repositories::projects_repo::get_settings(conn, project_id)?;
    let board_config = IssueBoardConfig::normalized_or_default(settings.issue_board);

    let mut columns: Vec<BoardColumn> = board_config
        .columns
        .iter()
        .map(|column| BoardColumn {
            id: column.id.clone(),
            canonical_status: column.canonical_status,
            label: column.label.clone(),
            issues: Vec::new(),
            count: 0,
        })
        .collect();

    for issue in all_issues {
        let column_id = match_column_id(&board_config, &issue)
            .or_else(|| {
                fallback_column_id(
                    &board_config,
                    status::normalize(&issue.provider, &issue.status),
                )
            })
            .or_else(|| board_config.columns.first().map(|column| column.id.clone()));

        if let Some(column_id) = column_id {
            if let Some(col) = columns.iter_mut().find(|c| c.id == column_id) {
                col.issues.push(issue);
                col.count += 1;
            }
        }
    }

    let sync_states = get_sync_states(conn, project_id)?;

    Ok(IssueBoard {
        columns,
        total,
        sync_states,
    })
}

pub fn resolve_column_target_status(
    conn: &Connection,
    project_id: &str,
    column_id: &str,
    provider: &str,
) -> GroveResult<Option<String>> {
    let settings = crate::db::repositories::projects_repo::get_settings(conn, project_id)?;
    let board_config = IssueBoardConfig::normalized_or_default(settings.issue_board);
    let Some(column) = board_config
        .columns
        .iter()
        .find(|column| column.id == column_id)
    else {
        return Ok(None);
    };

    if let Some(target) = column.provider_targets.get(provider) {
        return Ok(Some(target.clone()));
    }

    Ok(Some(
        status::denormalize(provider, &column.canonical_status).to_string(),
    ))
}

fn match_column_id(config: &IssueBoardConfig, issue: &Issue) -> Option<String> {
    let normalized_status = normalize_status_key(&issue.status);
    config.columns.iter().find_map(|column| {
        column
            .match_rules
            .get(&issue.provider)
            .and_then(|statuses| {
                statuses
                    .iter()
                    .any(|status| normalize_status_key(status) == normalized_status)
                    .then(|| column.id.clone())
            })
    })
}

fn fallback_column_id(
    config: &IssueBoardConfig,
    canonical_status: CanonicalStatus,
) -> Option<String> {
    config
        .columns
        .iter()
        .find(|column| column.canonical_status == canonical_status)
        .map(|column| column.id.clone())
}

fn normalize_status_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

// ── Mutations ─────────────────────────────────────────────────────────────────

/// Update the status of an issue and automatically record a `status_changed` event.
pub fn update_status(
    conn: &mut Connection,
    id: &str,
    status: &str,
    canonical: CanonicalStatus,
) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();

    let old_status: Option<String> = conn
        .query_row("SELECT status FROM issues WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?;

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "UPDATE issues SET status = ?1, canonical_status = ?2, updated_at = ?3 WHERE id = ?4",
        params![status, canonical.as_db_str(), now, id],
    )?;
    tx.execute(
        "INSERT INTO issue_events (issue_id, event_type, old_value, new_value, created_at)
         VALUES (?1, 'status_changed', ?2, ?3, ?4)",
        params![id, old_status.as_deref(), status, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// Update mutable fields on an issue and record a `synced` event.
pub fn update_fields(conn: &mut Connection, id: &str, update: &IssueUpdate) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    if let Some(title) = &update.title {
        tx.execute(
            "UPDATE issues SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, id],
        )?;
    }
    if let Some(body) = &update.body {
        tx.execute(
            "UPDATE issues SET body = ?1, updated_at = ?2 WHERE id = ?3",
            params![body, now, id],
        )?;
    }
    if let Some(assignee) = &update.assignee {
        tx.execute(
            "UPDATE issues SET assignee = ?1, updated_at = ?2 WHERE id = ?3",
            params![assignee, now, id],
        )?;
    }
    if let Some(priority) = &update.priority {
        tx.execute(
            "UPDATE issues SET priority = ?1, updated_at = ?2 WHERE id = ?3",
            params![priority, now, id],
        )?;
    }
    if let Some(labels) = &update.labels {
        let labels_json = serde_json::to_string(labels).unwrap_or_else(|_| "[]".into());
        tx.execute(
            "UPDATE issues SET labels_json = ?1, updated_at = ?2 WHERE id = ?3",
            params![labels_json, now, id],
        )?;
    }
    if let Some(status) = &update.status {
        let canonical = status::normalize("grove", status).as_db_str();
        tx.execute(
            "UPDATE issues SET status = ?1, canonical_status = ?2, updated_at = ?3 WHERE id = ?4",
            params![status, canonical, now, id],
        )?;
    }

    tx.execute(
        "INSERT INTO issue_events (issue_id, event_type, created_at) VALUES (?1, 'synced', ?2)",
        params![id, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// Set the `run_id` on an issue (latest Grove run linked to this issue).
pub fn link_run(conn: &Connection, issue_id: &str, run_id: &str) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE issues SET run_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![run_id, now, issue_id],
    )?;
    Ok(())
}

/// Delete an issue and all related comments/events (via ON DELETE CASCADE).
pub fn delete(conn: &Connection, id: &str) -> GroveResult<()> {
    conn.execute("DELETE FROM issues WHERE id = ?1", [id])?;
    Ok(())
}

// ── Comments ──────────────────────────────────────────────────────────────────

/// Append a comment to an issue. Returns the new row's `id`.
pub fn add_comment(
    conn: &mut Connection,
    issue_id: &str,
    body: &str,
    author: &str,
    posted: bool,
) -> GroveResult<i64> {
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO issue_comments (issue_id, body, author, posted_to_provider, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![issue_id, body, author, posted as i32, now],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(id)
}

/// List all comments for an issue, ordered oldest first.
pub fn list_comments(conn: &Connection, issue_id: &str) -> GroveResult<Vec<IssueComment>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, issue_id, body, author, posted_to_provider, created_at
         FROM issue_comments WHERE issue_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([issue_id], |r| {
            Ok(IssueComment {
                id: r.get(0)?,
                issue_id: r.get(1)?,
                body: r.get(2)?,
                author: r.get(3)?,
                posted_to_provider: r.get::<_, i32>(4)? != 0,
                created_at: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── Events ────────────────────────────────────────────────────────────────────

/// Record an audit event on an issue.
pub fn record_event(
    conn: &mut Connection,
    issue_id: &str,
    event_type: &str,
    actor: &str,
    old_val: Option<&str>,
    new_val: Option<&str>,
) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO issue_events (issue_id, event_type, actor, old_value, new_value, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![issue_id, event_type, actor, old_val, new_val, now],
    )?;
    tx.commit()?;
    Ok(())
}

/// List all events for an issue, ordered oldest first.
pub fn list_events(conn: &Connection, issue_id: &str) -> GroveResult<Vec<IssueEvent>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, issue_id, event_type, actor, old_value, new_value, payload_json, created_at
         FROM issue_events WHERE issue_id = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([issue_id], |r| {
            let payload_str: Option<String> = r.get(6)?;
            Ok(IssueEvent {
                id: r.get(0)?,
                issue_id: r.get(1)?,
                event_type: r.get(2)?,
                actor: r.get(3)?,
                old_value: r.get(4)?,
                new_value: r.get(5)?,
                payload: payload_str
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or(Value::Object(Default::default())),
                created_at: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ── Sync state ────────────────────────────────────────────────────────────────

/// Return all sync states for a project (one per provider).
pub fn get_sync_states(conn: &Connection, project_id: &str) -> GroveResult<Vec<SyncState>> {
    let mut stmt = conn.prepare_cached(
        "SELECT provider, project_id, last_synced_at, issues_synced, last_error, sync_duration_ms
         FROM issue_sync_state WHERE project_id = ?1",
    )?;
    let rows = stmt
        .query_map([project_id], |r| {
            Ok(SyncState {
                provider: r.get(0)?,
                project_id: r.get(1)?,
                last_synced_at: r.get(2)?,
                issues_synced: r.get(3)?,
                last_error: r.get(4)?,
                sync_duration_ms: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Upsert a sync state row after a sync run completes.
pub fn update_sync_state(
    conn: &mut Connection,
    provider: &str,
    project_id: &str,
    count: usize,
    error: Option<&str>,
    duration_ms: u64,
) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO issue_sync_state (provider, project_id, last_synced_at, issues_synced, last_error, sync_duration_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(provider, project_id) DO UPDATE SET
             last_synced_at   = excluded.last_synced_at,
             issues_synced    = excluded.issues_synced,
             last_error       = excluded.last_error,
             sync_duration_ms = excluded.sync_duration_ms",
        params![provider, project_id, now, count as i64, error, duration_ms as i64],
    )?;
    tx.commit()?;
    Ok(())
}

/// Count issues that are not yet done/cancelled for a project.
///
/// "Active" = `canonical_status IN ('open', 'in_progress', 'in_review', 'blocked')`.
pub fn count_open(conn: &Connection, project_id: &str) -> GroveResult<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM issues
         WHERE project_id = ?1
           AND canonical_status IN ('open', 'in_progress', 'in_review', 'blocked')",
        [project_id],
        |r| r.get(0),
    )?;
    Ok(count as usize)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn row_to_issue(r: &rusqlite::Row<'_>) -> rusqlite::Result<Issue> {
    let external_id: String = r.get(0)?;
    let provider: String = r.get(1)?;
    let title: String = r.get(2)?;
    let status: String = r.get(3)?;
    let labels_json: String = r.get(4)?;
    let body: Option<String> = r.get(5)?;
    let url: Option<String> = r.get(6)?;
    let assignee: Option<String> = r.get(7)?;
    let raw_json_str: String = r.get(8)?;
    let provider_native_id: Option<String> = r.get(9)?;
    let provider_scope_type: Option<String> = r.get(10)?;
    let provider_scope_key: Option<String> = r.get(11)?;
    let provider_scope_name: Option<String> = r.get(12)?;
    let provider_metadata_json: String = r.get(13)?;
    // DB-enriched fields (columns 14–21)
    let id: Option<String> = r.get(14)?;
    let project_id: Option<String> = r.get(15)?;
    let canonical_status: Option<String> = r.get(16)?;
    let priority: Option<String> = r.get(17)?;
    let is_native_int: i32 = r.get(18).unwrap_or(0);
    let created_at: Option<String> = r.get(19)?;
    let updated_at: Option<String> = r.get(20)?;
    let synced_at: Option<String> = r.get(21)?;
    let run_id: Option<String> = r.get(22).unwrap_or(None);

    let labels: Vec<String> = serde_json::from_str(&labels_json).unwrap_or_default();
    let raw_json: Value =
        serde_json::from_str(&raw_json_str).unwrap_or(Value::Object(Default::default()));
    let provider_metadata: Value =
        serde_json::from_str(&provider_metadata_json).unwrap_or(Value::Object(Default::default()));

    Ok(Issue {
        external_id,
        provider,
        title,
        status,
        labels,
        body,
        url,
        assignee,
        raw_json,
        provider_native_id,
        provider_scope_type,
        provider_scope_key,
        provider_scope_name,
        provider_metadata,
        id,
        project_id,
        canonical_status,
        priority,
        is_native: is_native_int != 0,
        created_at,
        updated_at,
        synced_at,
        run_id,
    })
}

/// Build a WHERE clause and bound parameter list from an `IssueFilter`.
fn build_filter_clause(project_id: &str, filter: &IssueFilter) -> (String, Vec<String>) {
    let mut conditions = vec![format!("project_id = '{}'", project_id.replace('\'', "''"))];
    let values: Vec<String> = Vec::new();

    if let Some(provider) = &filter.provider {
        conditions.push(format!("provider = '{}'", provider.replace('\'', "''")));
    }
    if let Some(cs) = &filter.canonical_status {
        conditions.push(format!("canonical_status = '{}'", cs.as_db_str()));
    }
    if let Some(assignee) = &filter.assignee {
        conditions.push(format!("assignee = '{}'", assignee.replace('\'', "''")));
    }
    if let Some(priority) = &filter.priority {
        conditions.push(format!("priority = '{}'", priority.replace('\'', "''")));
    }
    if let Some(run_id) = &filter.run_id {
        conditions.push(format!("run_id = '{}'", run_id.replace('\'', "''")));
    }
    if let Some(label) = &filter.label {
        // labels_json is a JSON array — check if it contains the given label string.
        conditions.push(format!(
            "labels_json LIKE '%{}%'",
            label.replace('\'', "''").replace('%', r"\%")
        ));
    }

    let _ = values; // values unused — conditions use inline literals after sanitisation
    let clause = format!("WHERE {}", conditions.join(" AND "));
    (clause, vec![])
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::DbHandle;

    fn setup() -> (tempfile::TempDir, rusqlite::Connection) {
        let dir = tempfile::tempdir().unwrap();
        db::initialize(dir.path()).unwrap();
        let handle = DbHandle::new(dir.path());
        let conn = handle.connect().unwrap();
        // Insert a workspace + project "p1" so tests that call update_settings
        // or board_view can find the project.
        let now = chrono::Utc::now().to_rfc3339();
        crate::db::repositories::workspaces_repo::insert(
            &conn,
            &crate::db::repositories::workspaces_repo::WorkspaceRow {
                id: "ws1".to_string(),
                name: None,
                state: "active".to_string(),
                created_at: now.clone(),
                updated_at: now.clone(),
                credits_usd: 0.0,
                llm_provider: None,
                llm_model: None,
                llm_auth_mode: "user_key".to_string(),
            },
        )
        .unwrap();
        crate::db::repositories::projects_repo::insert(
            &conn,
            &crate::db::repositories::projects_repo::ProjectRow {
                id: "p1".to_string(),
                workspace_id: "ws1".to_string(),
                name: Some("Test Project".to_string()),
                root_path: "/tmp/test-project".to_string(),
                state: "active".to_string(),
                created_at: now.clone(),
                updated_at: now,
                base_ref: None,
                source_kind: "local".to_string(),
                source_details: None,
            },
        )
        .unwrap();
        (dir, conn)
    }

    fn gh_issue(id: &str, title: &str) -> Issue {
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

    // ── upsert / get ──────────────────────────────────────────────────────────

    #[test]
    fn upsert_and_get_round_trip() {
        let (_dir, conn) = setup();
        let issue = Issue {
            external_id: "42".into(),
            provider: "github".into(),
            title: "Fix login bug".into(),
            status: "open".into(),
            labels: vec!["bug".into()],
            body: Some("description".into()),
            url: Some("https://github.com/org/repo/issues/42".into()),
            assignee: Some("alice".into()),
            raw_json: serde_json::json!({"number": 42}),
            provider_native_id: Some("issue-node-42".into()),
            provider_scope_type: Some("repository".into()),
            provider_scope_key: Some("org/repo".into()),
            provider_scope_name: Some("org/repo".into()),
            provider_metadata: serde_json::json!({"label_names": ["bug"]}),
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

        upsert(&conn, &issue, "proj-1").unwrap();
        let fetched = get(&conn, "github:42").unwrap().expect("issue must exist");
        assert_eq!(fetched.external_id, "42");
        assert_eq!(fetched.provider, "github");
        assert_eq!(fetched.title, "Fix login bug");
        assert_eq!(fetched.labels, vec!["bug"]);
        assert_eq!(fetched.assignee.as_deref(), Some("alice"));
        assert_eq!(fetched.provider_native_id.as_deref(), Some("issue-node-42"));
        assert_eq!(fetched.provider_scope_key.as_deref(), Some("org/repo"));
    }

    #[test]
    fn upsert_is_idempotent() {
        let (_dir, conn) = setup();
        let issue = gh_issue("1", "Original");
        upsert(&conn, &issue, "p1").unwrap();

        let updated = Issue {
            title: "Updated".into(),
            ..issue.clone()
        };
        upsert(&conn, &updated, "p1").unwrap();
        upsert(&conn, &updated, "p1").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM issues WHERE external_id='1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let fetched = get(&conn, "github:1").unwrap().unwrap();
        assert_eq!(fetched.title, "Updated");
    }

    #[test]
    fn upsert_preserves_created_at() {
        let (_dir, conn) = setup();
        let issue = gh_issue("5", "Issue");
        upsert(&conn, &issue, "p1").unwrap();
        let created_first: String = conn
            .query_row(
                "SELECT created_at FROM issues WHERE id='github:5'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));
        upsert(&conn, &issue, "p1").unwrap();
        let created_second: String = conn
            .query_row(
                "SELECT created_at FROM issues WHERE id='github:5'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(
            created_first, created_second,
            "created_at must not change on re-upsert"
        );
    }

    // ── get_by_external ───────────────────────────────────────────────────────

    #[test]
    fn get_by_external_scoped_to_project() {
        let (_dir, conn) = setup();
        // Each project uses a distinct external_id — the schema's UNIQUE(provider, external_id)
        // means the same numeric ID can't be stored for two separate projects.
        upsert(&conn, &gh_issue("10", "Issue A"), "proj-1").unwrap();
        upsert(&conn, &gh_issue("11", "Issue B"), "proj-2").unwrap();

        let a = get_by_external(&conn, "github", "10", "proj-1")
            .unwrap()
            .unwrap();
        assert_eq!(a.title, "Issue A");

        let b = get_by_external(&conn, "github", "11", "proj-2")
            .unwrap()
            .unwrap();
        assert_eq!(b.title, "Issue B");

        // Wrong project for issue "10" returns None.
        let none = get_by_external(&conn, "github", "10", "proj-2").unwrap();
        assert!(none.is_none());
    }

    // ── list ──────────────────────────────────────────────────────────────────

    #[test]
    fn list_returns_only_project_issues() {
        let (_dir, conn) = setup();
        for i in 1..=3 {
            upsert(
                &conn,
                &gh_issue(&i.to_string(), &format!("Issue {i}")),
                "p1",
            )
            .unwrap();
        }
        upsert(&conn, &gh_issue("99", "Other"), "p2").unwrap();

        let issues = list(&conn, "p1", &IssueFilter::new()).unwrap();
        assert_eq!(issues.len(), 3);
    }

    #[test]
    fn list_filter_by_canonical_status() {
        let (_dir, conn) = setup();
        let open_issue = gh_issue("1", "Open");
        let closed_issue = Issue {
            status: "closed".into(),
            external_id: "2".into(),
            title: "Closed".into(),
            ..gh_issue("2", "Closed")
        };

        upsert(&conn, &open_issue, "p1").unwrap();
        upsert(&conn, &closed_issue, "p1").unwrap();

        let open_only = list(
            &conn,
            "p1",
            &IssueFilter {
                canonical_status: Some(CanonicalStatus::Open),
                limit: 100,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(open_only.len(), 1);
        assert_eq!(open_only[0].external_id, "1");
    }

    // ── create_native ─────────────────────────────────────────────────────────

    #[test]
    fn create_native_generates_grove_id() {
        let (_dir, mut conn) = setup();
        let issue =
            create_native(&mut conn, "p1", "Native Issue", Some("body"), None, &[]).unwrap();
        assert_eq!(issue.provider, "grove");

        let id = format!("grove:{}", issue.external_id);
        let fetched = get(&conn, &id)
            .unwrap()
            .expect("native issue must be retrievable");
        assert_eq!(fetched.title, "Native Issue");
    }

    #[test]
    fn create_native_unique_ids() {
        let (_dir, mut conn) = setup();
        let a = create_native(&mut conn, "p1", "A", None, None, &[]).unwrap();
        let b = create_native(&mut conn, "p1", "B", None, None, &[]).unwrap();
        assert_ne!(a.external_id, b.external_id);
    }

    // ── update_status ─────────────────────────────────────────────────────────

    #[test]
    fn update_status_records_event() {
        let (_dir, mut conn) = setup();
        upsert(&conn, &gh_issue("7", "Test"), "p1").unwrap();

        update_status(&mut conn, "github:7", "closed", CanonicalStatus::Done).unwrap();

        let fetched = get(&conn, "github:7").unwrap().unwrap();
        assert_eq!(fetched.status, "closed");

        let events = list_events(&conn, "github:7").unwrap();
        assert!(events.iter().any(|e| e.event_type == "status_changed"));
    }

    // ── comments ─────────────────────────────────────────────────────────────

    #[test]
    fn add_and_list_comments() {
        let (_dir, mut conn) = setup();
        upsert(&conn, &gh_issue("3", "Issue"), "p1").unwrap();

        let id = add_comment(&mut conn, "github:3", "First comment", "alice", false).unwrap();
        assert!(id > 0);
        add_comment(&mut conn, "github:3", "Second comment", "bob", true).unwrap();

        let comments = list_comments(&conn, "github:3").unwrap();
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].body, "First comment");
        assert!(!comments[0].posted_to_provider);
        assert!(comments[1].posted_to_provider);
    }

    // ── count_open ────────────────────────────────────────────────────────────

    #[test]
    fn count_open_excludes_done_and_cancelled() {
        let (_dir, conn) = setup();
        upsert(&conn, &gh_issue("1", "Open"), "p1").unwrap();
        upsert(
            &conn,
            &Issue {
                external_id: "2".into(),
                title: "Closed".into(),
                status: "closed".into(),
                provider: "github".into(),
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
            },
            "p1",
        )
        .unwrap();
        upsert(
            &conn,
            &Issue {
                external_id: "3".into(),
                title: "In Progress".into(),
                status: "in_progress".into(),
                provider: "grove".into(),
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
            },
            "p1",
        )
        .unwrap();

        // Manually set canonical_status since 'in_progress' isn't a GitHub status
        conn.execute(
            "UPDATE issues SET canonical_status = 'in_progress' WHERE external_id = '3'",
            [],
        )
        .unwrap();

        let open = count_open(&conn, "p1").unwrap();
        assert_eq!(open, 2); // id 1 (open) + id 3 (in_progress)
    }

    // ── sync_state ────────────────────────────────────────────────────────────

    #[test]
    fn update_and_get_sync_state() {
        let (_dir, mut conn) = setup();
        update_sync_state(&mut conn, "github", "proj-1", 15, None, 1200).unwrap();
        update_sync_state(&mut conn, "jira", "proj-1", 5, Some("auth failed"), 0).unwrap();

        let states = get_sync_states(&conn, "proj-1").unwrap();
        assert_eq!(states.len(), 2);
        let gh = states.iter().find(|s| s.provider == "github").unwrap();
        assert_eq!(gh.issues_synced, 15);
        assert!(gh.last_error.is_none());

        let jira = states.iter().find(|s| s.provider == "jira").unwrap();
        assert_eq!(jira.last_error.as_deref(), Some("auth failed"));
    }

    // ── link_run ──────────────────────────────────────────────────────────────

    #[test]
    fn link_run_sets_run_id() {
        let (_dir, conn) = setup();
        upsert(&conn, &gh_issue("20", "Issue"), "p1").unwrap();
        link_run(&conn, "github:20", "run-xyz").unwrap();

        let run_id: Option<String> = conn
            .query_row(
                "SELECT run_id FROM issues WHERE id = 'github:20'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(run_id.as_deref(), Some("run-xyz"));
    }

    // ── board_view ────────────────────────────────────────────────────────────

    #[test]
    fn board_view_groups_by_canonical_status() {
        let (_dir, conn) = setup();
        upsert(&conn, &gh_issue("1", "Open issue"), "p1").unwrap();
        upsert(
            &conn,
            &Issue {
                external_id: "2".into(),
                title: "Closed issue".into(),
                status: "closed".into(),
                provider: "github".into(),
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
            },
            "p1",
        )
        .unwrap();

        let board = board_view(&conn, "p1", &IssueFilter::new()).unwrap();
        assert_eq!(board.total, 2);

        let open_col = board
            .columns
            .iter()
            .find(|c| c.canonical_status == CanonicalStatus::Open)
            .unwrap();
        assert_eq!(open_col.count, 1);

        let done_col = board
            .columns
            .iter()
            .find(|c| c.canonical_status == CanonicalStatus::Done)
            .unwrap();
        assert_eq!(done_col.count, 1);
    }

    #[test]
    fn board_view_uses_project_board_configuration() {
        let (_dir, conn) = setup();
        upsert(
            &conn,
            &Issue {
                external_id: "ENG-10".into(),
                provider: "jira".into(),
                title: "Backlog item".into(),
                status: "Backlog".into(),
                labels: vec![],
                body: None,
                url: None,
                assignee: None,
                raw_json: serde_json::json!({}),
                provider_native_id: Some("10010".into()),
                provider_scope_type: Some("project".into()),
                provider_scope_key: Some("ENG".into()),
                provider_scope_name: Some("Engineering".into()),
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
            },
            "p1",
        )
        .unwrap();

        crate::db::repositories::projects_repo::update_settings(
            &conn,
            "p1",
            &crate::db::repositories::projects_repo::ProjectSettings {
                issue_board: Some(IssueBoardConfig {
                    columns: vec![
                        crate::db::repositories::projects_repo::IssueBoardColumnConfig {
                            id: "queued".into(),
                            label: "Queued".into(),
                            canonical_status: CanonicalStatus::Open,
                            match_rules: [("jira".to_string(), vec!["Backlog".to_string()])]
                                .into_iter()
                                .collect(),
                            provider_targets: Default::default(),
                        },
                        crate::db::repositories::projects_repo::IssueBoardColumnConfig {
                            id: "doing".into(),
                            label: "Doing".into(),
                            canonical_status: CanonicalStatus::InProgress,
                            match_rules: Default::default(),
                            provider_targets: Default::default(),
                        },
                    ],
                }),
                ..Default::default()
            },
        )
        .unwrap();

        let board = board_view(&conn, "p1", &IssueFilter::new()).unwrap();
        assert_eq!(board.columns[0].id, "queued");
        assert_eq!(board.columns[0].label, "Queued");
        assert_eq!(board.columns[0].count, 1);
        assert_eq!(board.columns[0].issues[0].external_id, "ENG-10");
    }

    #[test]
    fn resolve_column_target_status_prefers_configured_provider_target() {
        let (_dir, conn) = setup();
        crate::db::repositories::projects_repo::update_settings(
            &conn,
            "p1",
            &crate::db::repositories::projects_repo::ProjectSettings {
                issue_board: Some(IssueBoardConfig {
                    columns: vec![
                        crate::db::repositories::projects_repo::IssueBoardColumnConfig {
                            id: "triage".into(),
                            label: "Triage".into(),
                            canonical_status: CanonicalStatus::Open,
                            match_rules: Default::default(),
                            provider_targets: [(
                                "jira".to_string(),
                                "Selected for Development".to_string(),
                            )]
                            .into_iter()
                            .collect(),
                        },
                    ],
                }),
                ..Default::default()
            },
        )
        .unwrap();

        let jira_target = resolve_column_target_status(&conn, "p1", "triage", "jira").unwrap();
        assert_eq!(jira_target.as_deref(), Some("Selected for Development"));

        let github_target = resolve_column_target_status(&conn, "p1", "triage", "github").unwrap();
        assert_eq!(github_target.as_deref(), Some("open"));
    }
}
