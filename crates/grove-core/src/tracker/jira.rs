use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::Value;

use std::collections::HashMap;

use super::credentials::{ConnectionStatus, CredentialStorage, CredentialStore};
use super::{Issue, IssueUpdate, SyncCursor, TrackerBackend};
use crate::config::JiraTrackerConfig;
use crate::errors::{GroveError, GroveResult};

const PROVIDER: &str = "jira";
const KEY_TOKEN: &str = "api-token";
const KEY_EMAIL: &str = "email";
/// Persisted alongside the token so site URL survives restarts without
/// requiring the user to edit grove.yaml manually.
const KEY_SITE_URL: &str = "site-url";

/// Jira issue tracker using the REST API v3.
pub struct JiraTracker {
    config: JiraTrackerConfig,
    client: reqwest::blocking::Client,
}

impl JiraTracker {
    pub fn new(config: &JiraTrackerConfig) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            config: config.clone(),
            client,
        }
    }

    /// Store Jira credentials in the selected Grove storage backend.
    ///
    /// Verifies the credentials against the Jira API before storing, so callers
    /// get an early error rather than discovering a bad token on first use.
    /// The site URL is stored alongside the token so it survives app restarts
    /// without requiring the user to manually edit grove.yaml.
    pub fn save_credentials(
        &self,
        email: &str,
        token: &str,
        storage: CredentialStorage,
    ) -> GroveResult<()> {
        let status = self.verify_credentials(email, token)?;
        if !status.connected {
            return Err(GroveError::Runtime(
                status
                    .error
                    .unwrap_or_else(|| "Jira authentication failed".into()),
            ));
        }
        let mut bundle = HashMap::new();
        bundle.insert(KEY_TOKEN.to_string(), token.to_string());
        bundle.insert(KEY_EMAIL.to_string(), email.to_string());
        // Persist site URL so base_url() works correctly after an app restart,
        // even when grove.yaml still has the default/empty placeholder.
        let site_url = self.config.site_url.trim().trim_end_matches('/');
        if !site_url.is_empty() {
            bundle.insert(KEY_SITE_URL.to_string(), site_url.to_string());
        }
        CredentialStore::store_bundle_with_storage(PROVIDER, bundle, storage)?;
        Ok(())
    }

    /// Check whether stored credentials are valid.
    ///
    /// Reconstructs a JiraTracker with the keychain-stored site URL if the
    /// config doesn't have one, so this works correctly after an app restart.
    pub fn check_connection(&self) -> ConnectionStatus {
        let email = match CredentialStore::retrieve(PROVIDER, KEY_EMAIL) {
            Ok(Some(e)) => e,
            Ok(None) => return ConnectionStatus::disconnected(PROVIDER),
            Err(e) => {
                return ConnectionStatus::err(
                    PROVIDER,
                    &format!("Keychain read failed — try re-entering your credentials. ({e})"),
                );
            }
        };
        let token = match CredentialStore::retrieve(PROVIDER, KEY_TOKEN) {
            Ok(Some(t)) => t,
            Ok(None) => return ConnectionStatus::disconnected(PROVIDER),
            Err(e) => {
                return ConnectionStatus::err(
                    PROVIDER,
                    &format!("Keychain read failed — try re-entering your credentials. ({e})"),
                );
            }
        };
        // If the config has no site URL but keychain does, build a fresh tracker
        // so verify_credentials uses the correct URL.
        if self.config.site_url.trim().is_empty() {
            if let Ok(Some(site_url)) = CredentialStore::retrieve(PROVIDER, KEY_SITE_URL) {
                let cfg = crate::config::JiraTrackerConfig {
                    site_url,
                    email: email.clone(),
                    ..self.config.clone()
                };
                let tracker = JiraTracker::new(&cfg);
                return match tracker.verify_credentials(&email, &token) {
                    Ok(status) => status,
                    Err(e) => ConnectionStatus::err(PROVIDER, &e.to_string()),
                };
            }
            return ConnectionStatus::disconnected(PROVIDER);
        }
        match self.verify_credentials(&email, &token) {
            Ok(status) => status,
            Err(e) => ConnectionStatus::err(PROVIDER, &e.to_string()),
        }
    }

    /// Remove all stored Jira credentials.
    pub fn disconnect(&self) -> GroveResult<()> {
        CredentialStore::delete_provider(PROVIDER)
    }

    /// Search issues via JQL text match across all accessible projects.
    ///
    /// Detects direct issue key patterns (e.g. "PROJ-123") and fetches them
    /// by key first, which is more reliable than text search for known keys.
    /// Fetch all workflow statuses available for a Jira project.
    pub fn list_statuses(
        &self,
        project_key: Option<&str>,
    ) -> GroveResult<Vec<super::ProviderStatus>> {
        let key = project_key
            .filter(|k| !k.is_empty())
            .unwrap_or(&self.config.project_key);
        if key.is_empty() {
            return Ok(vec![]);
        }
        let auth = self.auth_header_from_keychain()?;
        let base = self.base_url();
        let url = format!("{base}/rest/api/3/project/{key}/statuses");
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| {
                crate::errors::GroveError::Runtime(format!("Jira statuses request failed: {e}"))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(crate::errors::GroveError::Runtime(format!(
                "Jira statuses API error {status}: {}",
                body.chars().take(200).collect::<String>()
            )));
        }

        let data: Vec<Value> = resp.json().map_err(|e| {
            crate::errors::GroveError::Runtime(format!("failed to parse Jira statuses: {e}"))
        })?;

        let mut statuses = Vec::new();
        for issue_type in &data {
            if let Some(arr) = issue_type.get("statuses").and_then(|v| v.as_array()) {
                for s in arr {
                    let id = s
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = s
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if name.is_empty()
                        || statuses
                            .iter()
                            .any(|x: &super::ProviderStatus| x.name == name)
                    {
                        continue;
                    }
                    let cat_key = s
                        .pointer("/statusCategory/key")
                        .and_then(|v| v.as_str())
                        .unwrap_or("new");
                    let category = match cat_key {
                        "done" => "done",
                        "indeterminate" => "in_progress",
                        _ => "todo",
                    };
                    statuses.push(super::ProviderStatus {
                        id,
                        name,
                        category: category.to_string(),
                        color: None,
                    });
                }
            }
        }
        Ok(statuses)
    }

    /// List open issues, optionally scoped to a specific Jira project key.
    /// Falls back to `self.config.project_key` when `project_key` is `None`.
    pub fn list_for_project(&self, project_key: Option<&str>) -> GroveResult<Vec<Issue>> {
        let key = project_key
            .filter(|k| !k.is_empty())
            .unwrap_or(&self.config.project_key);
        if key.is_empty() {
            return Ok(vec![]);
        }
        let jql = format!("project = {} AND status != Done ORDER BY updated DESC", key);
        self.jql_search(&jql, 50)
    }

    pub fn search_issues(&self, query: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(vec![]);
        }
        let sanitized = query.replace('"', "\\\"");

        // Fast path: if it looks like a Jira key (e.g. "PROJ-123"), fetch directly.
        if is_issue_key(query) {
            if let Ok(issue) = self.get_issue(&query.to_ascii_uppercase()) {
                return Ok(vec![issue]);
            }
        }

        // Cross-project text search — no `project =` constraint so users can
        // find issues across all boards they have access to.
        let jql = format!("text ~ \"{}\" ORDER BY updated DESC", sanitized);
        self.jql_search(&jql, limit)
    }

    /// Resolve a Jira Cloud `accountId` from a display name or email address.
    ///
    /// Jira Cloud (since 2019) requires `accountId` for all user operations;
    /// the legacy `name` field is not accepted and silently fails.
    fn resolve_account_id(&self, name_or_email: &str) -> GroveResult<String> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!("{}/rest/api/3/user/search", self.base_url());

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .query(&[("query", name_or_email), ("maxResults", "10")])
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira user search failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(GroveError::Runtime(format!(
                "Jira user search returned HTTP {}",
                resp.status()
            )));
        }

        let users: Vec<Value> = resp.json().unwrap_or_default();
        users
            .first()
            .and_then(|u| {
                u.get("accountId")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                GroveError::Runtime(format!(
                    "no Jira user found matching '{}' — check the name or email",
                    name_or_email
                ))
            })
    }

    fn verify_credentials(&self, email: &str, token: &str) -> GroveResult<ConnectionStatus> {
        let url = format!(
            "{}/rest/api/3/myself",
            self.config.site_url.trim_end_matches('/')
        );
        let auth = auth_header(email, token);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira request failed: {e}")))?;

        if resp.status().is_success() {
            let body: Value = resp.json().unwrap_or_default();
            let display_name = body
                .get("displayName")
                .and_then(|s| s.as_str())
                .unwrap_or(email);
            Ok(ConnectionStatus::ok(PROVIDER, display_name))
        } else {
            Ok(ConnectionStatus::err(
                PROVIDER,
                &format!("Jira API returned HTTP {}", resp.status()),
            ))
        }
    }

    fn auth_header_from_keychain(&self) -> GroveResult<String> {
        let email = CredentialStore::retrieve(PROVIDER, KEY_EMAIL)?.ok_or_else(|| {
            GroveError::Runtime("Jira email not configured — run `grove connect jira`".into())
        })?;
        let token = CredentialStore::retrieve(PROVIDER, KEY_TOKEN)?.ok_or_else(|| {
            GroveError::Runtime("Jira token not configured — run `grove connect jira`".into())
        })?;
        Ok(auth_header(&email, &token))
    }

    /// Returns the Jira base URL, preferring the config value but falling back
    /// to the keychain-stored URL (set during `save_credentials`). This ensures
    /// the URL survives app restarts even when grove.yaml still has a placeholder.
    fn base_url(&self) -> String {
        let from_config = self.config.site_url.trim().trim_end_matches('/');
        if !from_config.is_empty() {
            return from_config.to_string();
        }
        CredentialStore::retrieve(PROVIDER, KEY_SITE_URL)
            .ok()
            .flatten()
            .unwrap_or_default()
            .trim()
            .trim_end_matches('/')
            .to_string()
    }

    fn jql_search(&self, jql: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!("{}/rest/api/3/search", self.base_url());
        let body = serde_json::json!({
            "jql": jql,
            "maxResults": limit,
            "fields": ["summary", "status", "labels", "description", "assignee", "issuetype", "project"]
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira search failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Jira search returned HTTP {status}: {}",
                text.chars().take(500).collect::<String>()
            )));
        }

        let data: Value = resp.json().unwrap_or_default();
        let issues = data
            .get("issues")
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| issue_from_json(v, &self.base_url()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(issues)
    }

    fn get_issue(&self, key: &str) -> GroveResult<Issue> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!(
            "{}/rest/api/3/issue/{}?fields=summary,status,labels,description,assignee,issuetype,project",
            self.base_url(),
            key
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira get issue failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(GroveError::Runtime(format!(
                "Jira issue {key} not found (HTTP {})",
                resp.status()
            )));
        }

        let v: Value = resp.json().unwrap_or_default();
        Ok(issue_from_json(&v, &self.base_url()))
    }

    fn create_issue_api(&self, title: &str, body: &str) -> GroveResult<Issue> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!("{}/rest/api/3/issue", self.base_url());

        let payload = serde_json::json!({
            "fields": {
                "project": { "key": self.config.project_key },
                "summary": title,
                "description": {
                    "type": "doc",
                    "version": 1,
                    "content": [{
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": body
                        }]
                    }]
                },
                "issuetype": { "name": "Task" }
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira create issue failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Jira create failed: {}",
                text.chars().take(500).collect::<String>()
            )));
        }

        let v: Value = resp.json().unwrap_or_default();
        let key = v.get("key").and_then(|k| k.as_str()).unwrap_or_default();
        self.get_issue(key)
    }

    fn transition_issue(&self, key: &str, target_name: &str) -> GroveResult<()> {
        let auth = self.auth_header_from_keychain()?;

        // First, get available transitions
        let url = format!("{}/rest/api/3/issue/{key}/transitions", self.base_url());
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira transitions failed: {e}")))?;

        let data: Value = resp.json().unwrap_or_default();
        let transitions = data
            .get("transitions")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();

        let target_lower = target_name.to_lowercase();
        let transition_id = transitions
            .iter()
            .find(|t| {
                t.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.to_lowercase().contains(&target_lower))
                    .unwrap_or(false)
            })
            .and_then(|t| t.get("id"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| {
                GroveError::Runtime(format!(
                    "no transition matching '{target_name}' found for {key}"
                ))
            })?;

        // Apply the transition
        let payload = serde_json::json!({ "transition": { "id": transition_id } });
        self.client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira transition failed: {e}")))?;

        Ok(())
    }
}

impl TrackerBackend for JiraTracker {
    fn provider_name(&self) -> &str {
        PROVIDER
    }

    fn create(&self, title: &str, body: &str) -> GroveResult<Issue> {
        self.create_issue_api(title, body)
    }

    fn show(&self, id: &str) -> GroveResult<Issue> {
        self.get_issue(id)
    }

    fn list(&self) -> GroveResult<Vec<Issue>> {
        self.list_for_project(None)
    }

    fn close(&self, id: &str) -> GroveResult<()> {
        self.transition_issue(id, "done")
    }

    fn ready(&self) -> GroveResult<Vec<Issue>> {
        let jql = format!(
            "project = {} AND {}",
            self.config.project_key, self.config.jql_ready
        );
        self.jql_search(&jql, 50)
    }

    fn search(&self, query: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        self.search_issues(query, limit)
    }

    fn list_paginated(&self, cursor: &SyncCursor) -> GroveResult<Vec<Issue>> {
        let auth = self.auth_header_from_keychain()?;
        let jql = if let Some(since) = cursor.since {
            format!(
                "project = {} AND updated >= \"{}\" ORDER BY updated DESC",
                self.config.project_key,
                since.format("%Y/%m/%d %H:%M")
            )
        } else {
            format!(
                "project = {} ORDER BY updated DESC",
                self.config.project_key
            )
        };

        let url = format!("{}/rest/api/3/search", self.base_url());
        let body = serde_json::json!({
            "jql": jql,
            "startAt": cursor.offset,
            "maxResults": cursor.limit,
            "fields": ["summary", "status", "labels", "description", "assignee", "issuetype", "project"]
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira paginated list failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Jira list_paginated returned HTTP {status}: {}",
                text.chars().take(500).collect::<String>()
            )));
        }

        let data: Value = resp.json().unwrap_or_default();
        let issues = data
            .get("issues")
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| issue_from_json(v, &self.base_url()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(issues)
    }

    fn comment(&self, id: &str, body: &str) -> GroveResult<String> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!("{}/rest/api/3/issue/{id}/comment", self.base_url());

        // Jira requires Atlassian Document Format (ADF) for comment bodies.
        let adf_body = serde_json::json!({
            "body": {
                "type": "doc",
                "version": 1,
                "content": [{
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": body }]
                }]
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&adf_body)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira comment failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Jira comment failed: {}",
                text.chars().take(500).collect::<String>()
            )));
        }

        let v: Value = resp.json().unwrap_or_default();
        let comment_id = v
            .get("id")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(format!(
            "{}/browse/{id}?focusedCommentId={comment_id}",
            self.base_url()
        ))
    }

    fn update(&self, id: &str, update: &IssueUpdate) -> GroveResult<Issue> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!("{}/rest/api/3/issue/{id}", self.base_url());

        let mut fields = serde_json::Map::new();
        if let Some(title) = &update.title {
            fields.insert("summary".into(), serde_json::Value::String(title.clone()));
        }
        if let Some(body_text) = &update.body {
            fields.insert("description".into(), serde_json::json!({
                "type": "doc",
                "version": 1,
                "content": [{"type": "paragraph", "content": [{"type": "text", "text": body_text}]}]
            }));
        }
        if let Some(lbls) = &update.labels {
            fields.insert(
                "labels".into(),
                serde_json::Value::Array(
                    lbls.iter()
                        .map(|l| serde_json::Value::String(l.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(assignee_name) = &update.assignee {
            // Jira Cloud: resolve accountId — the legacy `name` field is not accepted.
            match self.resolve_account_id(assignee_name) {
                Ok(account_id) => {
                    fields.insert(
                        "assignee".into(),
                        serde_json::json!({ "accountId": account_id }),
                    );
                }
                Err(e) => {
                    return Err(GroveError::Runtime(format!("cannot update assignee: {e}")));
                }
            }
        }

        let payload = serde_json::json!({ "fields": fields });
        let resp = self
            .client
            .put(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira update failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Jira update failed: {}",
                text.chars().take(500).collect::<String>()
            )));
        }

        // Return fresh issue state
        self.get_issue(id)
    }

    fn transition(&self, id: &str, target_status: &str) -> GroveResult<()> {
        self.transition_issue(id, target_status)
    }

    fn assign(&self, id: &str, assignee: &str) -> GroveResult<()> {
        // Jira Cloud requires `accountId` — the legacy `name` field was deprecated
        // in 2019 and silently fails on all modern Jira Cloud instances.
        let auth = self.auth_header_from_keychain()?;
        let account_id = self.resolve_account_id(assignee)?;
        let url = format!("{}/rest/api/3/issue/{id}/assignee", self.base_url());
        let payload = serde_json::json!({ "accountId": account_id });
        let resp = self
            .client
            .put(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira assign failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Jira assign failed: {}",
                text.chars().take(500).collect::<String>()
            )));
        }
        Ok(())
    }

    fn reopen(&self, id: &str) -> GroveResult<()> {
        self.transition_issue(id, "to do")
    }

    fn list_projects(&self) -> GroveResult<Vec<super::ProviderProject>> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!("{}/rest/api/3/project", self.base_url());
        let resp = self
            .client
            .get(&url)
            .header("Authorization", &auth)
            .header("Accept", "application/json")
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira list projects failed: {e}")))?;

        if !resp.status().is_success() {
            return Err(GroveError::Runtime(format!(
                "Jira list projects returned HTTP {}",
                resp.status()
            )));
        }

        let projects: Vec<Value> = resp.json().unwrap_or_default();
        Ok(projects
            .iter()
            .map(|v| super::ProviderProject {
                id: v
                    .get("id")
                    .and_then(|s| s.as_str())
                    .unwrap_or_default()
                    .to_string(),
                name: v
                    .get("name")
                    .and_then(|s| s.as_str())
                    .unwrap_or_default()
                    .to_string(),
                key: v.get("key").and_then(|s| s.as_str()).map(|s| s.to_string()),
                url: v
                    .get("self")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string()),
            })
            .collect())
    }

    fn create_in_project(
        &self,
        title: &str,
        body: &str,
        project_key: &str,
    ) -> GroveResult<super::Issue> {
        let auth = self.auth_header_from_keychain()?;
        let url = format!("{}/rest/api/3/issue", self.base_url());

        let payload = serde_json::json!({
            "fields": {
                "project": { "key": project_key },
                "summary": title,
                "description": {
                    "type": "doc", "version": 1,
                    "content": [{"type": "paragraph", "content": [{"type": "text", "text": body}]}]
                },
                "issuetype": { "name": "Task" }
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&payload)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Jira create issue failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Jira create in project '{}' failed: {}",
                project_key,
                text.chars().take(500).collect::<String>()
            )));
        }

        let v: Value = resp.json().unwrap_or_default();
        let key = v.get("key").and_then(|k| k.as_str()).unwrap_or_default();
        self.get_issue(key)
    }
}

fn auth_header(email: &str, token: &str) -> String {
    let encoded = BASE64.encode(format!("{email}:{token}"));
    format!("Basic {encoded}")
}

fn issue_from_json(v: &Value, base_url: &str) -> Issue {
    let provider_native_id = v
        .get("id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let key = v
        .get("key")
        .and_then(|k| k.as_str())
        .unwrap_or_default()
        .to_string();

    let fields = v.get("fields").cloned().unwrap_or(Value::Null);

    let title = fields
        .get("summary")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    let status = fields
        .get("status")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or("unknown")
        .to_string();

    let labels: Vec<String> = fields
        .get("labels")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Jira uses ADF (Atlassian Document Format) for description.
    // Extract plain text from the top-level content nodes.
    let body = fields.get("description").and_then(extract_adf_text);

    let url = if key.is_empty() {
        None
    } else {
        Some(format!("{base_url}/browse/{key}"))
    };

    let assignee = fields
        .get("assignee")
        .and_then(|a| a.get("displayName"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string());
    let project_key = fields
        .get("project")
        .and_then(|project| project.get("key"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let project_name = fields
        .get("project")
        .and_then(|project| project.get("name"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let issue_type = fields
        .get("issuetype")
        .and_then(|issue_type| issue_type.get("name"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let assignee_account_id = fields
        .get("assignee")
        .and_then(|assignee| assignee.get("accountId"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let status_category = fields
        .get("status")
        .and_then(|status| status.get("statusCategory"))
        .and_then(|category| category.get("name"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    Issue {
        external_id: key,
        provider: PROVIDER.to_string(),
        title,
        status: status.clone(),
        labels: labels.clone(),
        body,
        url,
        assignee,
        raw_json: v.clone(),
        provider_native_id,
        provider_scope_type: Some("project".to_string()),
        provider_scope_key: project_key.clone(),
        provider_scope_name: project_name.clone(),
        provider_metadata: serde_json::json!({
            "project_key": project_key,
            "project_name": project_name,
            "issue_type": issue_type,
            "assignee_account_id": assignee_account_id,
            "status_category": status_category,
            "status_name": status,
            "label_names": labels,
        }),
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

/// Extract plain text from Jira's Atlassian Document Format (ADF).
///
/// Handles all nesting levels including bullet lists, ordered lists, headings,
/// code blocks, and blockquotes. Block-level containers are separated by newlines.
fn extract_adf_text(v: &Value) -> Option<String> {
    let text = flatten_adf(v);
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn flatten_adf(node: &Value) -> String {
    if node.is_null() {
        return String::new();
    }
    // Plain string — some older Jira instances return description as a raw string.
    if let Some(s) = node.as_str() {
        return s.to_string();
    }

    let node_type = node.get("type").and_then(|t| t.as_str()).unwrap_or("");

    // Leaf text node — may have marks (bold, italic) which we ignore for plain text.
    if node_type == "text" {
        return node
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();
    }

    // Hard break → newline
    if node_type == "hardBreak" {
        return "\n".to_string();
    }

    let children = match node.get("content").and_then(|c| c.as_array()) {
        Some(arr) => arr,
        None => return String::new(),
    };

    let parts: Vec<String> = children.iter().map(flatten_adf).collect();

    // Block-level containers get newline separation; inline nodes are joined flat.
    match node_type {
        "doc" | "bulletList" | "orderedList" | "blockquote" => parts.join("\n"),
        "codeBlock" => {
            let inner = parts.join("");
            format!("```\n{inner}\n```")
        }
        "paragraph" | "heading" | "listItem" => parts.join(""),
        _ => parts.join(""),
    }
}

/// Returns true if `s` looks like a Jira issue key, e.g. "PROJ-123" or "MY_TEAM-45".
fn is_issue_key(s: &str) -> bool {
    let mut parts = s.splitn(2, '-');
    let project = parts.next().unwrap_or("");
    let number = parts.next().unwrap_or("");
    !project.is_empty()
        && project.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jira_issue() {
        let json = serde_json::json!({
            "key": "PROJ-123",
            "fields": {
                "summary": "Fix auth timeout",
                "status": { "name": "In Progress" },
                "labels": ["bug", "high-priority"],
                "description": {
                    "type": "doc",
                    "version": 1,
                    "content": [{
                        "type": "paragraph",
                        "content": [{
                            "type": "text",
                            "text": "Login times out after 30s"
                        }]
                    }]
                },
                "assignee": { "displayName": "John Doe" }
            }
        });

        let issue = issue_from_json(&json, "https://myco.atlassian.net");
        assert_eq!(issue.external_id, "PROJ-123");
        assert_eq!(issue.provider, "jira");
        assert_eq!(issue.title, "Fix auth timeout");
        assert_eq!(issue.status, "In Progress");
        assert_eq!(issue.labels, vec!["bug", "high-priority"]);
        assert_eq!(issue.body.as_deref(), Some("Login times out after 30s"));
        assert_eq!(
            issue.url.as_deref(),
            Some("https://myco.atlassian.net/browse/PROJ-123")
        );
        assert_eq!(issue.assignee.as_deref(), Some("John Doe"));
    }

    #[test]
    fn test_auth_header() {
        let header = auth_header("user@example.com", "token123");
        assert!(header.starts_with("Basic "));
        let decoded = BASE64
            .decode(header.strip_prefix("Basic ").unwrap())
            .unwrap();
        assert_eq!(
            String::from_utf8(decoded).unwrap(),
            "user@example.com:token123"
        );
    }

    #[test]
    fn test_extract_adf_text_plain() {
        let v = serde_json::json!("plain string body");
        assert_eq!(extract_adf_text(&v), Some("plain string body".to_string()));
    }

    #[test]
    fn test_extract_adf_text_single_paragraph() {
        // Two text nodes in the same paragraph are joined flat (no newline between them).
        let v = serde_json::json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": "Hello " },
                    { "type": "text", "text": "world" }
                ]
            }]
        });
        assert_eq!(extract_adf_text(&v), Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_adf_text_two_paragraphs() {
        // Two paragraphs in a doc are separated by a newline.
        let v = serde_json::json!({
            "type": "doc",
            "content": [
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "First paragraph" }]
                },
                {
                    "type": "paragraph",
                    "content": [{ "type": "text", "text": "Second paragraph" }]
                }
            ]
        });
        assert_eq!(
            extract_adf_text(&v),
            Some("First paragraph\nSecond paragraph".to_string())
        );
    }

    #[test]
    fn test_extract_adf_text_bullet_list() {
        let v = serde_json::json!({
            "type": "doc",
            "content": [{
                "type": "bulletList",
                "content": [
                    { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Item one" }] }] },
                    { "type": "listItem", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Item two" }] }] }
                ]
            }]
        });
        let text = extract_adf_text(&v).unwrap();
        assert!(text.contains("Item one"), "must contain first item");
        assert!(text.contains("Item two"), "must contain second item");
    }

    #[test]
    fn test_extract_adf_text_hard_break() {
        let v = serde_json::json!({
            "type": "doc",
            "content": [{
                "type": "paragraph",
                "content": [
                    { "type": "text", "text": "Line A" },
                    { "type": "hardBreak" },
                    { "type": "text", "text": "Line B" }
                ]
            }]
        });
        assert_eq!(extract_adf_text(&v), Some("Line A\nLine B".to_string()));
    }

    #[test]
    fn test_is_issue_key() {
        assert!(is_issue_key("PROJ-123"));
        assert!(is_issue_key("MY_TEAM-1"));
        assert!(!is_issue_key("not-a-key"));
        assert!(!is_issue_key("PROJ-"));
        assert!(!is_issue_key("-123"));
        assert!(!is_issue_key("plain text"));
    }
}
