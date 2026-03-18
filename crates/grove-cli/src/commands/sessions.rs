use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::SessionsArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &SessionsArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;
    let sessions = orchestrator::list_sessions(&ctx.project_root, &args.run_id)?;

    let json_rows: Vec<_> = sessions
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "agent_type": s.agent_type.as_str(),
                "state": s.state.as_str(),
                "worktree_path": s.worktree_path,
                "started_at": s.started_at,
                "ended_at": s.ended_at,
            })
        })
        .collect();

    let json = json!({ "run_id": args.run_id, "sessions": json_rows });

    if sessions.is_empty() {
        return Ok(to_text_or_json(
            ctx.format,
            format!("No sessions found for run {}.", args.run_id),
            json,
        ));
    }

    let mut lines = vec![format!("Sessions for run {}\n", args.run_id)];
    for s in &sessions {
        let started = s.started_at.as_deref().unwrap_or("-");
        let ended = s.ended_at.as_deref().unwrap_or("-");
        lines.push(format!(
            "  {:9}  {:12}  {}  started: {}  ended: {}",
            s.state.as_str(),
            s.agent_type.as_str(),
            s.id,
            started,
            ended,
        ));
        lines.push(format!("    worktree: {}", s.worktree_path));
    }

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json))
}
