use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::TasksArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &TasksArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    // Reconcile stale tasks when --refresh is passed.
    if args.refresh {
        let reconciled = orchestrator::reconcile_stale_tasks(&ctx.project_root)?;
        if reconciled > 0 {
            eprintln!("[TASKS] Reconciled {reconciled} stale running task(s) → failed");
        } else {
            eprintln!("[TASKS] No stale tasks found");
        }
    }

    let tasks = orchestrator::list_tasks(&ctx.project_root)?;

    let json_tasks: Vec<_> = tasks
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "objective": t.objective,
                "state": t.state,
                "priority": t.priority,
                "run_id": t.run_id,
                "queued_at": t.queued_at,
                "started_at": t.started_at,
                "completed_at": t.completed_at,
                "model": t.model,
                "conversation_id": t.conversation_id,
            })
        })
        .collect();

    let json = json!({ "tasks": json_tasks, "total": tasks.len() });

    if tasks.is_empty() {
        let text = "No tasks in queue.".to_string();
        return Ok(to_text_or_json(ctx.format, text, json));
    }

    let mut lines = vec![format!("{} task(s) in queue:\n", tasks.len())];
    for t in &tasks {
        let icon = match t.state.as_str() {
            "queued" => "⏳",
            "running" => "▶",
            "completed" => "✓",
            "failed" => "✗",
            "cancelled" => "⊘",
            _ => "?",
        };
        let run_info = t
            .run_id
            .as_deref()
            .map(|r| format!(" (run: {r})"))
            .unwrap_or_default();
        let conv_info = t
            .conversation_id
            .as_deref()
            .map(|c| format!(" conv={c}"))
            .unwrap_or_default();
        lines.push(format!(
            "  {icon} [{}] pri={} {}{run_info}{conv_info}",
            t.state, t.priority, t.objective
        ));
    }

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json))
}
