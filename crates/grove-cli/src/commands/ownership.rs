use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::OwnershipArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &OwnershipArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;
    let locks = orchestrator::list_ownership_locks(&ctx.project_root, args.run_id.as_deref())?;

    let json_rows: Vec<_> = locks
        .iter()
        .map(|l| {
            json!({
                "id": l.id,
                "path": l.path,
                "owner_session_id": l.owner_session_id,
                "run_id": l.run_id,
                "created_at": l.created_at,
            })
        })
        .collect();

    let json = json!({
        "run_id": args.run_id,
        "locks": json_rows,
        "total": locks.len(),
    });

    if locks.is_empty() {
        let msg = match &args.run_id {
            Some(rid) => format!("No ownership locks for run {rid}."),
            None => "No ownership locks currently held.".to_string(),
        };
        return Ok(to_text_or_json(ctx.format, msg, json));
    }

    let header = match &args.run_id {
        Some(rid) => format!("Ownership locks for run {rid}\n"),
        None => format!("All ownership locks ({} total)\n", locks.len()),
    };
    let mut lines = vec![header];
    for l in &locks {
        // Truncate long IDs at 36 chars so columns stay readable in a terminal.
        let run = &l.run_id;
        let sess = &l.owner_session_id;
        lines.push(format!("  run: {run}  session: {sess}"));
        // Quote the path so spaces don't confuse downstream parsers.
        lines.push(format!("    path: {:?}  created: {}", l.path, l.created_at));
    }

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json))
}
