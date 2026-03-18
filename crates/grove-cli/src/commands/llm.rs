/// `grove llm` — manage providers, models, workspace selection, and credits.
use anyhow::{Result, bail};
use grove_core::db;
use grove_core::db::repositories::workspaces_repo;
use grove_core::llm::{LlmAuthMode, LlmProviderKind, LlmRouter, LlmSelection};
use grove_core::orchestrator::workspace::ensure_workspace;
use serde_json::json;

use crate::cli::{LlmAction, LlmArgs, LlmCreditsAction};
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &LlmArgs) -> Result<CommandOutput> {
    match &args.action {
        LlmAction::List => handle_list(ctx),
        LlmAction::Models(a) => handle_models(ctx, &a.provider),
        LlmAction::Select(a) => {
            handle_select(ctx, &a.provider, a.model.as_deref(), a.workspace_credits)
        }
        LlmAction::Credits(a) => match &a.action {
            LlmCreditsAction::Balance => handle_credits_balance(ctx),
            LlmCreditsAction::Add(add) => handle_credits_add(ctx, add.amount_usd),
        },
    }
}

fn workspace_conn(ctx: &CommandContext) -> Result<(String, rusqlite::Connection)> {
    db::initialize(&ctx.project_root)?;
    let conn = grove_core::db::DbHandle::new(&ctx.project_root).connect()?;
    let ws_id = ensure_workspace(&conn)?;
    Ok((ws_id, conn))
}

fn handle_list(ctx: &CommandContext) -> Result<CommandOutput> {
    let statuses = LlmRouter::providers();

    // Try to read workspace selection (non-fatal if DB not initialised yet).
    let workspace_info: Option<(String, Option<LlmSelection>, f64)> = (|| -> Result<_> {
        let (ws_id, conn) = workspace_conn(ctx)?;
        let sel = LlmRouter::get_workspace_selection(&conn, &ws_id)?;
        let credits = workspaces_repo::credit_balance(&conn, &ws_id)?;
        Ok((ws_id, sel, credits))
    })()
    .ok();

    let mut lines = vec![
        format!(
            "{:<16} {:<22} {:<8} {:<8} {:<28} {}",
            "PROVIDER", "NAME", "MODELS", "AUTH", "DEFAULT MODEL", "SELECTED"
        ),
        format!(
            "{:<16} {:<22} {:<8} {:<8} {:<28} {}",
            "--------", "----", "------", "----", "-------------", "--------"
        ),
    ];

    for s in &statuses {
        let auth_icon = if s.authenticated { "yes" } else { "no" };
        let selected = if let Some((_, Some(sel), _)) = &workspace_info {
            if sel.kind == s.kind {
                "◀ workspace default"
            } else {
                ""
            }
        } else {
            ""
        };
        lines.push(format!(
            "{:<16} {:<22} {:<8} {:<8} {:<28} {}",
            s.kind.id(),
            s.name,
            s.model_count,
            auth_icon,
            s.default_model,
            selected,
        ));
    }

    if let Some((ref ws_id, ref sel, credits)) = workspace_info {
        lines.push(String::new());
        lines.push(format!("Workspace: {ws_id}"));
        if let Some(s) = sel {
            let model = s.model.as_deref().unwrap_or("(provider default)");
            let mode = s.auth_mode.as_str();
            lines.push(format!("Selected:  {} / {} [{}]", s.kind.id(), model, mode));
        } else {
            lines.push(
                "Selected:  (none — set with: grove llm select <provider> <model>)".to_string(),
            );
        }
        lines.push(format!("Credits:   ${credits:.4}"));
    }

    lines.push(String::new());
    lines.push("Run 'grove llm models <provider>' to list a provider's models.".to_string());
    lines.push("Run 'grove llm select <provider> <model>' to set workspace default.".to_string());
    lines.push("Run 'grove auth set <provider> <key>' to configure credentials.".to_string());

    let json_providers: Vec<_> = statuses
        .iter()
        .map(|s| {
            let selected = workspace_info
                .as_ref()
                .and_then(|(_, sel, _)| sel.as_ref())
                .map(|sel| sel.kind == s.kind)
                .unwrap_or(false);
            json!({
                "provider": s.kind.id(),
                "name": s.name,
                "authenticated": s.authenticated,
                "model_count": s.model_count,
                "default_model": s.default_model,
                "workspace_selected": selected,
            })
        })
        .collect();

    let credits = workspace_info.as_ref().map(|(_, _, c)| *c).unwrap_or(0.0);
    let ws_selection = workspace_info
        .as_ref()
        .and_then(|(_, sel, _)| sel.as_ref())
        .map(|s| {
            json!({
                "provider": s.kind.id(),
                "model": s.model,
                "auth_mode": s.auth_mode.as_str(),
            })
        });

    Ok(to_text_or_json(
        ctx.format,
        lines.join("\n"),
        json!({
            "providers": json_providers,
            "workspace_credits_usd": credits,
            "workspace_selection": ws_selection,
        }),
    ))
}

fn handle_models(ctx: &CommandContext, provider: &str) -> Result<CommandOutput> {
    let kind = LlmProviderKind::from_str(provider).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown provider '{}'. Valid values: anthropic, openai, deepseek, inception",
            provider
        )
    })?;

    let models = LlmRouter::models(kind);
    if models.is_empty() {
        bail!("no models registered for provider '{}'", kind.id());
    }

    let mut lines = vec![
        format!("Models for {} ({})", kind.display_name(), kind.id()),
        String::new(),
        format!(
            "{:<36} {:<32} {:>10} {:>13} {:>12} {:>9} {}",
            "MODEL ID", "NAME", "CONTEXT", "INPUT $/M", "OUTPUT $/M", "VISION", "TOOLS"
        ),
        format!(
            "{:<36} {:<32} {:>10} {:>13} {:>12} {:>9} {}",
            "--------", "----", "-------", "---------", "----------", "------", "-----"
        ),
    ];

    for m in models {
        lines.push(format!(
            "{:<36} {:<32} {:>10} {:>13} {:>12} {:>9} {}",
            m.id,
            m.name,
            format!("{}k", m.context_window / 1_000),
            format!("${:.2}", m.cost_input_per_m),
            format!("${:.2}", m.cost_output_per_m),
            if m.capabilities.vision { "yes" } else { "no" },
            if m.capabilities.tools { "yes" } else { "no" },
        ));
    }

    let json_models: Vec<_> = models
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "context_window": m.context_window,
                "max_output_tokens": m.max_output_tokens,
                "cost_input_per_m": m.cost_input_per_m,
                "cost_output_per_m": m.cost_output_per_m,
                "capabilities": {
                    "vision": m.capabilities.vision,
                    "tools": m.capabilities.tools,
                    "reasoning": m.capabilities.reasoning,
                }
            })
        })
        .collect();

    Ok(to_text_or_json(
        ctx.format,
        lines.join("\n"),
        json!({ "provider": kind.id(), "models": json_models }),
    ))
}

fn handle_select(
    ctx: &CommandContext,
    provider: &str,
    model: Option<&str>,
    workspace_credits: bool,
) -> Result<CommandOutput> {
    let kind = LlmProviderKind::from_str(provider).ok_or_else(|| {
        anyhow::anyhow!(
            "unknown provider '{}'. Valid values: anthropic, openai, deepseek, inception",
            provider
        )
    })?;

    // Validate model if provided.
    if let Some(m) = model {
        let models = LlmRouter::models(kind);
        if !models.iter().any(|md| md.id == m) {
            bail!(
                "model '{}' not found for provider '{}'. Run 'grove llm models {}' to list available models.",
                m,
                kind.id(),
                kind.id()
            );
        }
    }

    let auth_mode = if workspace_credits {
        LlmAuthMode::WorkspaceCredits
    } else {
        LlmAuthMode::UserKey
    };

    let selection = LlmSelection {
        kind,
        model: model.map(str::to_string),
        auth_mode,
    };

    let (ws_id, conn) = workspace_conn(ctx)?;
    LlmRouter::set_workspace_selection(&conn, &ws_id, &selection)?;

    let model_display = model.unwrap_or("(provider default)");
    let text = format!(
        "Workspace LLM selection updated.\n  Provider: {} ({})\n  Model:    {}\n  Auth:     {}\n\nRun 'grove run \"...\"' to use this provider.",
        kind.display_name(),
        kind.id(),
        model_display,
        auth_mode.as_str(),
    );

    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({
            "workspace_id": ws_id,
            "provider": kind.id(),
            "model": model,
            "auth_mode": auth_mode.as_str(),
        }),
    ))
}

fn handle_credits_balance(ctx: &CommandContext) -> Result<CommandOutput> {
    let (ws_id, conn) = workspace_conn(ctx)?;
    let balance = workspaces_repo::credit_balance(&conn, &ws_id)?;

    let text = format!("Workspace: {ws_id}\nCredit balance: ${balance:.4} USD");
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({ "workspace_id": ws_id, "credits_usd": balance }),
    ))
}

fn handle_credits_add(ctx: &CommandContext, amount_usd: f64) -> Result<CommandOutput> {
    if amount_usd <= 0.0 {
        bail!("amount_usd must be positive (got {amount_usd})");
    }

    let (ws_id, conn) = workspace_conn(ctx)?;
    let new_balance = workspaces_repo::add_credits(&conn, &ws_id, amount_usd)?;

    let text =
        format!("Added ${amount_usd:.4} to workspace {ws_id}.\nNew balance: ${new_balance:.4} USD");
    Ok(to_text_or_json(
        ctx.format,
        text,
        json!({
            "workspace_id": ws_id,
            "added_usd": amount_usd,
            "new_balance_usd": new_balance,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputFormat;
    use std::path::PathBuf;

    fn ctx() -> CommandContext {
        CommandContext {
            project_root: PathBuf::from("."),
            format: OutputFormat::Text,
            _verbose: false,
            _no_color: false,
        }
    }

    #[test]
    fn list_returns_all_providers() {
        let out = handle_list(&ctx()).unwrap();
        assert!(out.text.contains("anthropic"));
        assert!(out.text.contains("openai"));
        assert!(out.text.contains("deepseek"));
        assert!(out.text.contains("inception"));
        assert_eq!(out.json["providers"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn models_anthropic_returns_rows() {
        let out = handle_models(&ctx(), "anthropic").unwrap();
        assert!(out.text.contains("claude-sonnet-4-6"));
        let arr = out.json["models"].as_array().unwrap();
        assert!(!arr.is_empty());
    }

    #[test]
    fn models_inception_returns_mercury() {
        let out = handle_models(&ctx(), "inception").unwrap();
        assert!(out.text.contains("mercury-2"));
    }

    #[test]
    fn models_unknown_provider_errors() {
        let result = handle_models(&ctx(), "banana");
        assert!(result.is_err());
    }
}
