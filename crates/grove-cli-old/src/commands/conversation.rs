use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::{ConversationAction, ConversationArgs};
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &ConversationArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    match &args.action {
        ConversationAction::List(a) => handle_list(ctx, a.limit),
        ConversationAction::Show(a) => handle_show(ctx, &a.id, a.limit),
        ConversationAction::Archive(a) => handle_archive(ctx, &a.id),
        ConversationAction::Delete(a) => handle_delete(ctx, &a.id),
        ConversationAction::Rebase(a) => handle_rebase(ctx, &a.id),
        ConversationAction::Merge(a) => handle_merge(ctx, &a.id),
    }
}

fn handle_list(ctx: &CommandContext, limit: i64) -> Result<CommandOutput> {
    let conversations = orchestrator::list_conversations(&ctx.project_root, limit)?;

    if conversations.is_empty() {
        let text = "No conversations for this project.".to_string();
        let json_val = json!({ "conversations": [] });
        return Ok(to_text_or_json(ctx.format, text, json_val));
    }

    let mut lines = Vec::new();
    lines.push(format!("{} conversation(s):", conversations.len()));
    for c in &conversations {
        let title = c.title.as_deref().unwrap_or("(untitled)");
        lines.push(format!(
            "  {}  {}  {}  {}",
            c.id, title, c.state, c.updated_at,
        ));
    }

    let json_val = json!({
        "conversations": conversations.iter().map(|c| json!({
            "id": c.id,
            "project_id": c.project_id,
            "title": c.title,
            "state": c.state,
            "created_at": c.created_at,
            "updated_at": c.updated_at,
        })).collect::<Vec<_>>(),
    });

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json_val))
}

fn handle_show(ctx: &CommandContext, id: &str, msg_limit: i64) -> Result<CommandOutput> {
    let conv = orchestrator::get_conversation(&ctx.project_root, id)?;
    let messages = orchestrator::list_conversation_messages(&ctx.project_root, id, msg_limit)?;

    let mut lines = Vec::new();
    lines.push(format!("Conversation: {}", conv.id));
    lines.push(format!(
        "  title:      {}",
        conv.title.as_deref().unwrap_or("(untitled)")
    ));
    lines.push(format!("  state:      {}", conv.state));
    lines.push(format!("  created_at: {}", conv.created_at));
    lines.push(format!("  updated_at: {}", conv.updated_at));
    lines.push(String::new());

    if messages.is_empty() {
        lines.push("No messages.".to_string());
    } else {
        lines.push(format!("{} message(s):", messages.len()));
        for m in &messages {
            let agent_info = m.agent_type.as_deref().unwrap_or("");
            let content_preview: String = m.content.chars().take(120).collect();
            let ellipsis = if m.content.len() > 120 { "..." } else { "" };
            lines.push(format!(
                "  [{}] {} {}: {}{}",
                m.created_at, m.role, agent_info, content_preview, ellipsis,
            ));
        }
    }

    let messages_json: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "role": m.role,
                "agent_type": m.agent_type,
                "session_id": m.session_id,
                "content": m.content,
                "created_at": m.created_at,
            })
        })
        .collect();

    let json_val = json!({
        "conversation": {
            "id": conv.id,
            "project_id": conv.project_id,
            "title": conv.title,
            "state": conv.state,
            "created_at": conv.created_at,
            "updated_at": conv.updated_at,
        },
        "messages": messages_json,
    });

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json_val))
}

fn handle_archive(ctx: &CommandContext, id: &str) -> Result<CommandOutput> {
    orchestrator::archive_conversation(&ctx.project_root, id)?;
    let text = format!("Conversation {id} archived.");
    let json_val = json!({ "id": id, "state": "archived" });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_delete(ctx: &CommandContext, id: &str) -> Result<CommandOutput> {
    orchestrator::delete_conversation(&ctx.project_root, id)?;
    let text = format!("Conversation {id} deleted (messages removed).");
    let json_val = json!({ "id": id, "deleted": true });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_rebase(ctx: &CommandContext, id: &str) -> Result<CommandOutput> {
    let msg = orchestrator::rebase_conversation(&ctx.project_root, id)?;
    let json_val = json!({ "conversation_id": id, "result": "success", "message": msg });
    Ok(to_text_or_json(ctx.format, msg, json_val))
}

fn handle_merge(ctx: &CommandContext, id: &str) -> Result<CommandOutput> {
    let result = orchestrator::merge_conversation(&ctx.project_root, id)?;

    let text = match result.outcome.as_str() {
        "merged" => format!(
            "Merged {} into {} (direct).",
            result.source_branch, result.target_branch,
        ),
        "up_to_date" => format!(
            "Branch {} is already up to date with {}.",
            result.source_branch, result.target_branch,
        ),
        "conflict" => format!(
            "Merge conflict: {} file(s) conflicting: {}",
            result.conflicting_files.len(),
            result.conflicting_files.join(", "),
        ),
        "pr_opened" => format!(
            "PR opened: {}",
            result.pr_url.as_deref().unwrap_or("(unknown URL)"),
        ),
        "pr_exists" => format!(
            "PR already exists for branch {}. Changes were pushed.",
            result.source_branch,
        ),
        other => format!("Merge outcome: {other}"),
    };

    let json_val = json!({
        "conversation_id": result.conversation_id,
        "source_branch": result.source_branch,
        "target_branch": result.target_branch,
        "strategy": result.strategy,
        "outcome": result.outcome,
        "pr_url": result.pr_url,
        "conflicting_files": result.conflicting_files,
    });
    Ok(to_text_or_json(ctx.format, text, json_val))
}
