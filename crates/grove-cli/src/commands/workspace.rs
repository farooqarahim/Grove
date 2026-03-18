use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::{WorkspaceAction, WorkspaceArgs};
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &WorkspaceArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    match &args.action {
        WorkspaceAction::Show => handle_show(ctx),
        WorkspaceAction::SetName(a) => handle_set_name(ctx, &a.name),
        WorkspaceAction::Archive(a) => handle_archive(ctx, &a.id),
        WorkspaceAction::Delete(a) => handle_delete(ctx, &a.id),
    }
}

fn handle_show(ctx: &CommandContext) -> Result<CommandOutput> {
    let row = orchestrator::get_workspace(&ctx.project_root)?;

    let text = format!(
        "Workspace\n  id:         {}\n  name:       {}\n  state:      {}\n  created_at: {}\n  updated_at: {}",
        row.id,
        row.name.as_deref().unwrap_or("(none)"),
        row.state,
        row.created_at,
        row.updated_at,
    );

    let json_val = json!({
        "id": row.id,
        "name": row.name,
        "state": row.state,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    });

    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_set_name(ctx: &CommandContext, name: &str) -> Result<CommandOutput> {
    orchestrator::update_workspace_name(&ctx.project_root, name)?;
    let text = format!("Workspace name set to: {name}");
    let json_val = json!({ "name": name, "updated": true });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_archive(ctx: &CommandContext, id: &str) -> Result<CommandOutput> {
    orchestrator::archive_workspace(&ctx.project_root, id)?;
    let text = format!("Workspace {id} archived.");
    let json_val = json!({ "id": id, "state": "archived" });
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_delete(ctx: &CommandContext, id: &str) -> Result<CommandOutput> {
    orchestrator::delete_workspace(&ctx.project_root, id)?;
    let text = format!("Workspace {id} deleted.");
    let json_val = json!({ "id": id, "deleted": true });
    Ok(to_text_or_json(ctx.format, text, json_val))
}
