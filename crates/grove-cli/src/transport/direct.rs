use std::path::{Path, PathBuf};

const DEFAULT_REPORT_RUN_LIMIT: i64 = 50;

use super::{RunResult, StartRunRequest, Transport};
use crate::error::{CliError, CliResult};
use grove_core::llm::{AuthInfo, AuthStore, LlmProviderKind, LlmRouter};

pub struct DirectTransport {
    project: PathBuf,
}

impl DirectTransport {
    #[allow(dead_code)] // called from GroveTransport::detect (Task 6)
    pub fn new(project: &Path) -> Self {
        Self {
            project: project.to_owned(),
        }
    }
}

impl Transport for DirectTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        grove_core::orchestrator::list_runs(&self.project, limit).map_err(CliError::Core)
    }

    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        grove_core::orchestrator::list_tasks(&self.project).map_err(CliError::Core)
    }

    fn get_workspace(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        match grove_core::orchestrator::get_workspace(&self.project) {
            Ok(row) => Ok(Some(row)),
            Err(grove_core::GroveError::NotFound(_)) => Ok(None),
            Err(e) => Err(CliError::Core(e)),
        }
    }

    fn list_projects(
        &self,
    ) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        grove_core::orchestrator::list_projects(&self.project).map_err(CliError::Core)
    }

    fn list_conversations(
        &self,
        limit: i64,
    ) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        grove_core::orchestrator::list_conversations(&self.project, limit).map_err(CliError::Core)
    }

    fn list_issues(&self, _cached: bool) -> CliResult<Vec<serde_json::Value>> {
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let issues = grove_core::db::repositories::issues_repo::list(
            &conn,
            &project.id,
            &grove_core::db::repositories::issues_repo::IssueFilter::new(),
        )
        .map_err(CliError::Core)?;
        issues
            .into_iter()
            .map(|i| serde_json::to_value(&i).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn get_issue(&self, id: &str) -> CliResult<serde_json::Value> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let issue = grove_core::db::repositories::issues_repo::get(&conn, id)
            .map_err(CliError::Core)?
            .ok_or_else(|| CliError::NotFound(format!("issue {id}")))?;
        serde_json::to_value(&issue).map_err(|e| CliError::Other(e.to_string()))
    }

    fn create_issue(
        &self,
        title: &str,
        body: Option<&str>,
        labels: Vec<String>,
        priority: Option<i64>,
    ) -> CliResult<serde_json::Value> {
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        let priority_str = priority.map(|p| p.to_string());
        let issue = grove_core::db::repositories::issues_repo::create_native(
            &mut conn,
            &project.id,
            title,
            body,
            priority_str.as_deref(),
            &labels,
        )
        .map_err(CliError::Core)?;
        serde_json::to_value(&issue).map_err(|e| CliError::Other(e.to_string()))
    }

    fn close_issue(&self, id: &str) -> CliResult<()> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        grove_core::db::repositories::issues_repo::update_status(
            &mut conn,
            id,
            "closed",
            grove_core::tracker::status::CanonicalStatus::Done,
        )
        .map_err(CliError::Core)
    }

    fn search_issues(
        &self,
        query: &str,
        limit: i64,
        provider: Option<&str>,
    ) -> CliResult<Vec<serde_json::Value>> {
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let mut filter = grove_core::db::repositories::issues_repo::IssueFilter::new();
        filter.limit = if limit > 0 { limit as usize } else { 100 };
        if let Some(p) = provider {
            filter.provider = Some(p.to_string());
        }
        let issues = grove_core::db::repositories::issues_repo::list(&conn, &project.id, &filter)
            .map_err(CliError::Core)?;
        let q = query.to_ascii_lowercase();
        let filtered: Vec<_> = if q.is_empty() {
            issues
        } else {
            issues
                .into_iter()
                .filter(|i| {
                    i.title.to_ascii_lowercase().contains(&q)
                        || i.body
                            .as_deref()
                            .unwrap_or("")
                            .to_ascii_lowercase()
                            .contains(&q)
                })
                .collect()
        };
        filtered
            .into_iter()
            .map(|i| serde_json::to_value(&i).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn sync_issues(&self, provider: Option<&str>, full: bool) -> CliResult<serde_json::Value> {
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        let cfg = grove_core::config::loader::load_config(&self.project).map_err(CliError::Core)?;
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        let incremental = !full;
        let result = if let Some(p) = provider {
            // Find the specific provider backend.
            let backend = match p {
                "github" => {
                    let b: Box<dyn grove_core::tracker::TrackerBackend> =
                        Box::new(grove_core::tracker::github::GitHubTracker::new(
                            &self.project,
                            &cfg.tracker.github,
                        ));
                    b
                }
                "jira" => Box::new(grove_core::tracker::jira::JiraTracker::new(
                    &cfg.tracker.jira,
                )) as Box<dyn grove_core::tracker::TrackerBackend>,
                "linear" => Box::new(grove_core::tracker::linear::LinearTracker::new(
                    &cfg.tracker.linear,
                )) as Box<dyn grove_core::tracker::TrackerBackend>,
                other => {
                    return Err(CliError::BadArg(format!("unknown provider: {other}")));
                }
            };
            let r = grove_core::tracker::sync::sync_provider(
                &mut conn,
                backend.as_ref(),
                &project.id,
                incremental,
                0,
            );
            grove_core::tracker::sync::MultiSyncResult {
                total_new: r.new_count,
                total_updated: r.updated_count,
                total_errors: r.errors.len(),
                results: vec![r],
            }
        } else {
            grove_core::tracker::sync::sync_all(
                &mut conn,
                &cfg,
                &self.project,
                &project.id,
                incremental,
            )
        };
        serde_json::to_value(&result).map_err(|e| CliError::Other(e.to_string()))
    }

    fn queue_task(
        &self,
        objective: &str,
        priority: i64,
        model: Option<&str>,
        conversation_id: Option<&str>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<grove_core::orchestrator::TaskRecord> {
        grove_core::orchestrator::queue_task(
            &self.project,
            objective,
            None, // budget_usd
            priority,
            model,
            None, // provider
            conversation_id,
            None, // resume_provider_session_id
            pipeline,
            permission_mode,
            false, // disable_phase_gates
        )
        .map_err(CliError::Core)
    }

    fn cancel_task(&self, task_id: &str) -> CliResult<()> {
        grove_core::orchestrator::cancel_task(&self.project, task_id).map_err(CliError::Core)
    }

    fn drain_queue(&self, _project: &std::path::Path) -> CliResult<()> {
        Err(CliError::Other(
            "drain_queue not available in direct mode".into(),
        ))
    }

    fn get_logs(&self, run_id: &str, all: bool) -> CliResult<Vec<serde_json::Value>> {
        let events = if all {
            grove_core::orchestrator::run_events_all(&self.project, run_id)
        } else {
            grove_core::orchestrator::run_events(&self.project, run_id)
        }
        .map_err(CliError::Core)?;

        events
            .into_iter()
            .map(|e| serde_json::to_value(&e).map_err(|err| CliError::Other(err.to_string())))
            .collect()
    }

    fn get_report(&self, _run_id: &str) -> CliResult<serde_json::Value> {
        // cost_report returns aggregate data across all completed runs, not per-run.
        let report = grove_core::orchestrator::cost_report(&self.project, DEFAULT_REPORT_RUN_LIMIT)
            .map_err(CliError::Core)?;
        serde_json::to_value(&report).map_err(|e| CliError::Other(e.to_string()))
    }

    fn get_plan(&self, run_id: Option<&str>) -> CliResult<serde_json::Value> {
        let rid = run_id.ok_or_else(|| CliError::Other("run_id is required for plan".into()))?;
        let steps = grove_core::orchestrator::list_plan_steps(&self.project, rid)
            .map_err(CliError::Core)?;
        serde_json::to_value(&steps).map_err(|e| CliError::Other(e.to_string()))
    }

    fn get_subtasks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        let rid =
            run_id.ok_or_else(|| CliError::Other("run_id is required for subtasks".into()))?;
        let steps = grove_core::orchestrator::list_plan_steps(&self.project, rid)
            .map_err(CliError::Core)?;
        steps
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn get_sessions(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let sessions = grove_core::orchestrator::list_sessions(&self.project, run_id)
            .map_err(CliError::Core)?;
        sessions
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn abort_run(&self, run_id: &str) -> CliResult<()> {
        grove_core::orchestrator::abort_run(&self.project, run_id).map_err(CliError::Core)
    }

    fn resume_run(&self, run_id: &str) -> CliResult<()> {
        grove_core::orchestrator::resume_run(&self.project, run_id)
            .map(|_| ())
            .map_err(CliError::Core)
    }

    fn list_providers(&self) -> CliResult<Vec<serde_json::Value>> {
        let statuses = LlmRouter::providers();
        statuses
            .into_iter()
            .map(|s| {
                let key_hint = if s.authenticated {
                    AuthStore::get(s.kind.id())
                        .map(|info| match info {
                            AuthInfo::Api { key } => {
                                let prefix: String = key.chars().take(4).collect();
                                format!("{prefix}...")
                            }
                            AuthInfo::WorkspaceCredits => "workspace-credits".to_string(),
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let val = serde_json::json!({
                    "provider": s.kind.id(),
                    "name": s.name,
                    "authenticated": s.authenticated,
                    "key_hint": key_hint,
                    "model_count": s.model_count,
                    "default_model": s.default_model,
                });
                Ok(val)
            })
            .collect()
    }

    fn set_api_key(&self, provider: &str, key: &str) -> CliResult<()> {
        let kind = LlmProviderKind::from_str(provider)
            .ok_or_else(|| CliError::BadArg(format!("unknown provider: {provider}")))?;
        LlmRouter::set_api_key(kind, key).map_err(|e| CliError::Other(e.to_string()))
    }

    fn remove_api_key(&self, provider: &str) -> CliResult<()> {
        let kind = LlmProviderKind::from_str(provider)
            .ok_or_else(|| CliError::BadArg(format!("unknown provider: {provider}")))?;
        LlmRouter::remove_api_key(kind).map_err(|e| CliError::Other(e.to_string()))
    }

    fn list_models(&self, provider: &str) -> CliResult<Vec<serde_json::Value>> {
        let kind = LlmProviderKind::from_str(provider)
            .ok_or_else(|| CliError::BadArg(format!("unknown provider: {provider}")))?;
        let models = LlmRouter::models(kind);
        models
            .iter()
            .map(|m| {
                let val = serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "context_window": m.context_window,
                    "max_output_tokens": m.max_output_tokens,
                    "cost_input_per_m": m.cost_input_per_m,
                    "cost_output_per_m": m.cost_output_per_m,
                    "vision": m.capabilities.vision,
                    "tools": m.capabilities.tools,
                    "reasoning": m.capabilities.reasoning,
                });
                Ok(val)
            })
            .collect()
    }

    fn select_llm(&self, provider: &str, model: Option<&str>) -> CliResult<()> {
        LlmProviderKind::from_str(provider)
            .ok_or_else(|| CliError::BadArg(format!("unknown provider: {provider}")))?;
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        let mut settings =
            grove_core::orchestrator::get_project_settings(&self.project, &project.id)
                .map_err(CliError::Core)?;
        settings.default_llm_provider = Some(provider.to_string());
        if let Some(m) = model {
            settings.default_llm_model = Some(m.to_string());
        }
        grove_core::orchestrator::update_project_settings(&self.project, &project.id, &settings)
            .map_err(CliError::Core)
    }

    fn update_issue(
        &self,
        id: &str,
        title: Option<&str>,
        status: Option<&str>,
        label: Option<&str>,
        assignee: Option<&str>,
        priority: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        let update = grove_core::tracker::IssueUpdate {
            title: title.map(|s| s.to_string()),
            body: None,
            status: status.map(|s| s.to_string()),
            labels: label.map(|l| vec![l.to_string()]),
            assignee: assignee.map(|s| s.to_string()),
            priority: priority.map(|s| s.to_string()),
        };
        grove_core::db::repositories::issues_repo::update_fields(&mut conn, id, &update)
            .map_err(CliError::Core)?;
        let issue = grove_core::db::repositories::issues_repo::get(&conn, id)
            .map_err(CliError::Core)?
            .ok_or_else(|| CliError::NotFound(format!("issue {id}")))?;
        serde_json::to_value(&issue).map_err(|e| CliError::Other(e.to_string()))
    }

    fn comment_issue(&self, id: &str, body: &str) -> CliResult<serde_json::Value> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        let comment_id = grove_core::db::repositories::issues_repo::add_comment(
            &mut conn, id, body, "user", false,
        )
        .map_err(CliError::Core)?;
        Ok(serde_json::json!({ "id": comment_id, "issue_id": id, "body": body, "author": "user" }))
    }

    fn assign_issue(&self, id: &str, assignee: &str) -> CliResult<()> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        let update = grove_core::tracker::IssueUpdate {
            assignee: Some(assignee.to_string()),
            ..Default::default()
        };
        grove_core::db::repositories::issues_repo::update_fields(&mut conn, id, &update)
            .map_err(CliError::Core)
    }

    fn move_issue(&self, id: &str, status: &str) -> CliResult<()> {
        let canonical = grove_core::tracker::status::normalize("grove", status);
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        grove_core::db::repositories::issues_repo::update_status(&mut conn, id, status, canonical)
            .map_err(CliError::Core)
    }

    fn reopen_issue(&self, id: &str) -> CliResult<()> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        grove_core::db::repositories::issues_repo::update_status(
            &mut conn,
            id,
            "open",
            grove_core::tracker::status::CanonicalStatus::Open,
        )
        .map_err(CliError::Core)
    }

    fn activity_issue(&self, id: &str) -> CliResult<Vec<serde_json::Value>> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let events = grove_core::db::repositories::issues_repo::list_events(&conn, id)
            .map_err(CliError::Core)?;
        let comments = grove_core::db::repositories::issues_repo::list_comments(&conn, id)
            .map_err(CliError::Core)?;
        let mut activity: Vec<serde_json::Value> = events
            .into_iter()
            .map(|e| {
                let mut v = serde_json::to_value(&e).unwrap_or(serde_json::Value::Null);
                if let serde_json::Value::Object(ref mut m) = v {
                    m.insert("kind".to_string(), serde_json::json!("event"));
                }
                v
            })
            .collect();
        let mut comment_values: Vec<serde_json::Value> = comments
            .into_iter()
            .map(|c| {
                let mut v = serde_json::to_value(&c).unwrap_or(serde_json::Value::Null);
                if let serde_json::Value::Object(ref mut m) = v {
                    m.insert("kind".to_string(), serde_json::json!("comment"));
                }
                v
            })
            .collect();
        activity.append(&mut comment_values);
        activity.sort_by(|a, b| {
            let ta = a.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            let tb = b.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
            ta.cmp(tb)
        });
        Ok(activity)
    }

    fn push_issue(&self, id: &str, _provider: &str) -> CliResult<serde_json::Value> {
        // Return the current issue state; actual provider push requires an active backend.
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let issue = grove_core::db::repositories::issues_repo::get(&conn, id)
            .map_err(CliError::Core)?
            .ok_or_else(|| CliError::NotFound(format!("issue {id}")))?;
        serde_json::to_value(&issue).map_err(|e| CliError::Other(e.to_string()))
    }

    fn issue_ready(&self, id: &str) -> CliResult<serde_json::Value> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        let update = grove_core::tracker::IssueUpdate {
            status: Some("ready".to_string()),
            ..Default::default()
        };
        grove_core::db::repositories::issues_repo::update_fields(&mut conn, id, &update)
            .map_err(CliError::Core)?;
        let issue = grove_core::db::repositories::issues_repo::get(&conn, id)
            .map_err(CliError::Core)?
            .ok_or_else(|| CliError::NotFound(format!("issue {id}")))?;
        serde_json::to_value(&issue).map_err(|e| CliError::Other(e.to_string()))
    }

    fn connect_status(&self) -> CliResult<Vec<serde_json::Value>> {
        let statuses: Vec<grove_core::tracker::credentials::ConnectionStatus> =
            ["github", "jira", "linear"]
                .iter()
                .map(|p| {
                    let connected =
                        grove_core::tracker::credentials::CredentialStore::has(p, "token");
                    if connected {
                        grove_core::tracker::credentials::ConnectionStatus::ok(p, "configured")
                    } else {
                        grove_core::tracker::credentials::ConnectionStatus::disconnected(p)
                    }
                })
                .collect();
        statuses
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn connect_provider(
        &self,
        provider: &str,
        token: Option<&str>,
        site: Option<&str>,
        email: Option<&str>,
    ) -> CliResult<()> {
        if let Some(t) = token {
            grove_core::tracker::credentials::CredentialStore::store(provider, "token", t)
                .map_err(CliError::Core)?;
        }
        if let Some(s) = site {
            grove_core::tracker::credentials::CredentialStore::store(provider, "site_url", s)
                .map_err(CliError::Core)?;
        }
        if let Some(e) = email {
            grove_core::tracker::credentials::CredentialStore::store(provider, "email", e)
                .map_err(CliError::Core)?;
        }
        Ok(())
    }

    fn disconnect_provider(&self, provider: &str) -> CliResult<()> {
        grove_core::tracker::credentials::CredentialStore::delete_provider(provider)
            .map_err(CliError::Core)
    }

    fn run_lint(&self, fix: bool, _model: Option<&str>) -> CliResult<serde_json::Value> {
        let cfg = grove_core::config::loader::load_config(&self.project).map_err(CliError::Core)?;
        if cfg.linter.commands.is_empty() {
            return Ok(serde_json::json!({"issues": [], "count": 0, "fix_mode": fix}));
        }
        let mut all_issues: Vec<serde_json::Value> = Vec::new();
        for cmd_config in &cfg.linter.commands {
            let result = grove_core::tracker::linter::run_linter(cmd_config, &self.project)
                .map_err(CliError::Core)?;
            for issue in result.issues {
                if let Ok(v) = serde_json::to_value(&issue) {
                    all_issues.push(v);
                }
            }
        }
        let count = all_issues.len();
        Ok(serde_json::json!({"issues": all_issues, "count": count, "fix_mode": fix}))
    }

    fn run_ci(
        &self,
        branch: Option<&str>,
        wait: bool,
        timeout: Option<u64>,
        _fix: bool,
        _model: Option<&str>,
    ) -> CliResult<serde_json::Value> {
        let branch_name = match branch {
            Some(b) => b.to_string(),
            None => grove_core::git::branch_info(&self.project)
                .map(|b| b.branch)
                .unwrap_or_else(|_| "HEAD".to_string()),
        };
        let status = if wait {
            grove_core::tracker::ci::wait_for_ci(
                &self.project,
                &branch_name,
                timeout.unwrap_or(300),
            )
            .map_err(CliError::Core)?
        } else {
            grove_core::tracker::ci::get_ci_status(&self.project, &branch_name)
                .map_err(CliError::Core)?
        };
        serde_json::to_value(&status).map_err(|e| CliError::Other(e.to_string()))
    }

    fn set_workspace_name(&self, name: &str) -> CliResult<()> {
        grove_core::orchestrator::update_workspace_name(&self.project, name).map_err(CliError::Core)
    }

    fn archive_workspace(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::archive_workspace(&self.project, id).map_err(CliError::Core)
    }

    fn delete_workspace(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::delete_workspace(&self.project, id).map_err(CliError::Core)
    }

    fn get_project(
        &self,
    ) -> CliResult<Option<grove_core::db::repositories::projects_repo::ProjectRow>> {
        match grove_core::orchestrator::get_project(&self.project) {
            Ok(row) => Ok(Some(row)),
            Err(grove_core::GroveError::NotFound(_)) => Ok(None),
            Err(e) => Err(CliError::Core(e)),
        }
    }

    fn set_project_name(&self, name: &str) -> CliResult<()> {
        // Resolve the current project id then rename it.
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        grove_core::orchestrator::update_project_name(&self.project, &project.id, name)
            .map_err(CliError::Core)
    }

    fn set_project_settings(
        &self,
        provider: Option<&str>,
        parallel: Option<i64>,
        pipeline: Option<&str>,
        permission_mode: Option<&str>,
    ) -> CliResult<()> {
        let project =
            grove_core::orchestrator::get_project(&self.project).map_err(CliError::Core)?;
        let mut settings =
            grove_core::orchestrator::get_project_settings(&self.project, &project.id)
                .map_err(CliError::Core)?;
        if let Some(p) = provider {
            settings.default_provider = Some(p.to_string());
        }
        if let Some(n) = parallel {
            settings.max_parallel_agents = Some(n);
        }
        if let Some(pl) = pipeline {
            settings.default_pipeline = Some(pl.to_string());
        }
        if let Some(pm) = permission_mode {
            settings.default_permission_mode = Some(pm.to_string());
        }
        grove_core::orchestrator::update_project_settings(&self.project, &project.id, &settings)
            .map_err(CliError::Core)
    }

    fn archive_project(&self, id: Option<&str>) -> CliResult<()> {
        let project_id = match id {
            Some(i) => i.to_string(),
            None => {
                grove_core::orchestrator::get_project(&self.project)
                    .map_err(CliError::Core)?
                    .id
            }
        };
        grove_core::orchestrator::archive_project(&self.project, &project_id)
            .map_err(CliError::Core)
    }

    fn delete_project(&self, id: Option<&str>) -> CliResult<()> {
        let project_id = match id {
            Some(i) => i.to_string(),
            None => {
                grove_core::orchestrator::get_project(&self.project)
                    .map_err(CliError::Core)?
                    .id
            }
        };
        grove_core::orchestrator::delete_project(&self.project, &project_id).map_err(CliError::Core)
    }

    fn get_conversation(
        &self,
        id: &str,
    ) -> CliResult<Option<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        match grove_core::orchestrator::get_conversation(&self.project, id) {
            Ok(row) => Ok(Some(row)),
            Err(grove_core::GroveError::NotFound(_)) => Ok(None),
            Err(e) => Err(CliError::Core(e)),
        }
    }

    fn archive_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::archive_conversation(&self.project, id).map_err(CliError::Core)
    }

    fn delete_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::delete_conversation(&self.project, id).map_err(CliError::Core)
    }

    fn rebase_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::rebase_conversation(&self.project, id)
            .map(|_| ())
            .map_err(CliError::Core)
    }

    fn merge_conversation(&self, id: &str) -> CliResult<()> {
        grove_core::orchestrator::merge_conversation(&self.project, id)
            .map(|_| ())
            .map_err(CliError::Core)
    }

    // ── Task 15 signal methods (direct DB access via grove-core) ──────────────

    fn send_signal(
        &self,
        run_id: &str,
        from: &str,
        to: &str,
        signal_type: &str,
        payload: Option<&str>,
        priority: Option<i64>,
    ) -> CliResult<()> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let sig_type = grove_core::signals::SignalType::parse(signal_type)
            .ok_or_else(|| CliError::BadArg(format!("unknown signal type: {signal_type}")))?;
        let sig_priority = priority
            .map(|p| match p {
                i64::MIN..=-1 => grove_core::signals::SignalPriority::Low,
                0 => grove_core::signals::SignalPriority::Normal,
                1 => grove_core::signals::SignalPriority::High,
                _ => grove_core::signals::SignalPriority::Urgent,
            })
            .unwrap_or_default();
        let payload_val: serde_json::Value = payload
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);
        grove_core::signals::send_signal(
            &conn,
            run_id,
            from,
            to,
            sig_type,
            sig_priority,
            payload_val,
        )
        .map(|_| ())
        .map_err(CliError::Core)
    }

    fn check_signals(&self, run_id: &str, agent: &str) -> CliResult<Vec<serde_json::Value>> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let signals =
            grove_core::signals::check_signals(&conn, run_id, agent).map_err(CliError::Core)?;
        signals
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn list_signals(&self, run_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let conn = db.connect().map_err(CliError::Core)?;
        let signals = grove_core::signals::list_for_run(&conn, run_id).map_err(CliError::Core)?;
        signals
            .into_iter()
            .map(|s| serde_json::to_value(&s).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    // ── Task 15 hook methods ──────────────────────────────────────────────────

    fn run_hook(
        &self,
        event: &str,
        agent_type: Option<&str>,
        run_id: Option<&str>,
        session_id: Option<&str>,
        _tool: Option<&str>,
        _file_path: Option<&str>,
    ) -> CliResult<()> {
        let cfg = grove_core::config::loader::load_config(&self.project).map_err(CliError::Core)?;
        let hook_event = match event {
            "session_start" => grove_core::config::HookEvent::SessionStart,
            "user_prompt_submit" => grove_core::config::HookEvent::UserPromptSubmit,
            "pre_tool_use" => grove_core::config::HookEvent::PreToolUse,
            "post_tool_use" => grove_core::config::HookEvent::PostToolUse,
            "stop" => grove_core::config::HookEvent::Stop,
            "pre_compact" => grove_core::config::HookEvent::PreCompact,
            "post_run" => grove_core::config::HookEvent::PostRun,
            "pre_merge" => grove_core::config::HookEvent::PreMerge,
            other => {
                return Err(CliError::BadArg(format!("unknown hook event: {other}")));
            }
        };
        let ctx = grove_core::hooks::HookContext {
            run_id: run_id.unwrap_or("").to_string(),
            session_id: session_id.map(|s| s.to_string()),
            agent_type: agent_type.map(|s| s.to_string()),
            worktree_path: None,
            event: hook_event,
        };
        grove_core::hooks::run_hooks(&cfg.hooks, hook_event, &ctx, &self.project)
            .map_err(CliError::Core)
    }

    // ── Task 15 worktree methods ──────────────────────────────────────────────

    fn list_worktrees(&self) -> CliResult<Vec<serde_json::Value>> {
        let entries =
            grove_core::worktree::list_worktrees(&self.project, true).map_err(CliError::Core)?;
        entries
            .into_iter()
            .map(|e| {
                Ok(serde_json::json!({
                    "session_id": e.session_id,
                    "path": e.path.to_string_lossy(),
                    "size_bytes": e.size_bytes,
                    "size": e.size_display(),
                    "run_id": e.run_id,
                    "agent_type": e.agent_type,
                    "state": e.state,
                    "created_at": e.created_at,
                    "ended_at": e.ended_at,
                    "conversation_id": e.conversation_id,
                    "project_id": e.project_id,
                    "active": e.is_active(),
                }))
            })
            .collect()
    }

    fn clean_worktrees(&self) -> CliResult<serde_json::Value> {
        let (count, bytes) = grove_core::worktree::delete_finished_worktrees(&self.project)
            .map_err(CliError::Core)?;
        Ok(serde_json::json!({"deleted": count, "bytes_freed": bytes}))
    }

    fn delete_worktree(&self, id: &str) -> CliResult<()> {
        grove_core::worktree::delete_worktree(&self.project, id)
            .map(|_| ())
            .map_err(CliError::Core)
    }

    fn delete_all_worktrees(&self) -> CliResult<serde_json::Value> {
        let (count, bytes) =
            grove_core::worktree::delete_all_worktrees(&self.project).map_err(CliError::Core)?;
        Ok(serde_json::json!({"deleted": count, "bytes_freed": bytes}))
    }

    // ── Task 15 cleanup/gc methods ────────────────────────────────────────────

    fn run_cleanup(
        &self,
        _project: bool,
        _conversation: bool,
        _dry_run: bool,
        _yes: bool,
        _force: bool,
    ) -> CliResult<serde_json::Value> {
        let (deleted, bytes_freed) = grove_core::worktree::delete_finished_worktrees(&self.project)
            .map_err(CliError::Core)?;
        Ok(serde_json::json!({
            "deleted_worktrees": deleted,
            "bytes_freed": bytes_freed,
        }))
    }

    fn run_gc(&self, _dry_run: bool) -> CliResult<serde_json::Value> {
        let db = grove_core::db::DbHandle::new(&self.project);
        let mut conn = db.connect().map_err(CliError::Core)?;
        let report = grove_core::worktree::sweep_orphaned_resources(&self.project, &mut conn)
            .map_err(CliError::Core)?;
        Ok(serde_json::json!({
            "git_gc_ran": report.git_gc_ran,
            "orphaned_branches_deleted": report.orphaned_branches_deleted,
            "orphaned_dirs_removed": report.orphaned_dirs_removed,
            "ghost_sessions_recovered": report.ghost_sessions_recovered,
        }))
    }

    fn get_run(&self, run_id: &str) -> CliResult<Option<grove_core::orchestrator::RunRecord>> {
        let runs =
            grove_core::orchestrator::list_runs(&self.project, 1000).map_err(CliError::Core)?;
        Ok(runs.into_iter().find(|r| r.id == run_id))
    }

    fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult> {
        let task = grove_core::orchestrator::queue_task(
            &self.project,
            &req.objective,
            None, // budget_usd
            0,    // priority (default)
            req.model.as_deref(),
            None, // provider
            req.conversation_id.as_deref(),
            None, // resume_provider_session_id
            req.pipeline.as_deref(),
            req.permission_mode.as_deref(),
            false, // disable_phase_gates
        )
        .map_err(CliError::Core)?;

        let task_id = task.id;
        Ok(RunResult {
            run_id: task.run_id.unwrap_or_else(|| task_id.clone()),
            task_id,
            state: task.state,
            objective: task.objective,
        })
    }

    // ── New methods: ownership locks, merge queue, retry publish ─────────────

    fn list_ownership_locks(&self, run_id: Option<&str>) -> CliResult<Vec<serde_json::Value>> {
        let locks = grove_core::orchestrator::list_ownership_locks(&self.project, run_id)
            .map_err(CliError::Core)?;
        locks
            .into_iter()
            .map(|l| serde_json::to_value(&l).map_err(|e| CliError::Other(e.to_string())))
            .collect()
    }

    fn list_merge_queue(&self, conversation_id: &str) -> CliResult<Vec<serde_json::Value>> {
        let entries = grove_core::orchestrator::list_merge_queue(&self.project, conversation_id)
            .map_err(CliError::Core)?;
        entries
            .into_iter()
            .map(|e| serde_json::to_value(&e).map_err(|e2| CliError::Other(e2.to_string())))
            .collect()
    }

    fn retry_publish_run(&self, run_id: &str) -> CliResult<()> {
        grove_core::orchestrator::retry_publish_run(&self.project, run_id)
            .map(|_| ())
            .map_err(CliError::Core)
    }
}
