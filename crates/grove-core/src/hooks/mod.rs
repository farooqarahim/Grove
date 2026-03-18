use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::config::{CapabilityGuard, HookDefinition, HookEvent, HooksConfig};
use crate::errors::{GroveError, GroveResult};

/// Context passed to hook commands via environment variables.
pub struct HookContext {
    pub run_id: String,
    pub session_id: Option<String>,
    pub agent_type: Option<String>,
    pub worktree_path: Option<String>,
    pub event: HookEvent,
}

/// Run all hooks registered for `event`. Blocking hooks abort on failure.
pub fn run_hooks(
    cfg: &HooksConfig,
    event: HookEvent,
    ctx: &HookContext,
    project_root: &Path,
) -> GroveResult<()> {
    let hooks = match cfg.on.get(&event) {
        Some(hooks) => hooks,
        None => return Ok(()),
    };

    for hook in hooks {
        match execute_hook(hook, ctx, project_root) {
            Ok(()) => {}
            Err(e) => {
                if hook.blocking {
                    return Err(e);
                }
                tracing::warn!(
                    event = ?event,
                    command = %hook.command,
                    error = %e,
                    "non-blocking hook failed — continuing"
                );
            }
        }
    }

    Ok(())
}

/// Check if a file path is allowed by the guard for an agent type.
///
/// Returns `true` if no guard exists for this agent type, or if the path
/// passes all guard checks.
pub fn check_file_guard(
    guards: &HashMap<String, CapabilityGuard>,
    agent_type: &str,
    file_path: &str,
) -> bool {
    let guard = match guards.get(agent_type) {
        Some(g) => g,
        None => return true,
    };

    // Check blocked paths first
    for pattern in &guard.blocked_paths {
        if glob_matches(pattern, file_path) {
            return false;
        }
    }

    // If allowed_paths is non-empty, file must match at least one
    if !guard.allowed_paths.is_empty() {
        return guard
            .allowed_paths
            .iter()
            .any(|p| glob_matches(p, file_path));
    }

    true
}

/// Check if a tool is allowed by the guard for an agent type.
///
/// Returns `true` if no guard exists or if the tool is not blocked.
pub fn check_tool_guard(
    guards: &HashMap<String, CapabilityGuard>,
    agent_type: &str,
    tool_name: &str,
) -> bool {
    let guard = match guards.get(agent_type) {
        Some(g) => g,
        None => return true,
    };

    !guard.blocked_tools.iter().any(|t| t == tool_name)
}

fn execute_hook(hook: &HookDefinition, ctx: &HookContext, project_root: &Path) -> GroveResult<()> {
    let parts: Vec<&str> = hook.command.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(());
    }

    let event_str = match ctx.event {
        HookEvent::SessionStart => "session_start",
        HookEvent::UserPromptSubmit => "user_prompt_submit",
        HookEvent::PreToolUse => "pre_tool_use",
        HookEvent::PostToolUse => "post_tool_use",
        HookEvent::Stop => "stop",
        HookEvent::PreCompact => "pre_compact",
        HookEvent::PostRun => "post_run",
        HookEvent::PreMerge => "pre_merge",
    };

    let mut cmd = Command::new(parts[0]);
    cmd.args(&parts[1..])
        .current_dir(project_root)
        .env("PATH", crate::capability::shell_path())
        .env("GROVE_HOOK_EVENT", event_str)
        .env("GROVE_RUN_ID", &ctx.run_id);

    if let Some(ref sid) = ctx.session_id {
        cmd.env("GROVE_SESSION_ID", sid);
    }
    if let Some(ref at) = ctx.agent_type {
        cmd.env("GROVE_AGENT_TYPE", at);
    }
    if let Some(ref wt) = ctx.worktree_path {
        cmd.env("GROVE_WORKTREE_PATH", wt);
    }

    let timeout = Duration::from_secs(hook.timeout_secs);

    let mut child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| GroveError::HookError {
            hook: hook.command.clone(),
            message: format!("spawn failed: {e}"),
        })?;

    // Enforce timeout: poll in a background thread to kill the process if it hangs.
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(GroveError::HookError {
                        hook: hook.command.clone(),
                        message: format!("timed out after {}s", hook.timeout_secs),
                    });
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(GroveError::HookError {
                    hook: hook.command.clone(),
                    message: format!("wait failed: {e}"),
                });
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|e| GroveError::HookError {
            hook: hook.command.clone(),
            message: format!("output read failed: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GroveError::HookError {
            hook: hook.command.clone(),
            message: format!(
                "exit {}: {}",
                output.status,
                stderr.chars().take(500).collect::<String>()
            ),
        });
    }

    Ok(())
}

/// Simple glob matching: supports `*` (any chars) and `**` (any path segments).
fn glob_matches(pattern: &str, path: &str) -> bool {
    if pattern.contains("**") {
        // Split on ** and check if path contains all segments in order
        let parts: Vec<&str> = pattern.split("**").collect();
        let mut remaining = path;
        for part in parts {
            let needle = part.replace('*', "");
            if needle.is_empty() {
                continue;
            }
            match remaining.find(&needle) {
                Some(idx) => remaining = &remaining[idx + needle.len()..],
                None => return false,
            }
        }
        true
    } else if pattern.contains('*') {
        // Simple wildcard: split on * and match segments
        let parts: Vec<&str> = pattern.split('*').collect();
        let mut remaining = path;
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if i == 0 {
                if !remaining.starts_with(part) {
                    return false;
                }
                remaining = &remaining[part.len()..];
            } else {
                match remaining.find(part) {
                    Some(idx) => remaining = &remaining[idx + part.len()..],
                    None => return false,
                }
            }
        }
        true
    } else {
        pattern == path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CapabilityGuard, HookDefinition, HookEvent, HooksConfig};
    use std::collections::HashMap;

    #[test]
    fn test_run_hooks_empty() {
        let cfg = HooksConfig::default();
        let ctx = HookContext {
            run_id: "run_1".into(),
            session_id: None,
            agent_type: None,
            worktree_path: None,
            event: HookEvent::PostRun,
        };
        let dir = tempfile::tempdir().unwrap();
        let result = run_hooks(&cfg, HookEvent::PostRun, &ctx, dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_blocking_hook_failure_aborts() {
        let mut on = HashMap::new();
        on.insert(
            HookEvent::PostRun,
            vec![HookDefinition {
                command: "false".into(), // always fails
                blocking: true,
                timeout_secs: 5,
            }],
        );
        let cfg = HooksConfig {
            post_run: vec![],
            on,
            guards: HashMap::new(),
        };
        let ctx = HookContext {
            run_id: "run_2".into(),
            session_id: None,
            agent_type: None,
            worktree_path: None,
            event: HookEvent::PostRun,
        };
        let dir = tempfile::tempdir().unwrap();
        let result = run_hooks(&cfg, HookEvent::PostRun, &ctx, dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_nonblocking_hook_continues() {
        let mut on = HashMap::new();
        on.insert(
            HookEvent::PostRun,
            vec![HookDefinition {
                command: "false".into(),
                blocking: false,
                timeout_secs: 5,
            }],
        );
        let cfg = HooksConfig {
            post_run: vec![],
            on,
            guards: HashMap::new(),
        };
        let ctx = HookContext {
            run_id: "run_3".into(),
            session_id: None,
            agent_type: None,
            worktree_path: None,
            event: HookEvent::PostRun,
        };
        let dir = tempfile::tempdir().unwrap();
        let result = run_hooks(&cfg, HookEvent::PostRun, &ctx, dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_guard_allow() {
        let mut guards = HashMap::new();
        guards.insert(
            "builder".into(),
            CapabilityGuard {
                allowed_paths: vec!["src/**".into()],
                blocked_paths: vec![],
                blocked_tools: vec![],
            },
        );
        assert!(check_file_guard(&guards, "builder", "src/main.rs"));
        assert!(!check_file_guard(
            &guards,
            "builder",
            "config/settings.yaml"
        ));
    }

    #[test]
    fn test_file_guard_block() {
        let mut guards = HashMap::new();
        guards.insert(
            "tester".into(),
            CapabilityGuard {
                allowed_paths: vec![],
                blocked_paths: vec!["*.env".into(), "secrets/**".into()],
                blocked_tools: vec![],
            },
        );
        assert!(!check_file_guard(&guards, "tester", "production.env"));
        assert!(!check_file_guard(&guards, "tester", "secrets/api_key.txt"));
        assert!(check_file_guard(&guards, "tester", "src/main.rs"));
    }

    #[test]
    fn test_tool_guard_block() {
        let mut guards = HashMap::new();
        guards.insert(
            "documenter".into(),
            CapabilityGuard {
                allowed_paths: vec![],
                blocked_paths: vec![],
                blocked_tools: vec!["Bash".into(), "Write".into()],
            },
        );
        assert!(!check_tool_guard(&guards, "documenter", "Bash"));
        assert!(!check_tool_guard(&guards, "documenter", "Write"));
        assert!(check_tool_guard(&guards, "documenter", "Read"));
        // No guard for builder — everything allowed
        assert!(check_tool_guard(&guards, "builder", "Bash"));
    }

    #[test]
    fn test_hook_timeout() {
        // The timeout is set but we rely on the command to complete
        // This test verifies a fast command succeeds within timeout
        let mut on = HashMap::new();
        on.insert(
            HookEvent::SessionStart,
            vec![HookDefinition {
                command: "true".into(), // always succeeds
                blocking: true,
                timeout_secs: 1,
            }],
        );
        let cfg = HooksConfig {
            post_run: vec![],
            on,
            guards: HashMap::new(),
        };
        let ctx = HookContext {
            run_id: "run_t".into(),
            session_id: None,
            agent_type: Some("builder".into()),
            worktree_path: None,
            event: HookEvent::SessionStart,
        };
        let dir = tempfile::tempdir().unwrap();
        assert!(run_hooks(&cfg, HookEvent::SessionStart, &ctx, dir.path()).is_ok());
    }
}
