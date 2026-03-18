use anyhow::Result;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::LogsArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &LogsArgs) -> Result<CommandOutput> {
    let events = if args.all {
        orchestrator::run_events_all(&ctx.project_root, &args.run_id)?
    } else {
        orchestrator::run_events(&ctx.project_root, &args.run_id)?
    };

    let json = json!({
      "run_id": args.run_id,
      "events": events
    });

    let mut text = format!("Logs\nrun_id: {}\nevents: {}", args.run_id, events.len());
    for event in events {
        text.push_str(&format!(
            "\n- {} {} {}",
            event.created_at, event.event_type, event.payload
        ));
    }

    Ok(to_text_or_json(ctx.format, text, json))
}
