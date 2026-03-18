use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::{GroveError, GroveResult};

/// Overall CI pipeline status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CiOverall {
    Passing,
    Failing,
    Pending,
}

impl CiOverall {
    pub fn as_str(self) -> &'static str {
        match self {
            CiOverall::Passing => "passing",
            CiOverall::Failing => "failing",
            CiOverall::Pending => "pending",
        }
    }
}

/// A single CI check run (GitHub Actions job, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckRun {
    pub name: String,
    pub status: String,
    pub conclusion: Option<String>,
    pub url: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Aggregated CI status for a branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiStatus {
    pub checks: Vec<CheckRun>,
    pub overall: CiOverall,
    pub branch: String,
}

/// Fetch CI status for a branch using `gh` CLI.
///
/// Requires `gh` CLI to be installed and authenticated.
pub fn get_ci_status(project_root: &Path, branch: &str) -> GroveResult<CiStatus> {
    let output = Command::new("gh")
        .args([
            "pr",
            "checks",
            branch,
            "--json",
            "name,state,conclusion,detailsUrl,startedAt,completedAt",
        ])
        .current_dir(project_root)
        .env("PATH", crate::capability::shell_path())
        .output()
        .map_err(|e| GroveError::Runtime(format!("gh pr checks failed to start: {e}")))?;

    // If `gh pr checks` fails (e.g., no PR), fall back to `gh run list`
    let json_str = if output.status.success() {
        String::from_utf8_lossy(&output.stdout).to_string()
    } else {
        let fallback = Command::new("gh")
            .args([
                "run",
                "list",
                "--branch",
                branch,
                "--limit",
                "10",
                "--json",
                "name,status,conclusion,url,createdAt,updatedAt",
            ])
            .current_dir(project_root)
            .env("PATH", crate::capability::shell_path())
            .output()
            .map_err(|e| GroveError::Runtime(format!("gh run list failed: {e}")))?;

        if !fallback.status.success() {
            let stderr = String::from_utf8_lossy(&fallback.stderr);
            return Err(GroveError::Runtime(format!(
                "failed to fetch CI status for branch '{branch}': {stderr}"
            )));
        }
        String::from_utf8_lossy(&fallback.stdout).to_string()
    };

    let checks = parse_checks_json(&json_str);
    let overall = compute_overall(&checks);

    Ok(CiStatus {
        checks,
        overall,
        branch: branch.to_string(),
    })
}

/// Poll CI until all checks complete or timeout.
pub fn wait_for_ci(project_root: &Path, branch: &str, timeout_secs: u64) -> GroveResult<CiStatus> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_secs(15);

    loop {
        let status = get_ci_status(project_root, branch)?;
        if status.overall != CiOverall::Pending {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Ok(status); // return current state on timeout
        }
        std::thread::sleep(poll_interval);
    }
}

/// Build an objective from failing CI checks for an agent to fix.
pub fn failing_checks_to_objective(status: &CiStatus) -> String {
    let failures: Vec<&CheckRun> = status
        .checks
        .iter()
        .filter(|c| c.conclusion.as_deref() == Some("failure"))
        .collect();

    if failures.is_empty() {
        return "All CI checks are passing.".to_string();
    }

    let mut lines = vec![format!(
        "Fix the following failing CI checks on branch '{}':\n",
        status.branch
    )];

    for check in &failures {
        lines.push(format!("- **{}**: failed", check.name));
        if !check.url.is_empty() {
            lines.push(format!("  URL: {}", check.url));
        }
    }

    lines.push(String::new());
    lines.push("Investigate the failures, identify the root cause, and fix the code.".to_string());

    lines.join("\n")
}

fn parse_checks_json(json_str: &str) -> Vec<CheckRun> {
    let Ok(arr) = serde_json::from_str::<Vec<Value>>(json_str) else {
        return vec![];
    };

    arr.iter()
        .map(|v| CheckRun {
            name: v
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            status: v
                .get("state")
                .or_else(|| v.get("status"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            conclusion: v
                .get("conclusion")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            url: v
                .get("detailsUrl")
                .or_else(|| v.get("url"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            started_at: v
                .get("startedAt")
                .or_else(|| v.get("createdAt"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            completed_at: v
                .get("completedAt")
                .or_else(|| v.get("updatedAt"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
        .collect()
}

fn compute_overall(checks: &[CheckRun]) -> CiOverall {
    if checks.is_empty() {
        return CiOverall::Pending;
    }

    let has_failure = checks
        .iter()
        .any(|c| c.conclusion.as_deref() == Some("failure"));
    let has_pending = checks
        .iter()
        .any(|c| c.status == "in_progress" || c.status == "queued" || c.status == "pending");

    if has_failure {
        CiOverall::Failing
    } else if has_pending {
        CiOverall::Pending
    } else {
        CiOverall::Passing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_checks_json_gh_pr_checks() {
        let json = r#"[
            {"name": "build", "state": "completed", "conclusion": "success", "detailsUrl": "https://github.com/org/repo/actions/runs/1"},
            {"name": "test", "state": "completed", "conclusion": "failure", "detailsUrl": "https://github.com/org/repo/actions/runs/2"}
        ]"#;
        let checks = parse_checks_json(json);
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0].name, "build");
        assert_eq!(checks[0].conclusion.as_deref(), Some("success"));
        assert_eq!(checks[1].conclusion.as_deref(), Some("failure"));
    }

    #[test]
    fn compute_overall_passing() {
        let checks = vec![CheckRun {
            name: "build".into(),
            status: "completed".into(),
            conclusion: Some("success".into()),
            url: String::new(),
            started_at: None,
            completed_at: None,
        }];
        assert_eq!(compute_overall(&checks), CiOverall::Passing);
    }

    #[test]
    fn compute_overall_failing() {
        let checks = vec![
            CheckRun {
                name: "build".into(),
                status: "completed".into(),
                conclusion: Some("success".into()),
                url: String::new(),
                started_at: None,
                completed_at: None,
            },
            CheckRun {
                name: "test".into(),
                status: "completed".into(),
                conclusion: Some("failure".into()),
                url: String::new(),
                started_at: None,
                completed_at: None,
            },
        ];
        assert_eq!(compute_overall(&checks), CiOverall::Failing);
    }

    #[test]
    fn compute_overall_pending() {
        let checks = vec![CheckRun {
            name: "build".into(),
            status: "in_progress".into(),
            conclusion: None,
            url: String::new(),
            started_at: None,
            completed_at: None,
        }];
        assert_eq!(compute_overall(&checks), CiOverall::Pending);
    }

    #[test]
    fn failing_checks_objective() {
        let status = CiStatus {
            checks: vec![
                CheckRun {
                    name: "build".into(),
                    status: "completed".into(),
                    conclusion: Some("success".into()),
                    url: String::new(),
                    started_at: None,
                    completed_at: None,
                },
                CheckRun {
                    name: "test".into(),
                    status: "completed".into(),
                    conclusion: Some("failure".into()),
                    url: "https://ci.example.com/run/2".into(),
                    started_at: None,
                    completed_at: None,
                },
            ],
            overall: CiOverall::Failing,
            branch: "feature/foo".into(),
        };
        let objective = failing_checks_to_objective(&status);
        assert!(objective.contains("failing CI checks"));
        assert!(objective.contains("feature/foo"));
        assert!(objective.contains("test"));
        assert!(objective.contains("https://ci.example.com/run/2"));
    }
}
