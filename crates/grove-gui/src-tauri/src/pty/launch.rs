use std::collections::HashMap;

use super::types::PtyOpenConfig;
use crate::state::AppState;

/// Resolve the CLI agent launch configuration for a conversation.
///
/// Returns `None` if the conversation is not a CLI conversation.
pub fn resolve_agent_launch(
    state: &AppState,
    conversation_id: &str,
) -> Result<Option<PtyOpenConfig>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let conversation =
        match grove_core::db::repositories::conversations_repo::get(&conn, conversation_id) {
            Ok(row) => row,
            Err(grove_core::errors::GroveError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };

    if conversation.conversation_kind
        != grove_core::orchestrator::conversation::CLI_CONVERSATION_KIND
    {
        return Ok(None);
    }

    let project = grove_core::db::repositories::projects_repo::get(&conn, &conversation.project_id)
        .map_err(|e| e.to_string())?;

    let worktree_path = conversation
        .worktree_path
        .clone()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| project.root_path.clone());

    let project_root = std::path::PathBuf::from(&project.root_path);

    let provider = conversation
        .cli_provider
        .clone()
        .ok_or_else(|| format!("CLI conversation '{conversation_id}' is missing cli_provider"))?;

    let (command, args) =
        resolve_cli_command(&project_root, &provider, conversation.cli_model.as_deref())?;

    let mut env = HashMap::new();
    env.insert("PATH".to_string(), shell_path().to_string());
    env.insert("TERM".to_string(), "xterm-256color".to_string());
    env.insert("COLORTERM".to_string(), "truecolor".to_string());
    env.insert("CLAUDECODE".to_string(), String::new());

    Ok(Some(PtyOpenConfig {
        cwd: worktree_path,
        command: Some((command, args)),
        env,
        cols: 120,
        rows: 32,
    }))
}

/// Resolve a shell launch configuration for a given working directory.
pub fn resolve_shell_launch(cwd: &str, shell: Option<&str>) -> PtyOpenConfig {
    let binary = shell
        .map(|s| s.to_string())
        .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()));

    let mut env = HashMap::new();
    env.insert("PATH".to_string(), shell_path().to_string());
    env.insert("TERM".to_string(), "xterm-256color".to_string());
    env.insert("COLORTERM".to_string(), "truecolor".to_string());

    PtyOpenConfig {
        cwd: cwd.to_string(),
        command: Some((binary, vec!["-l".to_string()])),
        env,
        cols: 120,
        rows: 32,
    }
}

/// Resolve the CLI binary + args for a given provider.
pub(crate) fn resolve_cli_command(
    project_root: &std::path::Path,
    provider_id: &str,
    model: Option<&str>,
) -> Result<(String, Vec<String>), String> {
    let cfg =
        grove_core::config::GroveConfig::load_or_create(project_root).map_err(|e| e.to_string())?;
    let mut args = Vec::new();

    let command = if provider_id == "claude_code" {
        if !cfg.providers.claude_code.enabled {
            return Err("Claude Code is disabled in Grove config.".to_string());
        }
        if let Some(model_id) = model.filter(|value| !value.trim().is_empty()) {
            args.push("--model".to_string());
            args.push(model_id.to_string());
        }
        cfg.providers.claude_code.command.clone()
    } else {
        let agent_cfg = cfg
            .providers
            .coding_agents
            .get(provider_id)
            .cloned()
            .ok_or_else(|| format!("unknown CLI provider '{provider_id}'"))?;
        if !agent_cfg.enabled {
            return Err(format!(
                "Provider '{provider_id}' is disabled in Grove config."
            ));
        }
        args.extend(agent_cfg.default_args);
        if let (Some(flag), Some(model_id)) = (
            agent_cfg.model_flag.as_deref(),
            model.filter(|value| !value.trim().is_empty()),
        ) {
            args.push(flag.to_string());
            args.push(model_id.to_string());
        }
        agent_cfg.command
    };

    let mut command_parts = command.split_whitespace();
    let binary = command_parts
        .next()
        .ok_or_else(|| format!("provider '{provider_id}' has an empty command"))?
        .to_string();
    let inline_args: Vec<String> = command_parts.map(|part| part.to_string()).collect();
    if !inline_args.is_empty() {
        let mut combined = inline_args;
        combined.extend(args);
        args = combined;
    }

    if which::which_in(&binary, Some(shell_path()), ".").is_err() {
        return Err(format!(
            "CLI binary '{binary}' for provider '{provider_id}' was not found on PATH."
        ));
    }

    Ok((binary, args))
}

/// Escape a string for safe use inside a POSIX shell command.
///
/// Returns the value unchanged when it only contains safe characters
/// (alphanumeric plus `/._-`). Otherwise wraps it in single quotes,
/// escaping any embedded single quotes.
pub(crate) fn shell_escape(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "/._-".contains(ch))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Re-export shell_path from commands.rs.
pub(crate) fn shell_path() -> &'static str {
    crate::commands::shell_path()
}
