use anyhow::Result;
use grove_core::config::HookEvent;
use grove_core::hooks;
use serde_json::json;

use crate::cli::HookArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &HookArgs) -> Result<CommandOutput> {
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;

    let event = parse_event(&args.event)
        .ok_or_else(|| anyhow::anyhow!("unknown hook event: {}", args.event))?;

    // Guard checks for pre_tool_use
    if event == HookEvent::PreToolUse {
        // Check file guard if file_path is provided
        if let Some(ref file_path) = args.file_path {
            let allowed = hooks::check_file_guard(&cfg.hooks.guards, &args.agent_type, file_path);
            if !allowed {
                let json = json!({
                    "allowed": false,
                    "reason": format!("file path '{}' is blocked for agent type '{}'", file_path, args.agent_type)
                });
                let text = serde_json::to_string_pretty(&json)?;
                return Ok(to_text_or_json(ctx.format, text, json));
            }
        }

        // Check tool guard if tool is provided
        if let Some(ref tool) = args.tool {
            let allowed = hooks::check_tool_guard(&cfg.hooks.guards, &args.agent_type, tool);
            if !allowed {
                let json = json!({
                    "allowed": false,
                    "reason": format!("tool '{}' is blocked for agent type '{}'", tool, args.agent_type)
                });
                let text = serde_json::to_string_pretty(&json)?;
                return Ok(to_text_or_json(ctx.format, text, json));
            }
        }
    }

    // Run lifecycle hooks
    let hook_ctx = hooks::HookContext {
        run_id: args.run_id.clone().unwrap_or_default(),
        session_id: args.session_id.clone(),
        agent_type: Some(args.agent_type.clone()),
        worktree_path: args.worktree.clone(),
        event,
    };

    match hooks::run_hooks(&cfg.hooks, event, &hook_ctx, &ctx.project_root) {
        Ok(()) => {
            let json = json!({ "allowed": true });
            let text = serde_json::to_string_pretty(&json)?;
            Ok(to_text_or_json(ctx.format, text, json))
        }
        Err(e) => {
            let json = json!({
                "allowed": false,
                "reason": e.to_string()
            });
            let text = serde_json::to_string_pretty(&json)?;
            Ok(to_text_or_json(ctx.format, text, json))
        }
    }
}

fn parse_event(s: &str) -> Option<HookEvent> {
    match s {
        "session_start" => Some(HookEvent::SessionStart),
        "user_prompt_submit" => Some(HookEvent::UserPromptSubmit),
        "pre_tool_use" => Some(HookEvent::PreToolUse),
        "post_tool_use" => Some(HookEvent::PostToolUse),
        "stop" => Some(HookEvent::Stop),
        "pre_compact" => Some(HookEvent::PreCompact),
        "post_run" => Some(HookEvent::PostRun),
        _ => None,
    }
}
