use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::TaskCancelArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &TaskCancelArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;
    orchestrator::cancel_task(&ctx.project_root, &args.task_id)?;

    let text = format!("Task {} cancelled.", args.task_id);
    let json = json!({ "task_id": args.task_id, "state": "cancelled" });
    Ok(to_text_or_json(ctx.format, text, json))
}
