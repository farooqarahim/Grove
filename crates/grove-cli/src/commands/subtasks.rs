use anyhow::{Result, anyhow};
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::SubtasksArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &SubtasksArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    let conn = grove_core::db::DbHandle::new(&ctx.project_root).connect()?;

    // Resolve run_id: use the argument or fall back to the most recent run with sub-tasks.
    let run_id: String = if let Some(ref id) = args.run_id {
        id.clone()
    } else {
        conn.query_row(
            "SELECT run_id FROM subtasks ORDER BY created_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| {
            anyhow!("no sub-tasks found — run a large objective first to generate sub-tasks")
        })?
    };

    // Fetch run metadata.
    let (run_objective, run_state): (String, String) = conn
        .query_row(
            "SELECT objective, state FROM runs WHERE id=?1",
            [&run_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| anyhow!("run '{}' not found", run_id))?;

    let subtasks = orchestrator::list_subtasks(&ctx.project_root, &run_id)?;

    if subtasks.is_empty() {
        let text =
            format!("no sub-tasks found for run {run_id} (architect used single-builder mode)");
        return Ok(to_text_or_json(
            ctx.format,
            text,
            json!({ "run_id": run_id, "run_status": run_state, "objective": run_objective, "subtasks": [] }),
        ));
    }

    // Count by status.
    let completed = subtasks.iter().filter(|s| s.status == "completed").count();
    let in_progress = subtasks
        .iter()
        .filter(|s| s.status == "in_progress")
        .count();
    let pending = subtasks.iter().filter(|s| s.status == "pending").count();
    let failed = subtasks.iter().filter(|s| s.status == "failed").count();
    let total = subtasks.len();

    // ── Text output ───────────────────────────────────────────────────────────
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Run: {run_id}"));
    lines.push(format!("Objective: {run_objective}"));
    lines.push(format!("Status: {run_state}"));
    lines.push(String::new());

    let mut summary_parts: Vec<String> = Vec::new();
    if completed > 0 {
        summary_parts.push(format!("{completed} done"));
    }
    if in_progress > 0 {
        summary_parts.push(format!("{in_progress} active"));
    }
    if pending > 0 {
        summary_parts.push(format!("{pending} pending"));
    }
    if failed > 0 {
        summary_parts.push(format!("{failed} failed"));
    }
    lines.push(format!(
        "Sub-tasks ({total} total — {}):",
        summary_parts.join(", ")
    ));
    lines.push(String::new());

    for subtask in &subtasks {
        let status_sym = match subtask.status.as_str() {
            "completed" => "✓",
            "in_progress" => "●",
            "failed" => "✗",
            _ => "○", // pending / skipped
        };

        let files_display = if subtask.files_hint.is_empty() {
            String::new()
        } else {
            format!("  {}", subtask.files_hint.join(", "))
        };

        // Extract short task id for display (strip "sub_{run_id}_" prefix).
        let display_id = subtask
            .id
            .strip_prefix(&format!("sub_{}_", run_id))
            .unwrap_or(&subtask.id);

        lines.push(format!(
            "  {status_sym}  [{display_id}] {title:<26} {status:<12}{files_display}",
            title = subtask.title,
            status = subtask.status,
        ));

        // Todos
        for (idx, todo) in subtask.todos.iter().enumerate() {
            let todo_sym = match subtask.status.as_str() {
                "completed" => "✓",
                "in_progress" => {
                    if idx == 0 {
                        "●"
                    } else {
                        "○"
                    }
                }
                _ => "○",
            };
            lines.push(format!("     {todo_sym} {todo}"));
        }

        // Result summary
        if let Some(ref summary) = subtask.result_summary {
            let preview: String = summary.chars().take(120).collect();
            let ellipsis = if summary.len() > 120 { "…" } else { "" };
            lines.push(format!("     Result: {preview}{ellipsis}"));
        }

        lines.push(String::new());
    }

    let text = lines.join("\n");

    // ── JSON output ───────────────────────────────────────────────────────────
    let subtasks_json: Vec<serde_json::Value> = subtasks
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "run_id": s.run_id,
                "session_id": s.session_id,
                "title": s.title,
                "description": s.description,
                "status": s.status,
                "priority": s.priority,
                "depends_on": s.depends_on,
                "assigned_agent": s.assigned_agent,
                "files_hint": s.files_hint,
                "todos": s.todos,
                "result_summary": s.result_summary,
                "created_at": s.created_at,
                "updated_at": s.updated_at,
            })
        })
        .collect();

    let json_val = json!({
        "run_id": run_id,
        "run_status": run_state,
        "objective": run_objective,
        "subtasks": subtasks_json,
    });

    Ok(to_text_or_json(ctx.format, text, json_val))
}
