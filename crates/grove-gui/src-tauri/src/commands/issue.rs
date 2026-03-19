use tauri::State;

use grove_core::tracker::{Issue, TrackerBackend};

use super::{
    CONNECTION_CACHE_TTL, CONNECTION_STATUS_CACHE, CachedConnectionStatus, ConnectionStatusDto,
};
use crate::state::AppState;

// ── Issue Tracker ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_issues(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> Result<Vec<Issue>, String> {
    let pid = match project_id {
        Some(id) => id,
        None => return Ok(vec![]),
    };
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::tracker::list_cached(&conn, &pid).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_issue(
    state: State<'_, AppState>,
    title: String,
    body: String,
    project_id: Option<String>,
) -> Result<Issue, String> {
    let cfg = grove_core::config::GroveConfig::load_or_create(state.workspace_root())
        .map_err(|e| e.to_string())?;
    let backend = grove_core::tracker::build_backend(&cfg, state.workspace_root())
        .map_err(|e| e.to_string())?;
    let issue = backend.create(&title, &body).map_err(|e| e.to_string())?;
    // Cache locally
    let pid = match project_id {
        Some(id) => id,
        None => return Ok(issue), // skip caching when no project selected
    };
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::tracker::cache_issue(&conn, &issue, &pid).ok();
    Ok(issue)
}

#[tauri::command]
pub fn close_issue(state: State<'_, AppState>, external_id: String) -> Result<(), String> {
    let cfg = grove_core::config::GroveConfig::load_or_create(state.workspace_root())
        .map_err(|e| e.to_string())?;
    let backend = grove_core::tracker::build_backend(&cfg, state.workspace_root())
        .map_err(|e| e.to_string())?;
    backend.close(&external_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn refresh_issues(
    state: State<'_, AppState>,
    project_id: Option<String>,
) -> Result<Vec<Issue>, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        let backend =
            grove_core::tracker::build_backend(&cfg, &workspace_root).map_err(|e| e.to_string())?;
        let issues = backend.list().map_err(|e| e.to_string())?;
        let pid = match project_id {
            Some(id) => id,
            None => return Ok(issues),
        };
        let conn = pool.get().map_err(|e| e.to_string())?;
        for issue in &issues {
            grove_core::tracker::cache_issue(&conn, issue, &pid).ok();
        }
        Ok(issues)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Connections (issue tracker providers) ────────────────────────────────────

#[tauri::command]
pub async fn check_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionStatusDto>, String> {
    // Return cached result if fresh (avoids hitting 3 APIs every 30s from multiple components)
    {
        let cache = CONNECTION_STATUS_CACHE.lock();
        if let Some((ref cached, fetched_at)) = *cache {
            if fetched_at.elapsed() < CONNECTION_CACHE_TTL {
                return Ok(cached
                    .iter()
                    .map(|c| ConnectionStatusDto {
                        provider: c.provider.clone(),
                        connected: c.connected,
                        user_display: c.user_display.clone(),
                        error: c.error.clone(),
                    })
                    .collect());
            }
        }
    }

    let workspace_root = state.workspace_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;

        let mut statuses = Vec::new();

        // GitHub
        let gh =
            grove_core::tracker::github::GitHubTracker::new(&workspace_root, &cfg.tracker.github);
        let gh_s = gh.check_connection();
        statuses.push(ConnectionStatusDto {
            provider: gh_s.provider,
            connected: gh_s.connected,
            user_display: gh_s.user_display,
            error: gh_s.error,
        });

        // Jira
        let jira = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
        let jira_s = jira.check_connection();
        statuses.push(ConnectionStatusDto {
            provider: jira_s.provider,
            connected: jira_s.connected,
            user_display: jira_s.user_display,
            error: jira_s.error,
        });

        // Linear
        let linear = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
        let linear_s = linear.check_connection();
        statuses.push(ConnectionStatusDto {
            provider: linear_s.provider,
            connected: linear_s.connected,
            user_display: linear_s.user_display,
            error: linear_s.error,
        });

        // Update cache
        let cached: Vec<CachedConnectionStatus> = statuses
            .iter()
            .map(|s| CachedConnectionStatus {
                provider: s.provider.clone(),
                connected: s.connected,
                user_display: s.user_display.clone(),
                error: s.error.clone(),
            })
            .collect();
        *CONNECTION_STATUS_CACHE.lock() = Some((cached, std::time::Instant::now()));

        Ok(statuses)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn connect_provider(
    state: State<'_, AppState>,
    provider: String,
    credentials: serde_json::Value,
    storage: String,
) -> Result<ConnectionStatusDto, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        // Invalidate connection cache — credentials changed
        *CONNECTION_STATUS_CACHE.lock() = None;

        match provider.as_str() {
            "github" => {
                let token = credentials
                    .get("token")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'token' in credentials")?;
                let gh = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                gh.authenticate(token).map_err(|e| e.to_string())?;
                let s = gh.check_connection();
                Ok(ConnectionStatusDto {
                    provider: s.provider,
                    connected: s.connected,
                    user_display: s.user_display,
                    error: s.error,
                })
            }
            "jira" => {
                let email = credentials
                    .get("email")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'email' in credentials")?;
                let token = credentials
                    .get("token")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'token' in credentials")?;
                let site = credentials
                    .get("site")
                    .and_then(|v| v.as_str())
                    .unwrap_or(&cfg.tracker.jira.site_url);
                let jira_cfg = grove_core::config::JiraTrackerConfig {
                    site_url: site.to_string(),
                    email: email.to_string(),
                    ..cfg.tracker.jira.clone()
                };
                let storage = grove_core::tracker::credentials::CredentialStorage::parse(&storage)
                    .map_err(|e| e.to_string())?;
                let jira = grove_core::tracker::jira::JiraTracker::new(&jira_cfg);
                jira.save_credentials(email, token, storage)
                    .map_err(|e| e.to_string())?;
                let s = jira.check_connection();
                Ok(ConnectionStatusDto {
                    provider: s.provider,
                    connected: s.connected,
                    user_display: s.user_display,
                    error: s.error,
                })
            }
            "linear" => {
                let token = credentials
                    .get("token")
                    .and_then(|v| v.as_str())
                    .ok_or("missing 'token' in credentials")?;
                let storage = grove_core::tracker::credentials::CredentialStorage::parse(&storage)
                    .map_err(|e| e.to_string())?;
                let linear = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                linear
                    .save_token(token, storage)
                    .map_err(|e| e.to_string())?;
                let s = linear.check_connection();
                Ok(ConnectionStatusDto {
                    provider: s.provider,
                    connected: s.connected,
                    user_display: s.user_display,
                    error: s.error,
                })
            }
            _ => Err(format!("unknown provider '{provider}'")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn disconnect_provider(provider: String) -> Result<(), String> {
    // Invalidate connection cache — credentials removed
    *CONNECTION_STATUS_CACHE.lock() = None;

    match provider.as_str() {
        "github" => grove_core::tracker::credentials::CredentialStore::delete_provider("github")
            .map_err(|e| e.to_string()),
        "jira" => grove_core::tracker::credentials::CredentialStore::delete_provider("jira")
            .map_err(|e| e.to_string()),
        "linear" => grove_core::tracker::credentials::CredentialStore::delete_provider("linear")
            .map_err(|e| e.to_string()),
        _ => Err(format!("unknown provider '{provider}'")),
    }
}

/// List available statuses/states/labels for a provider, scoped to the given project.
///
/// Returns a flat list of ProviderStatus entries the user can pick as workflow
/// transition targets in project settings.
#[tauri::command]
pub async fn list_provider_statuses(
    state: State<'_, AppState>,
    provider: String,
    project_id: Option<String>,
) -> Result<Vec<grove_core::tracker::ProviderStatus>, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;

        // Resolve provider-specific key from project settings.
        let project_key: Option<String> = project_id.as_deref().and_then(|pid| {
            let conn = pool.get().ok()?;
            let settings =
                grove_core::db::repositories::projects_repo::get_settings(&conn, pid).ok()?;
            settings.project_key_for(&provider).map(|s| s.to_string())
        });

        match provider.as_str() {
            "grove" => Ok(vec![
                grove_core::tracker::ProviderStatus {
                    id: "backlog".into(),
                    name: "Backlog".into(),
                    category: "backlog".into(),
                    color: None,
                },
                grove_core::tracker::ProviderStatus {
                    id: "todo".into(),
                    name: "To Do".into(),
                    category: "todo".into(),
                    color: None,
                },
                grove_core::tracker::ProviderStatus {
                    id: "in_progress".into(),
                    name: "In Progress".into(),
                    category: "in_progress".into(),
                    color: Some("31B97B".into()),
                },
                grove_core::tracker::ProviderStatus {
                    id: "in_review".into(),
                    name: "In Review".into(),
                    category: "in_progress".into(),
                    color: None,
                },
                grove_core::tracker::ProviderStatus {
                    id: "blocked".into(),
                    name: "Blocked".into(),
                    category: "todo".into(),
                    color: Some("EF4444".into()),
                },
                grove_core::tracker::ProviderStatus {
                    id: "done".into(),
                    name: "Done".into(),
                    category: "done".into(),
                    color: Some("31B97B".into()),
                },
                grove_core::tracker::ProviderStatus {
                    id: "cancelled".into(),
                    name: "Cancelled".into(),
                    category: "cancelled".into(),
                    color: None,
                },
            ]),
            "github" => {
                let tracker = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                tracker
                    .list_statuses(project_key.as_deref())
                    .map_err(|e| e.to_string())
            }
            "jira" => {
                let tracker = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
                tracker
                    .list_statuses(project_key.as_deref())
                    .map_err(|e| e.to_string())
            }
            "linear" => {
                let tracker = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                tracker
                    .list_states(project_key.as_deref())
                    .map_err(|e| e.to_string())
            }
            other => Err(format!("unknown provider: {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List all open issues/tasks from a specific provider.
///
/// For "grove", returns locally cached issues from the DB.
/// For "github"/"jira"/"linear", calls the live provider API.
#[tauri::command]
pub async fn list_provider_issues(
    state: State<'_, AppState>,
    provider: String,
    project_id: Option<String>,
) -> Result<Vec<Issue>, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;

        match provider.as_str() {
            "grove" => {
                let pid = match project_id {
                    Some(id) => id,
                    None => return Ok(vec![]),
                };
                let conn = pool.get().map_err(|e| e.to_string())?;
                grove_core::tracker::list_cached(&conn, &pid).map_err(|e| e.to_string())
            }
            "github" => {
                let tracker = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                // If the project has a configured repo (owner/repo), pass it explicitly
                // so `gh` doesn't try to infer the repo from the workspace directory.
                let repo_key: Option<String> = project_id.as_deref().and_then(|pid| {
                    let conn = pool.get().ok()?;
                    let settings =
                        grove_core::db::repositories::projects_repo::get_settings(&conn, pid)
                            .ok()?;
                    settings.project_key_for("github").map(|s| s.to_string())
                });
                tracker
                    .list_for_repo(repo_key.as_deref())
                    .map_err(|e| e.to_string())
            }
            "jira" => {
                let tracker = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
                let project_key: Option<String> = project_id.as_deref().and_then(|pid| {
                    let conn = pool.get().ok()?;
                    let settings =
                        grove_core::db::repositories::projects_repo::get_settings(&conn, pid)
                            .ok()?;
                    settings.project_key_for("jira").map(|s| s.to_string())
                });
                tracker
                    .list_for_project(project_key.as_deref())
                    .map_err(|e| e.to_string())
            }
            "linear" => {
                let tracker = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                let team_key: Option<String> = project_id.as_deref().and_then(|pid| {
                    let conn = pool.get().ok()?;
                    let settings =
                        grove_core::db::repositories::projects_repo::get_settings(&conn, pid)
                            .ok()?;
                    settings.project_key_for("linear").map(|s| s.to_string())
                });
                tracker
                    .list_for_team(team_key.as_deref())
                    .map_err(|e| e.to_string())
            }
            other => Err(format!("unknown provider: {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn search_issues(
    state: State<'_, AppState>,
    query: String,
    provider: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<Issue>, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        let registry =
            grove_core::tracker::registry::TrackerRegistry::from_config(&cfg, &workspace_root);
        if !registry.is_active() {
            return Ok(vec![]);
        }
        let max = limit.unwrap_or(20);
        let _ = provider; // reserved for future per-provider filtering
        registry.search_all(&query, max).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn fetch_ready_issues(state: State<'_, AppState>) -> Result<Vec<Issue>, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        let registry =
            grove_core::tracker::registry::TrackerRegistry::from_config(&cfg, &workspace_root);
        if !registry.is_active() {
            return Ok(vec![]);
        }
        registry.list_all_ready().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Issue Board commands ───────────────────────────────────────────────────────

/// Return the full kanban board view for a project.
#[tauri::command]
pub async fn issue_board(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<grove_core::db::repositories::issues_repo::IssueBoard, String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_core::db::repositories::issues_repo::board_view(
            &conn,
            &project_id,
            &grove_core::db::repositories::issues_repo::IssueFilter::new(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get a single issue by its DB id.
#[tauri::command]
pub async fn issue_get(
    state: State<'_, AppState>,
    issue_id: String,
) -> Result<Option<grove_core::tracker::Issue>, String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_core::db::repositories::issues_repo::get(&conn, &issue_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Create a Grove-native issue (no provider required). Returns the created Issue.
#[tauri::command]
pub async fn issue_create_native(
    state: State<'_, AppState>,
    project_id: String,
    title: String,
    body: Option<String>,
    labels: Option<Vec<String>>,
    priority: Option<String>,
) -> Result<grove_core::tracker::Issue, String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        let lbls = labels.unwrap_or_default();
        grove_core::db::repositories::issues_repo::create_native(
            &mut conn,
            &project_id,
            &title,
            body.as_deref(),
            priority.as_deref(),
            &lbls,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Update issue metadata fields (title, body, labels, priority, assignee).
/// Writes changes to the local DB. Does NOT push to provider automatically.
#[tauri::command]
pub async fn issue_update(
    state: State<'_, AppState>,
    issue_id: String,
    title: Option<String>,
    body: Option<String>,
    labels: Option<Vec<String>>,
    priority: Option<String>,
    assignee: Option<String>,
) -> Result<(), String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        let update = grove_core::tracker::IssueUpdate {
            title,
            body,
            labels,
            priority,
            assignee,
            status: None,
        };
        grove_core::db::repositories::issues_repo::update_fields(&mut conn, &issue_id, &update)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Move an issue to a new canonical status column on the board.
/// Records a status-change event and updates `updated_at`.
#[tauri::command]
pub async fn issue_move(
    state: State<'_, AppState>,
    issue_id: String,
    status: String,
) -> Result<(), String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let canonical = grove_core::tracker::status::CanonicalStatus::from_db_str(&status)
            .ok_or_else(|| format!("unknown status '{status}'"))?;
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        grove_core::db::repositories::issues_repo::update_status(
            &mut conn,
            &issue_id,
            canonical.as_db_str(),
            canonical,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Assign an issue to a user. Stores locally; optionally pushes to provider.
#[tauri::command]
pub async fn issue_assign(
    state: State<'_, AppState>,
    issue_id: String,
    assignee: String,
    push_to_provider: bool,
) -> Result<(), String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        let update = grove_core::tracker::IssueUpdate {
            assignee: Some(assignee.clone()),
            ..Default::default()
        };
        grove_core::db::repositories::issues_repo::update_fields(&mut conn, &issue_id, &update)
            .map_err(|e| e.to_string())?;

        if push_to_provider {
            if let Ok(Some(issue)) =
                grove_core::db::repositories::issues_repo::get(&conn, &issue_id)
            {
                let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
                    .map_err(|e| e.to_string())?;
                let registry = grove_core::tracker::registry::TrackerRegistry::from_config(
                    &cfg,
                    &workspace_root,
                );
                for backend in registry.all_backends() {
                    if backend.provider_name() == issue.provider {
                        if let Err(e) = backend.assign(&issue.external_id, &assignee) {
                            tracing::warn!(
                                issue_id = %issue_id, error = %e,
                                "issue_assign: provider push failed"
                            );
                        }
                        break;
                    }
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Add a comment to an issue. Stores locally and optionally posts to provider.
/// Returns the stored comment including its assigned `id` and `created_at`.
#[tauri::command]
pub async fn issue_comment_add(
    state: State<'_, AppState>,
    issue_id: String,
    body: String,
    author: Option<String>,
    push_to_provider: bool,
) -> Result<grove_core::db::repositories::issues_repo::IssueComment, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = pool.get().map_err(|e| e.to_string())?;

        let mut posted = false;
        if push_to_provider {
            if let Ok(Some(issue)) =
                grove_core::db::repositories::issues_repo::get(&conn, &issue_id)
            {
                let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
                    .map_err(|e| e.to_string())?;
                let registry = grove_core::tracker::registry::TrackerRegistry::from_config(
                    &cfg,
                    &workspace_root,
                );
                for backend in registry.all_backends() {
                    if backend.provider_name() == issue.provider {
                        match backend.comment(&issue.external_id, &body) {
                            Ok(_) => posted = true,
                            Err(e) => tracing::warn!(
                                issue_id = %issue_id, error = %e,
                                "issue_comment_add: provider push failed"
                            ),
                        }
                        break;
                    }
                }
            }
        }

        let author_str = author.as_deref().unwrap_or("user").to_string();
        grove_core::db::repositories::issues_repo::add_comment(
            &mut conn,
            &issue_id,
            &body,
            &author_str,
            posted,
        )
        .map_err(|e| e.to_string())?;
        // Fetch the stored comment so the caller gets the real `id` and `created_at`.
        let comments = grove_core::db::repositories::issues_repo::list_comments(&conn, &issue_id)
            .map_err(|e| e.to_string())?;
        comments
            .into_iter()
            .next_back()
            .ok_or_else(|| "comment insert succeeded but row not found".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List all comments on an issue.
#[tauri::command]
pub async fn issue_list_comments(
    state: State<'_, AppState>,
    issue_id: String,
) -> Result<Vec<grove_core::db::repositories::issues_repo::IssueComment>, String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_core::db::repositories::issues_repo::list_comments(&conn, &issue_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List the audit-trail events for an issue.
#[tauri::command]
pub async fn issue_list_activity(
    state: State<'_, AppState>,
    issue_id: String,
) -> Result<Vec<grove_core::db::repositories::issues_repo::IssueEvent>, String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_core::db::repositories::issues_repo::list_events(&conn, &issue_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Link an existing run to an issue so write-back hooks fire on completion.
#[tauri::command]
pub async fn issue_link_run(
    state: State<'_, AppState>,
    issue_id: String,
    run_id: String,
) -> Result<(), String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_core::db::repositories::issues_repo::link_run(&conn, &issue_id, &run_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sync all configured providers for a project.
/// `incremental` skips issues older than the last sync cursor when `true`.
#[tauri::command]
pub async fn issue_sync_all(
    state: State<'_, AppState>,
    project_id: String,
    incremental: bool,
) -> Result<grove_core::tracker::sync::MultiSyncResult, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        Ok(grove_core::tracker::sync::sync_all(
            &mut conn,
            &cfg,
            &workspace_root,
            &project_id,
            incremental,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Sync a single named provider for a project.
#[tauri::command]
pub async fn issue_sync_provider(
    state: State<'_, AppState>,
    project_id: String,
    provider: String,
    incremental: bool,
) -> Result<grove_core::tracker::sync::SyncResult, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        let registry =
            grove_core::tracker::registry::TrackerRegistry::from_config(&cfg, &workspace_root);
        let backend = registry
            .all_backends()
            .iter()
            .find(|b| b.provider_name() == provider.as_str())
            .ok_or_else(|| format!("no active backend for provider '{provider}'"))?;
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        let debounce = cfg.tracker.sync.debounce_secs;
        Ok(grove_core::tracker::sync::sync_provider(
            &mut conn,
            backend.as_ref(),
            &project_id,
            incremental,
            debounce,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reopen a closed issue. Updates status locally and optionally on the provider.
#[tauri::command]
pub async fn issue_reopen(
    state: State<'_, AppState>,
    issue_id: String,
    push_to_provider: bool,
) -> Result<(), String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut conn = pool.get().map_err(|e| e.to_string())?;
        let cs = grove_core::tracker::status::CanonicalStatus::Open;
        grove_core::db::repositories::issues_repo::update_status(
            &mut conn,
            &issue_id,
            cs.as_db_str(),
            cs,
        )
        .map_err(|e| e.to_string())?;

        if push_to_provider {
            if let Ok(Some(issue)) =
                grove_core::db::repositories::issues_repo::get(&conn, &issue_id)
            {
                let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
                    .map_err(|e| e.to_string())?;
                let registry = grove_core::tracker::registry::TrackerRegistry::from_config(
                    &cfg,
                    &workspace_root,
                );
                for backend in registry.all_backends() {
                    if backend.provider_name() == issue.provider {
                        if let Err(e) = backend.reopen(&issue.external_id) {
                            tracing::warn!(
                                issue_id = %issue_id, error = %e,
                                "issue_reopen: provider push failed"
                            );
                        }
                        break;
                    }
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Permanently delete a Grove-native issue. Fails on provider-backed issues.
#[tauri::command]
pub async fn issue_delete(state: State<'_, AppState>, issue_id: String) -> Result<(), String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        if let Ok(Some(issue)) = grove_core::db::repositories::issues_repo::get(&conn, &issue_id) {
            if issue.provider != "grove" {
                return Err(format!(
                    "cannot delete provider-backed issue '{issue_id}' (provider={}); \
                     use close/transition instead",
                    issue.provider
                ));
            }
        }
        grove_core::db::repositories::issues_repo::delete(&conn, &issue_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Count open issues for a project (used by NavRail badge).
#[tauri::command]
pub async fn issue_count_open(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<usize, String> {
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_core::db::repositories::issues_repo::count_open(&conn, &project_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List projects / repos / teams available for a given provider.
///
/// Builds the tracker directly from config + keychain credentials so the call
/// succeeds regardless of what `tracker.mode` is set to in grove.yaml.
/// Returns an empty list when the provider is unknown or credentials are missing.
#[tauri::command]
pub async fn issue_list_provider_projects(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<grove_core::tracker::ProviderProject>, String> {
    use grove_core::tracker::TrackerBackend as _;

    let workspace_root = state.workspace_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;
        match provider.as_str() {
            "github" => {
                let tracker = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                tracker.list_projects().map_err(|e| e.to_string())
            }
            "jira" => {
                let tracker = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
                tracker.list_projects().map_err(|e| e.to_string())
            }
            "linear" => {
                let tracker = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                tracker.list_projects().map_err(|e| e.to_string())
            }
            other => Err(format!("unknown provider: {other}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Push an existing Grove-native issue to an external provider, creating a new
/// ticket there. Updates the local row to reflect the new external id/url.
///
/// The issue must be a native Grove issue (`provider = "grove"`).
#[tauri::command]
pub async fn push_issue_to_provider(
    state: State<'_, AppState>,
    issue_id: String,
    provider: String,
    project_key: String,
    project_id: Option<String>,
) -> Result<grove_core::tracker::Issue, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;

        let issue = grove_core::db::repositories::issues_repo::get(&conn, &issue_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("issue '{issue_id}' not found"))?;

        if issue.provider != "grove" {
            return Err(format!(
                "issue '{issue_id}' is already backed by provider '{}'; \
                 use sync instead of push",
                issue.provider
            ));
        }

        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;

        // Resolve effective key: explicit arg → project settings → error
        let effective_key = if !project_key.is_empty() {
            project_key.clone()
        } else if let Some(ref pid) = project_id {
            let settings = grove_core::db::repositories::projects_repo::get_settings(&conn, pid)
                .map_err(|e| e.to_string())?;
            settings
                .project_key_for(&provider)
                .map(|s| s.to_string())
                .ok_or_else(|| format!("no project key configured for '{provider}'"))?
        } else {
            return Err(format!("project_key is required for provider '{provider}'"));
        };

        let body = issue.body.as_deref().unwrap_or("");
        let remote: grove_core::tracker::Issue = match provider.as_str() {
            "github" => {
                let tracker = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                tracker
                    .create_in_project(&issue.title, body, &effective_key)
                    .map_err(|e| e.to_string())?
            }
            "jira" => {
                let tracker = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
                tracker
                    .create_in_project(&issue.title, body, &effective_key)
                    .map_err(|e| e.to_string())?
            }
            "linear" => {
                let tracker = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                tracker
                    .create_in_project(&issue.title, body, &effective_key)
                    .map_err(|e| e.to_string())?
            }
            other => return Err(format!("unknown provider '{other}'")),
        };

        // Update the local row and cascade the PK change to child tables.
        // issue_comments and issue_events both FK-reference issues(id).
        //
        // SQLite enforces FK constraints immediately per-statement (not at
        // COMMIT), so we use `defer_foreign_keys = ON` to defer all FK
        // checks until COMMIT — at which point parent and children are
        // consistent.
        let new_id = format!("{}:{}", remote.provider, remote.external_id);
        conn.execute_batch("PRAGMA defer_foreign_keys = ON")
            .map_err(|e| e.to_string())?;
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;

        let result = (|| -> Result<(), String> {
            // Update parent row first (PK change)
            conn.execute(
                "UPDATE issues SET
                     id                  = ?1,
                     external_id         = ?2,
                     provider            = ?3,
                     external_url        = ?4,
                     is_native           = 0,
                     provider_scope_key  = ?6,
                     provider_scope_name = ?7,
                     provider_scope_type = ?8,
                     updated_at          = datetime('now')
                 WHERE id = ?5",
                rusqlite::params![
                    new_id,
                    remote.external_id,
                    remote.provider,
                    remote.url,
                    issue_id,
                    remote.provider_scope_key,
                    remote.provider_scope_name,
                    remote.provider_scope_type,
                ],
            )
            .map_err(|e| e.to_string())?;

            // Update child FK references to match new parent PK
            conn.execute(
                "UPDATE issue_comments SET issue_id = ?1 WHERE issue_id = ?2",
                rusqlite::params![new_id, issue_id],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE issue_events SET issue_id = ?1 WHERE issue_id = ?2",
                rusqlite::params![new_id, issue_id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })();

        match result {
            Ok(()) => conn.execute_batch("COMMIT").map_err(|e| e.to_string())?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }

        grove_core::db::repositories::issues_repo::get(&conn, &new_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "push succeeded but updated row not found".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Create a new issue on an external provider in the specified project, then
/// store it locally so it appears on the board immediately.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn issue_create_on_provider(
    state: State<'_, AppState>,
    project_id: String,
    provider: String,
    project_key: String,
    title: String,
    body: Option<String>,
    labels: Option<Vec<String>>,
    priority: Option<String>,
) -> Result<grove_core::tracker::Issue, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    let pool = state.pool().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = grove_core::config::GroveConfig::load_or_create(&workspace_root)
            .map_err(|e| e.to_string())?;

        let conn = pool.get().map_err(|e| e.to_string())?;

        // Resolve project key: explicit arg → project settings → error
        let effective_key = if !project_key.is_empty() {
            project_key.clone()
        } else {
            let settings =
                grove_core::db::repositories::projects_repo::get_settings(&conn, &project_id)
                    .map_err(|e| e.to_string())?;
            settings
                .project_key_for(&provider)
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    format!("no project key configured for '{provider}' on this project")
                })?
        };

        let body_str = body.as_deref().unwrap_or("");

        let remote: grove_core::tracker::Issue = match provider.as_str() {
            "github" => {
                let tracker = grove_core::tracker::github::GitHubTracker::new(
                    &workspace_root,
                    &cfg.tracker.github,
                );
                tracker
                    .create_in_project(title.as_str(), body_str, &effective_key)
                    .map_err(|e| e.to_string())?
            }
            "jira" => {
                let tracker = grove_core::tracker::jira::JiraTracker::new(&cfg.tracker.jira);
                tracker
                    .create_in_project(title.as_str(), body_str, &effective_key)
                    .map_err(|e| e.to_string())?
            }
            "linear" => {
                let tracker = grove_core::tracker::linear::LinearTracker::new(&cfg.tracker.linear);
                tracker
                    .create_in_project(title.as_str(), body_str, &effective_key)
                    .map_err(|e| e.to_string())?
            }
            other => return Err(format!("unknown provider '{other}'")),
        };

        let labels_json = serde_json::to_string(&labels.clone().unwrap_or_default())
            .unwrap_or_else(|_| "[]".into());
        let composite_id = format!("{}:{}", remote.provider, remote.external_id);
        let canonical = grove_core::tracker::status::normalize(&remote.provider, &remote.status)
            .as_db_str()
            .to_string();

        conn.execute(
            "INSERT INTO issues (
                 id, external_id, provider, project_id,
                 title, status, canonical_status, priority, labels_json, body,
                 external_url, provider_scope_key, provider_scope_name, provider_scope_type,
                 synced_at, created_at, updated_at, raw_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                       datetime('now'), datetime('now'), datetime('now'), '{}')
             ON CONFLICT(id) DO UPDATE SET
                 title               = excluded.title,
                 status              = excluded.status,
                 canonical_status    = excluded.canonical_status,
                 priority            = excluded.priority,
                 labels_json         = excluded.labels_json,
                 body                = excluded.body,
                 external_url        = excluded.external_url,
                 provider_scope_key  = excluded.provider_scope_key,
                 provider_scope_name = excluded.provider_scope_name,
                 provider_scope_type = excluded.provider_scope_type,
                 synced_at           = excluded.synced_at,
                 updated_at          = excluded.updated_at",
            rusqlite::params![
                composite_id,
                remote.external_id,
                remote.provider,
                project_id,
                remote.title,
                remote.status,
                canonical,
                priority,
                labels_json,
                remote.body,
                remote.url,
                remote.provider_scope_key,
                remote.provider_scope_name,
                remote.provider_scope_type,
            ],
        )
        .map_err(|e| e.to_string())?;

        grove_core::db::repositories::issues_repo::get(&conn, &composite_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "issue_create_on_provider: row not found after insert".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
