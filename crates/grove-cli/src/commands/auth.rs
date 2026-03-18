/// `grove auth` — manage API keys for direct LLM providers.
///
/// Keys are stored in `<XDG_DATA_HOME>/grove/auth.json` (Linux) or
/// `~/Library/Application Support/grove/auth.json` (macOS) with `0o600`
/// permissions.
///
/// Priority order for key resolution at runtime:
///   1. Environment variable (e.g. `ANTHROPIC_API_KEY`)
///   2. Stored key in auth.json
use anyhow::{Result, bail};
use grove_core::llm::{AuthInfo, AuthStore, LlmProviderKind, LlmRouter};
use serde_json::json;

use crate::cli::{AuthAction, AuthArgs};
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &AuthArgs) -> Result<CommandOutput> {
    match &args.action {
        AuthAction::Set(a) => handle_set(ctx, &a.provider, &a.api_key),
        AuthAction::Remove(a) => handle_remove(ctx, &a.provider),
        AuthAction::List => handle_list(ctx),
    }
}

fn resolve_kind(provider: &str) -> Result<LlmProviderKind> {
    LlmProviderKind::from_str(provider).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown provider '{}'. Valid values: anthropic, openai, deepseek, inception",
            provider
        )
    })
}

fn handle_set(_ctx: &CommandContext, provider: &str, api_key: &str) -> Result<CommandOutput> {
    if api_key.is_empty() {
        bail!("api-key must not be empty");
    }

    let kind = resolve_kind(provider)?;
    LlmRouter::set_api_key(kind, api_key)?;

    let text = format!(
        "Stored API key for {} ({}). Key path: {}",
        kind.display_name(),
        kind.id(),
        AuthStore::path().display(),
    );
    let json_val = json!({
        "provider": kind.id(),
        "name": kind.display_name(),
        "stored": true,
        "path": AuthStore::path().to_string_lossy(),
    });

    Ok(to_text_or_json(
        crate::cli::OutputFormat::Text,
        text,
        json_val,
    ))
}

fn handle_remove(_ctx: &CommandContext, provider: &str) -> Result<CommandOutput> {
    let kind = resolve_kind(provider)?;
    LlmRouter::remove_api_key(kind)?;

    let text = format!(
        "Removed stored API key for {} ({})",
        kind.display_name(),
        kind.id(),
    );
    let json_val = json!({ "provider": kind.id(), "removed": true });

    Ok(to_text_or_json(
        crate::cli::OutputFormat::Text,
        text,
        json_val,
    ))
}

fn handle_list(_ctx: &CommandContext) -> Result<CommandOutput> {
    let statuses = LlmRouter::providers();
    let auth_path = AuthStore::path();

    let mut lines = vec![
        format!("Auth store: {}", auth_path.display()),
        String::new(),
        format!("{:<16} {:<20} {}", "PROVIDER", "NAME", "STATUS"),
        format!("{:<16} {:<20} {}", "--------", "----", "------"),
    ];

    for s in &statuses {
        let env_var = grove_core::llm::auth::env_var_for(s.kind.id());
        let status = if std::env::var(&env_var).is_ok() {
            format!("authenticated (via {})", env_var)
        } else {
            match AuthStore::get(s.kind.id()) {
                Some(AuthInfo::Api { .. }) => "authenticated (stored)".to_string(),
                Some(_) => "configured (workspace credits)".to_string(),
                None => "not configured".to_string(),
            }
        };
        lines.push(format!("{:<16} {:<20} {}", s.kind.id(), s.name, status));
    }

    let json_providers: Vec<_> = statuses
        .iter()
        .map(|s| {
            let env_var = grove_core::llm::auth::env_var_for(s.kind.id());
            let via_env = std::env::var(&env_var).is_ok();
            let via_store =
                !via_env && matches!(AuthStore::get(s.kind.id()), Some(AuthInfo::Api { .. }));
            json!({
                "provider": s.kind.id(),
                "name": s.name,
                "authenticated": s.authenticated,
                "via_env": via_env,
                "via_store": via_store,
                "env_var": env_var,
            })
        })
        .collect();

    let json_val = json!({
        "auth_path": auth_path.to_string_lossy(),
        "providers": json_providers,
    });

    Ok(to_text_or_json(
        crate::cli::OutputFormat::Text,
        lines.join("\n"),
        json_val,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_kind_accepts_aliases() {
        assert!(resolve_kind("anthropic").is_ok());
        assert!(resolve_kind("claude").is_ok());
        assert!(resolve_kind("openai").is_ok());
        assert!(resolve_kind("open-ai").is_ok());
        assert!(resolve_kind("deepseek").is_ok());
        assert!(resolve_kind("inception").is_ok());
        assert!(resolve_kind("mercury").is_ok());
    }

    #[test]
    fn resolve_kind_rejects_unknown() {
        assert!(resolve_kind("foobar").is_err());
    }

    #[test]
    fn handle_set_rejects_empty_key() {
        let ctx = CommandContext {
            project_root: std::path::PathBuf::from("."),
            format: crate::cli::OutputFormat::Text,
            _verbose: false,
            _no_color: false,
        };
        let result = handle_set(&ctx, "anthropic", "");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }
}
