use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use super::credentials::{ConnectionStatus, CredentialStore};
use super::{Issue, IssueUpdate, SyncCursor, TrackerBackend};
use crate::config::GitHubTrackerConfig;
use crate::errors::{GroveError, GroveResult};

/// Returns an expanded PATH that includes common CLI install locations on macOS.
/// Returns the user's login-shell PATH, cached after the first call.
///
/// macOS GUI apps launch with a minimal system PATH. Spawning a login shell
/// (`zsh -l -c 'echo $PATH'`) sources the user's shell config and returns
/// the same PATH they see in a terminal — wherever they installed their CLIs.
fn shell_path() -> &'static str {
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        // Try the user's preferred shell first, then common shells.
        let candidates = [
            std::env::var("SHELL").unwrap_or_default(),
            "/bin/zsh".to_string(),
            "/bin/bash".to_string(),
        ];
        for shell in &candidates {
            if shell.is_empty() {
                continue;
            }
            let mut child = match std::process::Command::new(shell)
                .args(["-l", "-c", "echo $PATH"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(_) => continue,
            };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if std::time::Instant::now() >= deadline => {
                        let _ = child.kill();
                        break;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                    Err(_) => break,
                }
            }
            if let Ok(out) = child.wait_with_output() {
                if out.status.success() {
                    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !path.is_empty() {
                        return path;
                    }
                }
            }
        }
        // Fallback: system PATH as-is.
        std::env::var("PATH").unwrap_or_default()
    })
}

const PROVIDER: &str = "github";
const KEY_TOKEN: &str = "oauth-token";

/// GitHub issue tracker backed by the `gh` CLI.
pub struct GitHubTracker {
    project_root: PathBuf,
    config: GitHubTrackerConfig,
}

impl GitHubTracker {
    pub fn new(project_root: &Path, config: &GitHubTrackerConfig) -> Self {
        Self {
            project_root: project_root.to_owned(),
            config: config.clone(),
        }
    }

    /// Check whether `gh` CLI is installed and authenticated.
    pub fn check_connection(&self) -> ConnectionStatus {
        let output = Command::new("gh")
            .args(["auth", "status", "--hostname", "github.com"])
            .env("PATH", shell_path())
            .current_dir(&self.project_root)
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let text = String::from_utf8_lossy(&o.stdout);
                let user = text
                    .lines()
                    .find(|l| l.contains("Logged in to"))
                    .and_then(|l| l.split("account ").nth(1))
                    .map(|s| s.trim().trim_end_matches(|c: char| !c.is_alphanumeric()))
                    .unwrap_or("authenticated");
                ConnectionStatus::ok(PROVIDER, user)
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                ConnectionStatus::err(
                    PROVIDER,
                    &format!(
                        "gh not authenticated: {}",
                        stderr.chars().take(200).collect::<String>()
                    ),
                )
            }
            Err(_) => ConnectionStatus::err(
                PROVIDER,
                "gh CLI not found — install from https://cli.github.com",
            ),
        }
    }

    /// Authenticate `gh` CLI with a token (stored in keychain + passed to gh).
    pub fn authenticate(&self, token: &str) -> GroveResult<()> {
        CredentialStore::store(PROVIDER, KEY_TOKEN, token)?;

        let mut child = Command::new("gh")
            .args(["auth", "login", "--with-token"])
            .env("PATH", shell_path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| GroveError::Runtime(format!("failed to start gh: {e}")))?;

        if let Some(ref mut stdin) = child.stdin {
            use std::io::Write;
            stdin
                .write_all(token.as_bytes())
                .map_err(|e| GroveError::Runtime(format!("failed to write token to gh: {e}")))?;
        }

        let output = child
            .wait_with_output()
            .map_err(|e| GroveError::Runtime(format!("gh auth failed: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GroveError::Runtime(format!(
                "gh auth login failed: {stderr}"
            )));
        }
        Ok(())
    }

    /// Remove stored credentials and log out of gh.
    pub fn disconnect(&self) -> GroveResult<()> {
        CredentialStore::delete(PROVIDER, KEY_TOKEN)?;
        let _ = Command::new("gh")
            .args(["auth", "logout", "--hostname", "github.com"])
            .env("PATH", shell_path())
            .stdin(std::process::Stdio::null())
            .output();
        Ok(())
    }

    /// Fetch a single issue by number, optionally scoped to `owner/repo`.
    pub fn show_for_repo(&self, id: &str, repo: Option<&str>) -> GroveResult<Issue> {
        let mut args = vec![
            "issue",
            "view",
            id,
            "--json",
            "id,number,title,state,labels,body,assignees,url",
        ];
        let repo_owned;
        if let Some(r) = repo {
            repo_owned = r.to_string();
            args.extend_from_slice(&["--repo", &repo_owned]);
        }
        let output = self.run_gh(&args)?;
        let repo_locator = repo
            .map(|r| r.to_string())
            .or_else(|| self.current_repo_locator());
        parse_issue(&output).map(|issue| self.enrich_issue(issue, repo_locator.as_deref()))
    }

    /// Search issues by query string.
    pub fn search_issues(&self, query: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        let output = self.run_gh(&[
            "issue",
            "list",
            "--search",
            query,
            "--limit",
            &limit.to_string(),
            "--json",
            "id,number,title,state,labels,body,assignees,url",
        ])?;
        self.enrich_issues(parse_issue_list(&output))
    }

    /// List available labels for the repo (these are used as issue statuses in GitHub),
    /// plus synthetic "open"/"closed" pseudo-statuses for the native GitHub state.
    pub fn list_statuses(&self, repo: Option<&str>) -> GroveResult<Vec<super::ProviderStatus>> {
        let mut args = vec!["label", "list", "--json", "name,color,description"];
        let repo_owned;
        if let Some(r) = repo {
            repo_owned = r.to_string();
            args.extend_from_slice(&["--repo", &repo_owned]);
        }

        let mut statuses = vec![
            super::ProviderStatus {
                id: "open".to_string(),
                name: "Open".to_string(),
                category: "todo".to_string(),
                color: None,
            },
            super::ProviderStatus {
                id: "closed".to_string(),
                name: "Closed (resolved)".to_string(),
                category: "done".to_string(),
                color: None,
            },
        ];

        if let Ok(output) = self.run_gh(&args) {
            if let Ok(labels) = serde_json::from_str::<Vec<Value>>(&output) {
                for label in labels {
                    let name = label
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let color = label
                        .get("color")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let name_lower = name.to_ascii_lowercase();
                    let category = if name_lower.contains("progress")
                        || name_lower.contains("doing")
                        || name_lower.contains("wip")
                        || name_lower.contains("active")
                    {
                        "in_progress"
                    } else if name_lower.contains("done")
                        || name_lower.contains("complete")
                        || name_lower.contains("resolved")
                        || name_lower.contains("merged")
                    {
                        "done"
                    } else if name_lower.contains("cancel")
                        || name_lower.contains("wont")
                        || name_lower.contains("invalid")
                    {
                        "cancelled"
                    } else if name_lower.contains("backlog") {
                        "backlog"
                    } else {
                        "todo"
                    };
                    statuses.push(super::ProviderStatus {
                        id: name.clone(),
                        name,
                        category: category.to_string(),
                        color,
                    });
                }
            }
        }

        Ok(statuses)
    }

    fn run_gh(&self, args: &[&str]) -> GroveResult<String> {
        let output = Command::new("gh")
            .args(args)
            .env("PATH", shell_path())
            .current_dir(&self.project_root)
            .output()
            .map_err(|e| GroveError::Runtime(format!("gh command failed to start: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GroveError::Runtime(format!(
                "gh command failed (exit {}): {}",
                output.status,
                stderr.chars().take(500).collect::<String>()
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn ready_labels(&self) -> String {
        self.config.labels_ready.join(",")
    }

    /// List open issues, optionally scoped to a specific `owner/repo` string.
    /// When `repo` is `None`, `gh` infers the repo from the current directory's git remote.
    pub fn list_for_repo(&self, repo: Option<&str>) -> GroveResult<Vec<Issue>> {
        let mut args = vec![
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            "50",
            "--json",
            "id,number,title,state,labels,body,assignees,url",
        ];
        // Owned string so the slice reference can outlive the block.
        let repo_owned;
        if let Some(r) = repo {
            repo_owned = r.to_string();
            args.extend_from_slice(&["--repo", &repo_owned]);
        }
        let output = self.run_gh(&args)?;
        let repo_locator = repo
            .map(|r| r.to_string())
            .or_else(|| self.current_repo_locator());
        let issues = parse_issue_list(&output)?;
        Ok(issues
            .into_iter()
            .map(|issue| self.enrich_issue(issue, repo_locator.as_deref()))
            .collect())
    }

    fn current_repo_locator(&self) -> Option<String> {
        let output = self
            .run_gh(&["repo", "view", "--json", "nameWithOwner"])
            .ok()?;
        let value: Value = serde_json::from_str(output.trim()).ok()?;
        value
            .get("nameWithOwner")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
    }

    fn enrich_issue(&self, mut issue: Issue, repo_locator: Option<&str>) -> Issue {
        if let Some(repo_locator) = repo_locator {
            issue.provider_scope_type = Some("repository".to_string());
            issue.provider_scope_key = Some(repo_locator.to_string());
            issue.provider_scope_name = Some(repo_locator.to_string());
            if let Some(metadata) = issue.provider_metadata.as_object_mut() {
                metadata.insert(
                    "repository".to_string(),
                    Value::String(repo_locator.to_string()),
                );
            }
        }
        issue
    }

    fn enrich_issues(&self, issues: GroveResult<Vec<Issue>>) -> GroveResult<Vec<Issue>> {
        let repo_locator = self.current_repo_locator();
        issues.map(|issues| {
            issues
                .into_iter()
                .map(|issue| self.enrich_issue(issue, repo_locator.as_deref()))
                .collect()
        })
    }

    /// Transition an issue with an explicit `--repo` flag and correct GitHub semantics:
    /// - "closed" / "close" / "done" / "cancelled" → `gh issue close`
    /// - "open" / "reopen" → `gh issue reopen`
    /// - anything else → `gh issue edit --add-label <status>` (GitHub has no native workflow states)
    pub fn apply_transition_for_repo(
        &self,
        id: &str,
        target_status: &str,
        repo: Option<&str>,
    ) -> GroveResult<()> {
        let status_lower = target_status.to_ascii_lowercase();
        let repo_owned;
        let repo_args: &[&str] = if let Some(r) = repo {
            repo_owned = r.to_string();
            &["--repo", &repo_owned]
        } else {
            &[]
        };

        if status_lower == "close"
            || status_lower == "closed"
            || status_lower == "done"
            || status_lower == "cancelled"
        {
            let mut args = vec!["issue", "close", id];
            args.extend_from_slice(repo_args);
            self.run_gh(&args)?;
        } else if status_lower == "open" || status_lower == "reopen" {
            let mut args = vec!["issue", "reopen", id];
            args.extend_from_slice(repo_args);
            self.run_gh(&args)?;
        } else {
            // Treat the target_status as a label name and add it to the issue.
            let mut args = vec!["issue", "edit", id, "--add-label", target_status];
            args.extend_from_slice(repo_args);
            self.run_gh(&args)?;
        }
        Ok(())
    }
}

impl TrackerBackend for GitHubTracker {
    fn provider_name(&self) -> &str {
        PROVIDER
    }

    fn create(&self, title: &str, body: &str) -> GroveResult<Issue> {
        let output = self.run_gh(&[
            "issue",
            "create",
            "--title",
            title,
            "--body",
            body,
            "--json",
            "id,number,title,state,labels,body,url",
        ])?;
        let repo_locator = self.current_repo_locator();
        parse_issue(&output).map(|issue| self.enrich_issue(issue, repo_locator.as_deref()))
    }

    fn show(&self, id: &str) -> GroveResult<Issue> {
        self.show_for_repo(id, None)
    }

    fn list(&self) -> GroveResult<Vec<Issue>> {
        self.list_for_repo(None)
    }

    fn close(&self, id: &str) -> GroveResult<()> {
        self.run_gh(&["issue", "close", id])?;
        Ok(())
    }

    fn ready(&self) -> GroveResult<Vec<Issue>> {
        let labels = self.ready_labels();
        let output = self.run_gh(&[
            "issue",
            "list",
            "--label",
            &labels,
            "--state",
            "open",
            "--limit",
            "50",
            "--json",
            "id,number,title,state,labels,body,assignees,url",
        ])?;
        self.enrich_issues(parse_issue_list(&output))
    }

    fn search(&self, query: &str, limit: usize) -> GroveResult<Vec<Issue>> {
        self.search_issues(query, limit)
    }

    fn list_paginated(&self, cursor: &SyncCursor) -> GroveResult<Vec<Issue>> {
        let limit = cursor.limit.to_string();
        if let Some(since) = cursor.since {
            // Use GitHub search API to filter by update date.
            let query = format!("updated:>{}", since.format("%Y-%m-%d"));
            let output = self.run_gh(&[
                "issue",
                "list",
                "--search",
                &query,
                "--state",
                "all",
                "--limit",
                &limit,
                "--json",
                "id,number,title,state,labels,body,assignees,url",
            ])?;
            self.enrich_issues(parse_issue_list(&output))
        } else {
            let output = self.run_gh(&[
                "issue",
                "list",
                "--state",
                "all",
                "--limit",
                &limit,
                "--json",
                "id,number,title,state,labels,body,assignees,url",
            ])?;
            self.enrich_issues(parse_issue_list(&output))
        }
    }

    fn comment(&self, id: &str, body: &str) -> GroveResult<String> {
        let output = self.run_gh(&["issue", "comment", id, "--body", body])?;
        // `gh issue comment` outputs the comment URL on success.
        let url = output
            .lines()
            .find(|l| l.contains("github.com"))
            .unwrap_or("")
            .trim()
            .to_string();
        Ok(url)
    }

    fn update(&self, id: &str, update: &IssueUpdate) -> GroveResult<Issue> {
        let mut args = vec!["issue", "edit", id];
        let title_owned;
        let body_owned;
        let labels_owned;
        if let Some(t) = &update.title {
            title_owned = t.clone();
            args.extend_from_slice(&["--title", &title_owned]);
        }
        if let Some(b) = &update.body {
            body_owned = b.clone();
            args.extend_from_slice(&["--body", &body_owned]);
        }
        if let Some(lbls) = &update.labels {
            labels_owned = lbls.join(",");
            args.extend_from_slice(&["--add-label", &labels_owned]);
        }
        self.run_gh(&args)?;
        self.show(id)
    }

    fn transition(&self, id: &str, target_status: &str) -> GroveResult<()> {
        self.apply_transition_for_repo(id, target_status, None)
    }

    fn assign(&self, id: &str, assignee: &str) -> GroveResult<()> {
        self.run_gh(&["issue", "edit", id, "--assignee", assignee])?;
        Ok(())
    }

    fn reopen(&self, id: &str) -> GroveResult<()> {
        self.run_gh(&["issue", "reopen", id])?;
        Ok(())
    }

    fn list_projects(&self) -> GroveResult<Vec<super::ProviderProject>> {
        let output = self.run_gh(&[
            "repo",
            "list",
            "--json",
            "name,nameWithOwner,url",
            "--limit",
            "100",
        ])?;
        let arr: Vec<Value> = serde_json::from_str(output.trim())
            .map_err(|e| GroveError::Runtime(format!("failed to parse gh repo list: {e}")))?;
        Ok(arr
            .iter()
            .map(|v| {
                let name_with_owner = v
                    .get("nameWithOwner")
                    .and_then(|s| s.as_str())
                    .unwrap_or_default()
                    .to_string();
                super::ProviderProject {
                    id: name_with_owner.clone(),
                    name: v
                        .get("name")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    // key = "owner/repo" — the value we pass to --repo in all gh commands
                    key: Some(name_with_owner),
                    url: v.get("url").and_then(|s| s.as_str()).map(|s| s.to_string()),
                }
            })
            .collect())
    }

    fn create_in_project(
        &self,
        title: &str,
        body: &str,
        project_key: &str,
    ) -> GroveResult<super::Issue> {
        // project_key is "owner/repo" (nameWithOwner) returned by list_projects.
        // Pass --repo so the issue is created in the selected repo, not the cwd repo.
        // `gh issue create` doesn't support --json, so we create first (returns URL),
        // then fetch the created issue via `gh issue view --json`.
        let create_output = self.run_gh(&[
            "issue",
            "create",
            "--repo",
            project_key,
            "--title",
            title,
            "--body",
            body,
        ])?;

        // `gh issue create` prints the new issue URL on stdout, e.g.
        // "https://github.com/owner/repo/issues/42\n"
        let url = create_output.trim();
        let issue_number = url.rsplit('/').next().unwrap_or("");

        let view_output = self.run_gh(&[
            "issue",
            "view",
            issue_number,
            "--repo",
            project_key,
            "--json",
            "id,number,title,state,labels,body,url",
        ])?;
        parse_issue(&view_output).map(|issue| self.enrich_issue(issue, Some(project_key)))
    }
}

fn parse_issue(raw: &str) -> GroveResult<Issue> {
    let v: Value = serde_json::from_str(raw.trim())
        .map_err(|e| GroveError::Runtime(format!("failed to parse GitHub issue JSON: {e}")))?;
    Ok(issue_from_json(&v))
}

fn parse_issue_list(raw: &str) -> GroveResult<Vec<Issue>> {
    let arr: Vec<Value> = serde_json::from_str(raw.trim())
        .map_err(|e| GroveError::Runtime(format!("failed to parse GitHub issue list JSON: {e}")))?;
    Ok(arr.iter().map(issue_from_json).collect())
}

fn issue_from_json(v: &Value) -> Issue {
    let provider_native_id = v
        .get("id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let external_id = v
        .get("number")
        .and_then(|n| n.as_i64())
        .map(|n| n.to_string())
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

    let labels: Vec<String> = v
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
    let assignee_logins = v
        .get("assignees")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.get("login").and_then(|login| login.as_str()))
                .map(|login| login.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Issue {
        external_id,
        provider: PROVIDER.to_string(),
        title,
        status,
        labels: labels.clone(),
        body,
        url,
        assignee,
        raw_json: v.clone(),
        provider_native_id,
        provider_scope_type: None,
        provider_scope_key: None,
        provider_scope_name: None,
        provider_metadata: serde_json::json!({
            "label_names": labels,
            "assignee_logins": assignee_logins,
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
