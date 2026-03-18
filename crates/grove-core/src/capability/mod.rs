use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use crate::config::GroveConfig;
use serde::Deserialize;

/// How degraded the runtime environment is.
/// Higher levels indicate more functionality missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DegradationLevel {
    /// Everything works.
    Full = 0,
    /// No git remote configured (can't push/create PRs).
    NoRemote = 1,
    /// Provider binary is missing (can't run agents).
    NoProvider = 2,
    /// No internet connectivity (no marketplace, no telemetry).
    NoInternet = 3,
    /// Database is inaccessible.
    DegradedDb = 4,
}

impl DegradationLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NoRemote => "no_remote",
            Self::NoProvider => "no_provider",
            Self::NoInternet => "no_internet",
            Self::DegradedDb => "degraded_db",
        }
    }
}

/// Result of a single capability probe.
#[derive(Debug, Clone)]
pub struct CapabilityCheck {
    pub name: &'static str,
    pub available: bool,
    pub message: String,
}

/// Aggregated result of all capability probes.
#[derive(Debug, Clone)]
pub struct CapabilityReport {
    pub level: DegradationLevel,
    pub checks: Vec<CapabilityCheck>,
}

/// Run fast environment probes and return a capability report.
///
/// When `db_path_override` is provided (e.g. centralized `~/.grove/` DB), the
/// database check uses that path instead of deriving from `project_root`.
pub fn detect_capabilities_with_db(
    cfg: &GroveConfig,
    project_root: &Path,
    db_path_override: Option<&std::path::Path>,
) -> CapabilityReport {
    let mut checks = Vec::new();
    let mut level = DegradationLevel::Full;

    // 1. DB accessible
    let db_path = match db_path_override {
        Some(p) => p.to_path_buf(),
        None => crate::config::db_path(project_root),
    };
    let db_ok = crate::db::connection::open(&db_path).is_ok();
    checks.push(CapabilityCheck {
        name: "database",
        available: db_ok,
        message: if db_ok {
            "SQLite database accessible".into()
        } else {
            format!("Cannot open database at {}", db_path.display())
        },
    });
    if !db_ok {
        level = level.max(DegradationLevel::DegradedDb);
    }

    // 2. git on PATH
    let git_ok = which_on_shell_path("git");
    checks.push(CapabilityCheck {
        name: "git",
        available: git_ok,
        message: if git_ok {
            "git found on PATH".into()
        } else {
            "git not found on PATH".into()
        },
    });

    // 3. Provider binary
    let provider_cmd = &cfg.providers.claude_code.command;
    let provider_ok = which_on_shell_path(provider_cmd);
    checks.push(CapabilityCheck {
        name: "provider",
        available: provider_ok,
        message: if provider_ok {
            format!("provider binary `{provider_cmd}` found")
        } else {
            format!("provider binary `{provider_cmd}` not found on PATH")
        },
    });
    if !provider_ok {
        level = level.max(DegradationLevel::NoProvider);
    }

    // 4. git remote
    let remote_ok = check_git_remote(project_root);
    checks.push(CapabilityCheck {
        name: "git_remote",
        available: remote_ok,
        message: if remote_ok {
            "git remote origin configured".into()
        } else {
            "no git remote origin — push/PR features unavailable".into()
        },
    });
    if !remote_ok && level < DegradationLevel::NoRemote {
        level = level.max(DegradationLevel::NoRemote);
    }

    // 5. Internet connectivity (fast TCP probe to Cloudflare DNS)
    let internet_ok = check_internet();
    checks.push(CapabilityCheck {
        name: "internet",
        available: internet_ok,
        message: if internet_ok {
            "internet connectivity confirmed".into()
        } else {
            "no internet connectivity detected".into()
        },
    });
    if !internet_ok {
        level = level.max(DegradationLevel::NoInternet);
    }

    CapabilityReport { level, checks }
}

/// Convenience wrapper — delegates to [`detect_capabilities_with_db`] with no DB override.
pub fn detect_capabilities(cfg: &GroveConfig, project_root: &Path) -> CapabilityReport {
    detect_capabilities_with_db(cfg, project_root, None)
}

/// Validate that required capabilities are present for the requested operation.
///
/// Returns `Ok(())` if the environment is sufficient, or `Err(message)` if not.
pub fn preflight_check(report: &CapabilityReport, needs_provider: bool) -> Result<(), String> {
    let db_ok = report
        .checks
        .iter()
        .find(|c| c.name == "database")
        .map(|c| c.available)
        .unwrap_or(true);
    if !db_ok {
        return Err("database is inaccessible — cannot proceed".into());
    }
    let provider_ok = report
        .checks
        .iter()
        .find(|c| c.name == "provider")
        .map(|c| c.available)
        .unwrap_or(true);
    if needs_provider && !provider_ok {
        let msg = report
            .checks
            .iter()
            .find(|c| c.name == "provider")
            .map(|c| c.message.clone())
            .unwrap_or_else(|| "provider binary not found".into());
        return Err(msg);
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ClaudeAuthStatus {
    #[serde(rename = "loggedIn", default)]
    logged_in: bool,
    #[serde(rename = "apiKeySource", default)]
    api_key_source: Option<String>,
}

impl ClaudeAuthStatus {
    fn is_authenticated(&self) -> bool {
        self.logged_in
            || self
                .api_key_source
                .as_deref()
                .is_some_and(|value| !value.is_empty() && !value.eq_ignore_ascii_case("none"))
    }
}

/// Returns whether the configured Claude Code CLI appears authenticated.
///
/// `Ok(false)` means the probe explicitly determined that Claude Code is not
/// ready. `Err(...)` means the readiness probe itself failed to execute. If the
/// installed CLI does not support `auth status --json`, this returns `Ok(true)`
/// so older working installs are not blocked by the preflight.
pub fn is_claude_code_authenticated(command: &str) -> Result<bool, String> {
    let output = match command_output_with_timeout(
        command,
        &["auth", "status", "--json"],
        Duration::from_secs(5),
    ) {
        Ok(output) => output,
        Err(err) => return Err(err),
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if stdout.is_empty() {
        if looks_like_unsupported_auth_probe(&stderr) {
            return Ok(true);
        }
        if !output.status.success() {
            let detail = if stderr.is_empty() {
                "no output from `claude auth status --json`".to_string()
            } else {
                stderr
            };
            return Err(format!("failed to query Claude Code auth status: {detail}"));
        }
        return Ok(true);
    }

    match serde_json::from_str::<ClaudeAuthStatus>(&stdout) {
        Ok(status) => Ok(status.is_authenticated()),
        Err(_)
            if looks_like_unsupported_auth_probe(&stdout)
                || looks_like_unsupported_auth_probe(&stderr) =>
        {
            Ok(true)
        }
        Err(_) => Ok(true),
    }
}

pub fn ensure_claude_code_authenticated(command: &str) -> Result<(), String> {
    // Check binary exists before attempting auth probe so the error is actionable.
    if !which_on_shell_path(command) {
        return Err(format!(
            "Claude Code CLI `{command}` is not installed — install it with `npm install -g @anthropic-ai/claude-code` (requires Node 18+)"
        ));
    }

    if is_claude_code_authenticated(command)? {
        return Ok(());
    }

    Err(format!(
        "Claude Code CLI `{command}` is not authenticated — run `claude auth login` or open `claude` and use `/login`"
    ))
}

/// Look up a binary using the user's full login-shell PATH.
///
/// macOS GUI apps start with a minimal system PATH. This function replicates
/// the same shell-PATH resolution used by `shell_path()` in grove-gui so that
/// capability checks and agent spawning see the same binaries the user sees in
/// their terminal.
fn which_on_shell_path(binary: &str) -> bool {
    let shell_path = shell_path();
    which::which_in(binary, Some(shell_path), ".").is_ok()
}

fn command_output_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output, String> {
    let mut child = Command::new(command)
        .args(args)
        .env("PATH", shell_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch `{command}`: {e}"))?;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                return Err(format!(
                    "`{command} {}` timed out after {}s",
                    args.join(" "),
                    timeout.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Err(format!("failed while waiting for `{command}`: {e}")),
        }
    }

    child
        .wait_with_output()
        .map_err(|e| format!("failed to capture output from `{command}`: {e}"))
}

fn looks_like_unsupported_auth_probe(output: &str) -> bool {
    let lowered = output.to_ascii_lowercase();
    lowered.contains("unknown option")
        || lowered.contains("unknown argument")
        || lowered.contains("unexpected argument")
        || lowered.contains("unknown command")
        || lowered.contains("unrecognized subcommand")
}

/// Returns the user's full login-shell PATH, cached after the first call.
///
/// Use this when spawning CLI agent processes so they can find binaries
/// installed via homebrew, npm, pipx, cargo, etc. — locations that are
/// invisible to macOS GUI apps which start with a minimal system PATH.
pub fn shell_path() -> &'static str {
    static CACHE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        let user_shell = std::env::var("SHELL").unwrap_or_default();

        let shell_attempts: &[(&str, &[&str])] = &[
            (&user_shell, &["-ilc", "echo $PATH"]),
            (&user_shell, &["-lc", "echo $PATH"]),
            ("/bin/zsh", &["-ilc", "echo $PATH"]),
            ("/bin/zsh", &["-lc", "echo $PATH"]),
            ("/bin/bash", &["-lc", "echo $PATH"]),
        ];

        let mut shell_derived = String::new();
        'outer: for (shell, args) in shell_attempts {
            if shell.is_empty() {
                continue;
            }
            let mut child = match std::process::Command::new(shell)
                .args(*args)
                .stdin(std::process::Stdio::null())
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
                        continue 'outer;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                    Err(_) => continue 'outer,
                }
            }
            if let Ok(out) = child.wait_with_output() {
                if out.status.success() {
                    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !path.is_empty() {
                        shell_derived = path;
                        break 'outer;
                    }
                }
            }
        }

        if shell_derived.is_empty() {
            shell_derived = std::env::var("PATH").unwrap_or_default();
        }

        let well_known: &[String] = &[
            format!("{}/.local/bin", home),
            format!("{}/.cargo/bin", home),
            format!("{}/.bun/bin", home),
            format!("{}/.npm-global/bin", home),
            format!("{}/.claude/local/node_modules/.bin", home),
            "/opt/homebrew/bin".to_string(),
            "/opt/homebrew/sbin".to_string(),
            "/usr/local/bin".to_string(),
            "/usr/local/sbin".to_string(),
        ];

        let existing_parts: Vec<&str> = shell_derived.split(':').collect();
        let mut extra: Vec<&str> = well_known
            .iter()
            .filter(|p| {
                !p.is_empty()
                    && std::path::Path::new(p.as_str()).is_dir()
                    && !existing_parts.contains(&p.as_str())
            })
            .map(|p| p.as_str())
            .collect();
        extra.extend(existing_parts);
        extra.join(":")
    })
}

fn check_git_remote(project_root: &Path) -> bool {
    std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_internet() -> bool {
    let addr: SocketAddr = "1.1.1.1:443".parse().expect("valid socket addr");
    TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_capability_report_structure() {
        // Create a minimal config
        let cfg: GroveConfig =
            serde_yaml::from_str(crate::config::DEFAULT_CONFIG_YAML).expect("default config");
        let dir = tempfile::tempdir().expect("tempdir");
        let report = detect_capabilities(&cfg, dir.path());
        // Should have exactly 5 checks
        assert_eq!(report.checks.len(), 5);
        let names: Vec<&str> = report.checks.iter().map(|c| c.name).collect();
        assert!(names.contains(&"database"));
        assert!(names.contains(&"git"));
        assert!(names.contains(&"provider"));
        assert!(names.contains(&"git_remote"));
        assert!(names.contains(&"internet"));
    }

    #[test]
    fn test_no_provider_detected() {
        let yaml = crate::config::DEFAULT_CONFIG_YAML.replace(
            "command: \"claude\"",
            "command: \"nonexistent_binary_xyz_123\"",
        );
        let cfg: GroveConfig = serde_yaml::from_str(&yaml).expect("config");
        let dir = tempfile::tempdir().expect("tempdir");
        let report = detect_capabilities(&cfg, dir.path());
        assert!(report.level >= DegradationLevel::NoProvider);
        let provider_check = report.checks.iter().find(|c| c.name == "provider").unwrap();
        assert!(!provider_check.available);
    }

    #[test]
    fn test_preflight_blocks_no_provider() {
        let report = CapabilityReport {
            level: DegradationLevel::NoProvider,
            checks: vec![CapabilityCheck {
                name: "provider",
                available: false,
                message: "provider binary `claude` not found on PATH".into(),
            }],
        };
        assert!(preflight_check(&report, true).is_err());
    }

    #[test]
    fn test_preflight_allows_no_remote() {
        let report = CapabilityReport {
            level: DegradationLevel::NoRemote,
            checks: vec![],
        };
        // NoRemote should not block provider-requiring operations
        assert!(preflight_check(&report, true).is_ok());
        assert!(preflight_check(&report, false).is_ok());
    }

    #[test]
    fn test_preflight_allows_no_internet_when_provider_exists() {
        let report = CapabilityReport {
            level: DegradationLevel::NoInternet,
            checks: vec![
                CapabilityCheck {
                    name: "database",
                    available: true,
                    message: "SQLite database accessible".into(),
                },
                CapabilityCheck {
                    name: "provider",
                    available: true,
                    message: "provider binary `claude` found".into(),
                },
                CapabilityCheck {
                    name: "internet",
                    available: false,
                    message: "no internet connectivity detected".into(),
                },
            ],
        };
        assert!(preflight_check(&report, true).is_ok());
    }

    #[test]
    fn claude_auth_status_accepts_logged_in_session() {
        let status: ClaudeAuthStatus =
            serde_json::from_str(r#"{"loggedIn":true,"apiKeySource":"user"}"#)
                .expect("status json");
        assert!(status.is_authenticated());
    }

    #[test]
    fn claude_auth_status_accepts_api_key_auth() {
        let status: ClaudeAuthStatus =
            serde_json::from_str(r#"{"loggedIn":false,"apiKeySource":"env"}"#)
                .expect("status json");
        assert!(status.is_authenticated());
    }

    #[test]
    fn claude_auth_status_rejects_missing_auth() {
        let status: ClaudeAuthStatus =
            serde_json::from_str(r#"{"loggedIn":false,"apiKeySource":"none"}"#)
                .expect("status json");
        assert!(!status.is_authenticated());
    }
}
