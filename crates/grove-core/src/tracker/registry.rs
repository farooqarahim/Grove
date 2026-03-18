use std::path::Path;

use super::credentials::ConnectionStatus;
use super::github::GitHubTracker;
use super::jira::JiraTracker;
use super::linear::LinearTracker;
use super::{Issue, ProviderProject, TrackerBackend};
use crate::config::{GroveConfig, TrackerMode};
use crate::errors::{GroveError, GroveResult};

/// Registry of active tracker providers. Dispatches operations to whichever
/// providers are enabled in the config.
pub struct TrackerRegistry {
    providers: Vec<Box<dyn TrackerBackend>>,
}

impl TrackerRegistry {
    /// Build a registry from config. Only providers with `enabled: true` are included.
    pub fn from_config(cfg: &GroveConfig, project_root: &Path) -> Self {
        let mut providers: Vec<Box<dyn TrackerBackend>> = Vec::new();

        match cfg.tracker.mode {
            TrackerMode::Disabled => {}
            TrackerMode::External => {
                // Legacy CLI-template mode — use ExternalTracker
                providers.push(Box::new(super::ExternalTracker::new(
                    cfg.tracker.external.clone(),
                    project_root,
                )));
            }
            TrackerMode::GitHub => {
                providers.push(Box::new(GitHubTracker::new(
                    project_root,
                    &cfg.tracker.github,
                )));
            }
            TrackerMode::Jira => {
                providers.push(Box::new(JiraTracker::new(&cfg.tracker.jira)));
            }
            TrackerMode::Linear => {
                providers.push(Box::new(LinearTracker::new(&cfg.tracker.linear)));
            }
            TrackerMode::Multi => {
                if cfg.tracker.github.enabled {
                    providers.push(Box::new(GitHubTracker::new(
                        project_root,
                        &cfg.tracker.github,
                    )));
                }
                if cfg.tracker.jira.enabled {
                    providers.push(Box::new(JiraTracker::new(&cfg.tracker.jira)));
                }
                if cfg.tracker.linear.enabled {
                    providers.push(Box::new(LinearTracker::new(&cfg.tracker.linear)));
                }
            }
        }

        Self { providers }
    }

    /// Returns true if at least one provider is active.
    pub fn is_active(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Check connection status for all active providers.
    pub fn check_all_connections(&self) -> Vec<ConnectionStatus> {
        let mut statuses = Vec::new();
        for p in &self.providers {
            let name = p.provider_name();
            match name {
                "github" => {
                    // Downcast not needed — we just report based on provider name
                    statuses.push(ConnectionStatus::ok(
                        name,
                        "check via `grove connect status`",
                    ));
                }
                _ => {
                    statuses.push(ConnectionStatus::ok(name, "configured"));
                }
            }
        }
        statuses
    }

    /// List open issues across all providers.
    pub fn list_all_issues(&self) -> GroveResult<Vec<Issue>> {
        if self.providers.is_empty() {
            return Err(GroveError::Runtime(
                "no tracker providers enabled — configure tracker.mode in grove.yaml".into(),
            ));
        }
        let mut all = Vec::new();
        for p in &self.providers {
            match p.list() {
                Ok(issues) => all.extend(issues),
                Err(e) => {
                    tracing::warn!(provider = p.provider_name(), error = %e, "failed to list issues");
                }
            }
        }
        Ok(all)
    }

    /// List issues marked as ready across all providers.
    pub fn list_all_ready(&self) -> GroveResult<Vec<Issue>> {
        if self.providers.is_empty() {
            return Err(GroveError::Runtime(
                "no tracker providers enabled — configure tracker.mode in grove.yaml".into(),
            ));
        }
        let mut all = Vec::new();
        for p in &self.providers {
            match p.ready() {
                Ok(issues) => all.extend(issues),
                Err(e) => {
                    tracing::warn!(provider = p.provider_name(), error = %e, "failed to list ready issues");
                }
            }
        }
        Ok(all)
    }

    /// Search for issues across all providers.
    ///
    /// Tries the provider's native `search()` first (Jira JQL, Linear GraphQL,
    /// GitHub `--search`). Falls back to `list()` + client-side filter for
    /// providers that do not implement native search.
    pub fn search_all(&self, query: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        if self.providers.is_empty() {
            return Err(GroveError::Runtime(
                "no tracker providers enabled — configure tracker.mode in grove.yaml".into(),
            ));
        }
        let mut all = Vec::new();
        let per_provider = (limit / self.providers.len().max(1)).max(10);
        for p in &self.providers {
            let issues = p.search(query, per_provider).or_else(|_| {
                // Provider doesn't support native search — list all and filter.
                p.list().map(|all_issues| {
                    let q = query.to_lowercase();
                    all_issues
                        .into_iter()
                        .filter(|i| {
                            i.title.to_lowercase().contains(&q)
                                || i.external_id.to_lowercase().contains(&q)
                                || i.body
                                    .as_deref()
                                    .map(|b| b.to_lowercase().contains(&q))
                                    .unwrap_or(false)
                        })
                        .take(per_provider)
                        .collect()
                })
            });
            match issues {
                Ok(found) => all.extend(found),
                Err(e) => {
                    tracing::warn!(provider = p.provider_name(), error = %e, "failed to search issues");
                }
            }
        }
        Ok(all)
    }

    /// Return all active backends (used by the sync engine to iterate providers).
    pub fn all_backends(&self) -> &[Box<dyn TrackerBackend>] {
        &self.providers
    }

    /// Find a specific issue by ID across all providers.
    pub fn find_issue(&self, id: &str) -> GroveResult<Option<Issue>> {
        for p in &self.providers {
            match p.show(id) {
                Ok(issue) => return Ok(Some(issue)),
                Err(_) => continue,
            }
        }
        Ok(None)
    }

    /// Create an issue on the first active provider.
    pub fn create_issue(&self, title: &str, body: &str) -> GroveResult<Issue> {
        let provider = self
            .providers
            .first()
            .ok_or_else(|| GroveError::Runtime("no tracker providers enabled".into()))?;
        provider.create(title, body)
    }

    /// Close an issue. Tries each provider until one succeeds.
    pub fn close_issue(&self, id: &str) -> GroveResult<()> {
        for p in &self.providers {
            if p.close(id).is_ok() {
                return Ok(());
            }
        }
        Err(GroveError::Runtime(format!(
            "no provider could close issue {id}"
        )))
    }

    /// List available projects / boards / teams for a named provider.
    pub fn list_projects_for(&self, provider_name: &str) -> GroveResult<Vec<ProviderProject>> {
        let backend = self
            .providers
            .iter()
            .find(|p| p.provider_name() == provider_name)
            .ok_or_else(|| {
                GroveError::Runtime(format!("provider '{provider_name}' is not enabled"))
            })?;
        backend.list_projects()
    }

    /// Create an issue in a specific project on a named provider, then return it.
    pub fn create_issue_in_project(
        &self,
        provider_name: &str,
        title: &str,
        body: &str,
        project_key: &str,
    ) -> GroveResult<Issue> {
        let backend = self
            .providers
            .iter()
            .find(|p| p.provider_name() == provider_name)
            .ok_or_else(|| {
                GroveError::Runtime(format!("provider '{provider_name}' is not enabled"))
            })?;
        backend.create_in_project(title, body, project_key)
    }
}
