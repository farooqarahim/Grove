use anyhow::{Result, anyhow};
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::PlanArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &PlanArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    let conn = grove_core::db::DbHandle::new(&ctx.project_root).connect()?;

    // Resolve run_id: use arg → most recent run with plan_steps → error.
    let run_id: String = if let Some(ref id) = args.run_id {
        id.clone()
    } else {
        conn.query_row(
            "SELECT run_id FROM plan_steps ORDER BY created_at DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| {
            anyhow!(
                "no structured plan found — run a complex objective first, \
                 or pass a run_id explicitly"
            )
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

    let steps = orchestrator::list_plan_steps(&ctx.project_root, &run_id)?;

    if steps.is_empty() {
        let text = format!("no structured plan for run {run_id} — used single-step execution mode");
        return Ok(to_text_or_json(
            ctx.format,
            text,
            json!({
                "run_id": run_id,
                "run_status": run_state,
                "objective": run_objective,
                "steps": [],
            }),
        ));
    }

    // ── Text output ───────────────────────────────────────────────────────────
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("Run: {run_id}"));
    lines.push(format!("Objective: {run_objective}"));
    lines.push(format!("Status: {run_state}"));
    lines.push(String::new());
    lines.push(format!("Plan ({} step(s)):", steps.len()));
    lines.push(String::new());

    // Group by wave.
    let max_wave = steps.iter().map(|s| s.wave).max().unwrap_or(0);
    for wave in 0..=max_wave {
        let wave_steps: Vec<&grove_core::orchestrator::PlanStep> =
            steps.iter().filter(|s| s.wave == wave).collect();

        if wave_steps.is_empty() {
            continue;
        }

        let wave_header = if wave == 0 {
            format!("Wave {wave}:")
        } else {
            format!("Wave {wave}:  (runs after Wave {})", wave - 1)
        };
        lines.push(wave_header);

        for step in &wave_steps {
            let status_sym = match step.status.as_str() {
                "completed" => "✓",
                "in_progress" => "●",
                "failed" => "✗",
                "skipped" => "⊘",
                _ => "○", // pending
            };

            lines.push(format!(
                "  {status_sym}  [{id}] {agent:<12} \"{title}\"   {status}",
                id = step.id.split('_').last().unwrap_or(&step.id),
                agent = step.agent_type,
                title = step.title,
                status = step.status,
            ));

            for todo in &step.todos {
                let todo_sym = match step.status.as_str() {
                    "completed" => "✓",
                    "in_progress" => "●",
                    _ => "○",
                };
                lines.push(format!("     {todo_sym} {todo}"));
            }

            if let Some(ref summary) = step.result_summary {
                let preview: String = summary.chars().take(120).collect();
                let ellipsis = if summary.len() > 120 { "…" } else { "" };
                lines.push(format!("     Result: {preview}{ellipsis}"));
            }

            lines.push(String::new());
        }
    }

    let text = lines.join("\n");

    // ── JSON output ───────────────────────────────────────────────────────────
    let steps_json: Vec<serde_json::Value> = steps
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "run_id": s.run_id,
                "step_index": s.step_index,
                "wave": s.wave,
                "agent_type": s.agent_type,
                "title": s.title,
                "description": s.description,
                "todos": s.todos,
                "files": s.files,
                "depends_on": s.depends_on,
                "status": s.status,
                "session_id": s.session_id,
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
        "steps": steps_json,
    });

    Ok(to_text_or_json(ctx.format, text, json_val))
}
