use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::MergeStatusArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &MergeStatusArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;
    let entries = orchestrator::list_merge_queue(&ctx.project_root, &args.conversation_id)?;

    let json_rows: Vec<_> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "conversation_id": e.conversation_id,
                "branch_name": e.branch_name,
                "target_branch": e.target_branch,
                "status": e.status,
                "strategy": e.strategy,
                "pr_url": e.pr_url,
                "error": e.error,
                "created_at": e.created_at,
                "updated_at": e.updated_at,
            })
        })
        .collect();

    let json = json!({ "conversation_id": args.conversation_id, "entries": json_rows });

    if entries.is_empty() {
        return Ok(to_text_or_json(
            ctx.format,
            format!(
                "No merge-queue entries for conversation {}.",
                args.conversation_id
            ),
            json,
        ));
    }

    let mut lines = vec![format!(
        "Merge queue for conversation {}\n",
        args.conversation_id
    )];
    for e in &entries {
        let err = e
            .error
            .as_deref()
            .map(|s| format!("  error: {s}"))
            .unwrap_or_default();
        let pr = e
            .pr_url
            .as_deref()
            .map(|s| format!("  pr: {s}"))
            .unwrap_or_default();
        lines.push(format!(
            "  id={:<4}  {:8}  branch: {}  target: {}  strategy: {}{}{}",
            e.id, e.status, e.branch_name, e.target_branch, e.strategy, pr, err,
        ));
        lines.push(format!(
            "    created: {}  updated: {}",
            e.created_at, e.updated_at,
        ));
    }

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json))
}
