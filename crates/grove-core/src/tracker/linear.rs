use serde_json::Value;

use super::credentials::{ConnectionStatus, CredentialStorage, CredentialStore};
use super::{Issue, IssueUpdate, SyncCursor, TrackerBackend};
use crate::config::LinearTrackerConfig;
use crate::errors::{GroveError, GroveResult};

const PROVIDER: &str = "linear";
const KEY_TOKEN: &str = "api-token";
const GRAPHQL_URL: &str = "https://api.linear.app/graphql";

/// Linear issue tracker using the GraphQL API.
pub struct LinearTracker {
    config: LinearTrackerConfig,
    client: reqwest::blocking::Client,
}

impl LinearTracker {
    pub fn new(config: &LinearTrackerConfig) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();
        Self {
            config: config.clone(),
            client,
        }
    }

    /// Store the Linear API token in the selected Grove storage backend.
    pub fn save_token(&self, token: &str, storage: CredentialStorage) -> GroveResult<()> {
        let status = self.verify_token(token)?;
        if !status.connected {
            return Err(GroveError::Runtime(
                status
                    .error
                    .unwrap_or_else(|| "Linear authentication failed".into()),
            ));
        }
        CredentialStore::store_with_storage(PROVIDER, KEY_TOKEN, token, storage)?;
        Ok(())
    }

    /// Check whether stored token is valid.
    pub fn check_connection(&self) -> ConnectionStatus {
        let token = match CredentialStore::retrieve(PROVIDER, KEY_TOKEN) {
            Ok(Some(t)) => t,
            Ok(None) => return ConnectionStatus::disconnected(PROVIDER),
            Err(e) => {
                return ConnectionStatus::err(
                    PROVIDER,
                    &format!("Keychain read failed — try re-entering your token. ({e})"),
                );
            }
        };
        match self.verify_token(&token) {
            Ok(status) => status,
            Err(e) => ConnectionStatus::err(PROVIDER, &e.to_string()),
        }
    }

    /// Remove stored Linear credentials.
    pub fn disconnect(&self) -> GroveResult<()> {
        CredentialStore::delete_provider(PROVIDER)
    }

    /// Search issues by text.
    pub fn search_issues(&self, query: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        let token = self.get_token()?;
        let gql = r#"
            query SearchIssues($term: String!, $limit: Int!) {
                searchIssues(term: $term, first: $limit) {
                    nodes {
                        id identifier title description url
                        state { name }
                        team { key }
                        project { name }
                        assignee { name }
                        labels { nodes { name } }
                    }
                }
            }
        "#;
        let vars = serde_json::json!({ "term": query, "limit": limit });
        let data = self.graphql(&token, gql, vars)?;
        let nodes = data
            .pointer("/searchIssues/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(nodes.iter().map(issue_from_json).collect())
    }

    fn verify_token(&self, token: &str) -> GroveResult<ConnectionStatus> {
        let gql = r#"query { viewer { id name email } }"#;
        let data = self.graphql(token, gql, serde_json::json!({}))?;
        let name = data
            .pointer("/viewer/name")
            .and_then(|n| n.as_str())
            .or_else(|| data.pointer("/viewer/email").and_then(|e| e.as_str()));
        match name {
            Some(n) => Ok(ConnectionStatus::ok(PROVIDER, n)),
            None => Ok(ConnectionStatus::err(
                PROVIDER,
                "failed to fetch Linear viewer",
            )),
        }
    }

    fn get_token(&self) -> GroveResult<String> {
        CredentialStore::retrieve(PROVIDER, KEY_TOKEN)?.ok_or_else(|| {
            GroveError::Runtime("Linear token not configured — run `grove connect linear`".into())
        })
    }

    fn graphql(&self, token: &str, query: &str, variables: Value) -> GroveResult<Value> {
        let body = serde_json::json!({
            "query": query,
            "variables": variables,
        });

        let resp = self
            .client
            .post(GRAPHQL_URL)
            // Linear Personal API keys are sent as the raw token value —
            // no "Bearer" prefix. Using "Bearer" causes HTTP 400.
            .header("Authorization", token)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| GroveError::Runtime(format!("Linear API request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(GroveError::Runtime(format!(
                "Linear API returned HTTP {status}: {}",
                text.chars().take(500).collect::<String>()
            )));
        }

        let result: Value = resp.json().unwrap_or_default();

        // Check for GraphQL errors
        if let Some(errors) = result.get("errors").and_then(|e| e.as_array()) {
            if let Some(first) = errors.first() {
                let msg = first
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown GraphQL error");
                return Err(GroveError::Runtime(format!("Linear GraphQL error: {msg}")));
            }
        }

        Ok(result.get("data").cloned().unwrap_or(Value::Null))
    }

    fn list_issues_gql(&self, limit: usize) -> GroveResult<Vec<Issue>> {
        self.list_for_team_gql(None, limit)
    }

    /// Fetch all workflow states for a Linear team.
    pub fn list_states(&self, team_key: Option<&str>) -> GroveResult<Vec<super::ProviderStatus>> {
        let key = team_key
            .filter(|k| !k.is_empty())
            .unwrap_or(&self.config.team_key);
        if key.is_empty() {
            return Ok(vec![]);
        }
        let token = self.get_token()?;
        let gql = r#"
            query TeamStates($teamKey: String!) {
                teams(filter: { key: { eq: $teamKey } }) {
                    nodes {
                        states {
                            nodes { id name color type }
                        }
                    }
                }
            }
        "#;
        let vars = serde_json::json!({ "teamKey": key });
        let data = self.graphql(&token, gql, vars)?;
        let nodes = data
            .pointer("/teams/nodes/0/states/nodes")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        Ok(nodes
            .iter()
            .map(|s| {
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
                let color = s
                    .get("color")
                    .and_then(|v| v.as_str())
                    .map(|c| c.trim_start_matches('#').to_string());
                let stype = s
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unstarted");
                let category = match stype {
                    "completed" => "done",
                    "cancelled" => "cancelled",
                    "started" => "in_progress",
                    "backlog" => "backlog",
                    _ => "todo",
                };
                super::ProviderStatus {
                    id,
                    name,
                    category: category.to_string(),
                    color,
                }
            })
            .collect())
    }

    /// List issues for a specific team key, or the configured default when `team_key` is `None`.
    pub fn list_for_team(&self, team_key: Option<&str>) -> GroveResult<Vec<Issue>> {
        self.list_for_team_gql(team_key, 50)
    }

    fn list_for_team_gql(&self, team_key: Option<&str>, limit: usize) -> GroveResult<Vec<Issue>> {
        let key = team_key
            .filter(|k| !k.is_empty())
            .unwrap_or(&self.config.team_key);
        if key.is_empty() {
            return Ok(vec![]);
        }
        let token = self.get_token()?;
        let gql = r#"
            query ListIssues($limit: Int!, $teamKey: String!) {
                issues(
                    first: $limit,
                    orderBy: updatedAt,
                    filter: {
                        team: { key: { eq: $teamKey } }
                        state: { type: { nin: ["completed", "cancelled"] } }
                    }
                ) {
                    nodes {
                        id identifier title description url
                        state { name }
                        team { key }
                        project { name }
                        assignee { name }
                        labels { nodes { name } }
                    }
                }
            }
        "#;
        let vars = serde_json::json!({ "limit": limit, "teamKey": key });
        let data = self.graphql(&token, gql, vars)?;
        let nodes = data
            .pointer("/issues/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(nodes.iter().map(issue_from_json).collect())
    }

    /// Resolve a workflow state name to its Linear state ID for the configured team.
    ///
    /// Fetches available states from the Linear API and returns the ID of the
    /// first state whose name contains `target_name` (case-insensitive).
    /// This avoids hardcoding state IDs which are team-specific UUIDs.
    fn resolve_state_id(&self, token: &str, target_name: &str) -> GroveResult<String> {
        let gql = r#"
            query TeamStates($teamKey: String!) {
                teams(filter: { key: { eq: $teamKey } }) {
                    nodes {
                        states { nodes { id name type } }
                    }
                }
            }
        "#;
        let vars = serde_json::json!({ "teamKey": self.config.team_key });
        let data = self.graphql(token, gql, vars)?;
        let states = data
            .pointer("/teams/nodes/0/states/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();

        let target_lower = target_name.to_ascii_lowercase();
        states
            .iter()
            .find(|s| {
                s.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.to_ascii_lowercase().contains(&target_lower))
                    .unwrap_or(false)
            })
            .and_then(|s| {
                s.get("id")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                GroveError::Runtime(format!(
                    "no Linear workflow state matching '{}' for team '{}'",
                    target_name, self.config.team_key
                ))
            })
    }

    /// Resolve a Linear user display name or email to their user ID.
    fn resolve_user_id(&self, token: &str, name_or_email: &str) -> GroveResult<String> {
        let gql = r#"
            query FindUser($query: String!) {
                users(filter: {
                    or: [
                        { name: { containsIgnoreCase: $query } },
                        { email: { eq: $query } }
                    ]
                }) {
                    nodes { id name email }
                }
            }
        "#;
        let vars = serde_json::json!({ "query": name_or_email });
        let data = self.graphql(token, gql, vars)?;
        let users = data
            .pointer("/users/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();

        users
            .first()
            .and_then(|u| {
                u.get("id")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string())
            })
            .ok_or_else(|| {
                GroveError::Runtime(format!("no Linear user found matching '{name_or_email}'"))
            })
    }

    fn list_ready_gql(&self, limit: usize) -> GroveResult<Vec<Issue>> {
        let token = self.get_token()?;
        let gql = r#"
            query ReadyIssues($limit: Int!, $teamKey: String!, $label: String!) {
                issues(
                    first: $limit,
                    orderBy: updatedAt,
                    filter: {
                        team: { key: { eq: $teamKey } }
                        labels: { name: { eq: $label } }
                        state: { type: { nin: ["completed", "cancelled"] } }
                    }
                ) {
                    nodes {
                        id identifier title description url
                        state { name }
                        team { key }
                        project { name }
                        assignee { name }
                        labels { nodes { name } }
                    }
                }
            }
        "#;
        let vars = serde_json::json!({
            "limit": limit,
            "teamKey": self.config.team_key,
            "label": self.config.label_ready,
        });
        let data = self.graphql(&token, gql, vars)?;
        let nodes = data
            .pointer("/issues/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(nodes.iter().map(issue_from_json).collect())
    }
}

impl TrackerBackend for LinearTracker {
    fn provider_name(&self) -> &str {
        PROVIDER
    }

    fn create(&self, title: &str, body: &str) -> GroveResult<Issue> {
        // Delegate to create_in_project so team resolution is always correct.
        // create_in_project resolves the team key → UUID via GraphQL before mutating,
        // which avoids the bug of passing a string key where Linear expects a UUID.
        let team_key = self.config.team_key.clone();
        self.create_in_project(title, body, &team_key)
    }

    fn show(&self, id: &str) -> GroveResult<Issue> {
        let token = self.get_token()?;
        let gql = r#"
            query GetIssue($id: String!) {
                issue(id: $id) {
                    id identifier title description url
                    state { name }
                    team { key }
                    project { name }
                    assignee { name }
                    labels { nodes { name } }
                }
            }
        "#;
        let vars = serde_json::json!({ "id": id });
        let data = self.graphql(&token, gql, vars)?;
        let issue_json = data.pointer("/issue").cloned().unwrap_or(Value::Null);
        if issue_json.is_null() {
            return Err(GroveError::NotFound(format!("Linear issue {id}")));
        }
        Ok(issue_from_json(&issue_json))
    }

    fn list(&self) -> GroveResult<Vec<Issue>> {
        self.list_issues_gql(50)
    }

    fn close(&self, id: &str) -> GroveResult<()> {
        // Linear state IDs are team-specific UUIDs — never hardcode them.
        // Resolve the correct "done" or "completed" state UUID first.
        let token = self.get_token()?;
        let state_id = self
            .resolve_state_id(&token, "done")
            .or_else(|_| self.resolve_state_id(&token, "completed"))
            .or_else(|_| self.resolve_state_id(&token, "finished"))?;

        let gql = r#"
            mutation CloseIssue($id: String!, $stateId: String!) {
                issueUpdate(id: $id, input: { stateId: $stateId }) {
                    issue { id }
                }
            }
        "#;
        let vars = serde_json::json!({ "id": id, "stateId": state_id });
        self.graphql(&token, gql, vars)?;
        Ok(())
    }

    fn ready(&self) -> GroveResult<Vec<Issue>> {
        self.list_ready_gql(50)
    }

    fn search(&self, query: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        self.search_issues(query, limit)
    }

    fn list_paginated(&self, cursor: &SyncCursor) -> GroveResult<Vec<Issue>> {
        let token = self.get_token()?;
        let gql = r#"
            query ListIssuesPaginated($limit: Int!, $teamKey: String!, $after: String) {
                issues(
                    first: $limit,
                    after: $after,
                    orderBy: updatedAt,
                    filter: { team: { key: { eq: $teamKey } } }
                ) {
                    nodes {
                        id identifier title description url
                        state { name }
                        team { key }
                        project { name }
                        assignee { name }
                        labels { nodes { name } }
                    }
                    pageInfo { hasNextPage endCursor }
                }
            }
        "#;
        let vars = serde_json::json!({
            "limit": cursor.limit,
            "teamKey": self.config.team_key,
            "after": cursor.after_cursor,
        });
        let data = self.graphql(&token, gql, vars)?;
        let nodes = data
            .pointer("/issues/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(nodes.iter().map(issue_from_json).collect())
    }

    fn comment(&self, id: &str, body: &str) -> GroveResult<String> {
        let token = self.get_token()?;
        let gql = r#"
            mutation CreateComment($issueId: String!, $body: String!) {
                commentCreate(input: { issueId: $issueId, body: $body }) {
                    comment { id url }
                    success
                }
            }
        "#;
        let vars = serde_json::json!({ "issueId": id, "body": body });
        let data = self.graphql(&token, gql, vars)?;
        let url = data
            .pointer("/commentCreate/comment/url")
            .and_then(|s| s.as_str())
            .or_else(|| {
                data.pointer("/commentCreate/comment/id")
                    .and_then(|s| s.as_str())
            })
            .unwrap_or_default()
            .to_string();
        Ok(url)
    }

    fn update(&self, id: &str, update: &IssueUpdate) -> GroveResult<Issue> {
        let token = self.get_token()?;
        let mut input = serde_json::Map::new();
        if let Some(t) = &update.title {
            input.insert("title".into(), serde_json::Value::String(t.clone()));
        }
        if let Some(b) = &update.body {
            input.insert("description".into(), serde_json::Value::String(b.clone()));
        }
        if let Some(priority_str) = &update.priority {
            // Linear priority: 0=No priority, 1=Urgent, 2=High, 3=Medium, 4=Low
            let p: u64 = match priority_str.as_str() {
                "urgent" | "critical" => 1,
                "high" => 2,
                "medium" => 3,
                "low" => 4,
                _ => 0,
            };
            input.insert("priority".into(), serde_json::Value::Number(p.into()));
        }

        let gql = r#"
            mutation UpdateIssue($id: String!, $input: IssueUpdateInput!) {
                issueUpdate(id: $id, input: $input) {
                    issue {
                        id identifier title description url
                        state { name }
                        team { key }
                        project { name }
                        assignee { name }
                        labels { nodes { name } }
                    }
                }
            }
        "#;
        let vars = serde_json::json!({ "id": id, "input": input });
        let data = self.graphql(&token, gql, vars)?;
        let issue_json = data
            .pointer("/issueUpdate/issue")
            .cloned()
            .unwrap_or(Value::Null);
        Ok(issue_from_json(&issue_json))
    }

    fn transition(&self, id: &str, target_status: &str) -> GroveResult<()> {
        let token = self.get_token()?;
        let state_id = self.resolve_state_id(&token, target_status)?;

        let gql = r#"
            mutation TransitionIssue($id: String!, $stateId: String!) {
                issueUpdate(id: $id, input: { stateId: $stateId }) {
                    issue { id }
                }
            }
        "#;
        let vars = serde_json::json!({ "id": id, "stateId": state_id });
        self.graphql(&token, gql, vars)?;
        Ok(())
    }

    fn assign(&self, id: &str, assignee: &str) -> GroveResult<()> {
        let token = self.get_token()?;
        let assignee_id = self.resolve_user_id(&token, assignee)?;

        let gql = r#"
            mutation AssignIssue($id: String!, $assigneeId: String!) {
                issueUpdate(id: $id, input: { assigneeId: $assigneeId }) {
                    issue { id }
                }
            }
        "#;
        let vars = serde_json::json!({ "id": id, "assigneeId": assignee_id });
        self.graphql(&token, gql, vars)?;
        Ok(())
    }

    fn reopen(&self, id: &str) -> GroveResult<()> {
        let token = self.get_token()?;
        // Find the first "started" or "unstarted" state for the team.
        let state_id = self
            .resolve_state_id(&token, "todo")
            .or_else(|_| self.resolve_state_id(&token, "in progress"))?;

        let gql = r#"
            mutation ReopenIssue($id: String!, $stateId: String!) {
                issueUpdate(id: $id, input: { stateId: $stateId }) {
                    issue { id }
                }
            }
        "#;
        let vars = serde_json::json!({ "id": id, "stateId": state_id });
        self.graphql(&token, gql, vars)?;
        Ok(())
    }

    fn list_projects(&self) -> GroveResult<Vec<super::ProviderProject>> {
        let token = self.get_token()?;
        let gql = r#"
            query ListTeams {
                teams { nodes { id key name } }
            }
        "#;
        let data = self.graphql(&token, gql, serde_json::json!({}))?;
        let teams = data
            .pointer("/teams/nodes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(teams
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
                url: None,
            })
            .collect())
    }

    fn create_in_project(
        &self,
        title: &str,
        body: &str,
        project_key: &str,
    ) -> GroveResult<super::Issue> {
        let token = self.get_token()?;
        // Resolve team ID from the provided team key.
        let gql_team = r#"
            query FindTeam($key: String!) {
                teams(filter: { key: { eq: $key } }) {
                    nodes { id key name }
                }
            }
        "#;
        let vars = serde_json::json!({ "key": project_key });
        let data = self.graphql(&token, gql_team, vars)?;
        let team_id = data
            .pointer("/teams/nodes/0/id")
            .and_then(|s| s.as_str())
            .ok_or_else(|| {
                GroveError::Runtime(format!("Linear team with key '{}' not found", project_key))
            })?
            .to_string();

        let gql = r#"
            mutation CreateIssueInTeam($title: String!, $description: String!, $teamId: String!) {
                issueCreate(input: {
                    title: $title,
                    description: $description,
                    teamId: $teamId
                }) {
                    issue {
                        id identifier title description url
                        state { name }
                        team { key }
                        assignee { name }
                        labels { nodes { name } }
                    }
                }
            }
        "#;
        let vars = serde_json::json!({
            "title": title,
            "description": body,
            "teamId": team_id,
        });
        let data = self.graphql(&token, gql, vars)?;
        let issue_json = data
            .pointer("/issueCreate/issue")
            .cloned()
            .unwrap_or(Value::Null);
        Ok(issue_from_json(&issue_json))
    }
}

fn issue_from_json(v: &Value) -> Issue {
    let id = v
        .get("id")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();

    let identifier = v
        .get("identifier")
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();

    let external_id = if identifier.is_empty() {
        id.clone()
    } else {
        identifier
    };

    let title = v
        .get("title")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    let status = v
        .pointer("/state/name")
        .and_then(|s| s.as_str())
        .unwrap_or("unknown")
        .to_string();

    let labels: Vec<String> = v
        .pointer("/labels/nodes")
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| {
                    l.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let body = v
        .get("description")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    let url = v.get("url").and_then(|s| s.as_str()).map(|s| s.to_string());

    let assignee = v
        .pointer("/assignee/name")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let team_key = v
        .pointer("/team/key")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let project_name = v
        .pointer("/project/name")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());
    let workflow_state_type = v
        .pointer("/state/type")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string());

    Issue {
        external_id,
        provider: PROVIDER.to_string(),
        title,
        status: status.clone(),
        labels: labels.clone(),
        body,
        url,
        assignee: assignee.clone(),
        raw_json: v.clone(),
        provider_native_id: Some(id),
        provider_scope_type: Some("team".to_string()),
        provider_scope_key: team_key.clone(),
        provider_scope_name: project_name.clone().or_else(|| team_key.clone()),
        provider_metadata: serde_json::json!({
            "team_key": team_key,
            "project_name": project_name,
            "workflow_state_name": status,
            "workflow_state_type": workflow_state_type,
            "assignee_name": assignee,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_linear_issue() {
        let json = serde_json::json!({
            "id": "abc-123",
            "identifier": "ENG-42",
            "title": "Fix auth flow",
            "description": "OAuth tokens expire too quickly",
            "url": "https://linear.app/myteam/issue/ENG-42",
            "state": { "name": "In Progress" },
            "team": { "key": "ENG" },
            "project": { "name": "Auth" },
            "assignee": { "name": "Jane Doe" },
            "labels": { "nodes": [{ "name": "bug" }, { "name": "urgent" }] }
        });

        let issue = issue_from_json(&json);
        assert_eq!(issue.external_id, "ENG-42");
        assert_eq!(issue.provider, "linear");
        assert_eq!(issue.title, "Fix auth flow");
        assert_eq!(issue.status, "In Progress");
        assert_eq!(issue.labels, vec!["bug", "urgent"]);
        assert_eq!(
            issue.body.as_deref(),
            Some("OAuth tokens expire too quickly")
        );
        assert_eq!(
            issue.url.as_deref(),
            Some("https://linear.app/myteam/issue/ENG-42")
        );
        assert_eq!(issue.assignee.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn test_parse_linear_issue_minimal() {
        let json = serde_json::json!({
            "id": "xyz",
            "title": "Minimal issue"
        });

        let issue = issue_from_json(&json);
        assert_eq!(issue.external_id, "xyz");
        assert_eq!(issue.title, "Minimal issue");
        assert_eq!(issue.status, "unknown");
        assert!(issue.labels.is_empty());
        assert!(issue.body.is_none());
    }
}
