use anyhow::Result;
use grove_core::db::DbHandle;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::AbortArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &AbortArgs) -> Result<CommandOutput> {
    // Look up the conversation_id for this run before aborting.
    let conv_id: Option<String> = {
        let handle = DbHandle::new(&ctx.project_root);
        handle.connect().ok().and_then(|conn| {
            conn.query_row(
                "SELECT conversation_id FROM runs WHERE id=?1",
                [&args.run_id],
                |r| r.get(0),
            )
            .ok()
        })
    };

    orchestrator::abort_run(&ctx.project_root, &args.run_id)?;

    // Cancel the running task for this conversation so the queue reflects the
    // abort immediately and the next task becomes eligible.
    if let Some(ref cid) = conv_id {
        let cancelled =
            orchestrator::cancel_running_tasks_for_conversation(&ctx.project_root, cid)?;
        if cancelled > 0 {
            eprintln!("[ABORT] Cancelled {cancelled} running task(s) for conversation {cid}");
        }
    }

    let json = json!({
      "run_id": args.run_id,
      "state": "paused",
      "aborted": true
    });

    let text = format!("Run aborted\nrun_id: {}\nstate: paused", args.run_id);

    Ok(to_text_or_json(ctx.format, text, json))
}
