use anyhow::Result;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::CostsArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &CostsArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;
    let report = orchestrator::cost_report(&ctx.project_root, args.recent_runs)?;

    // ── Text output ───────────────────────────────────────────────────────────
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!(
        "Total spent: ${:.4}  ({} completed run{})",
        report.total_spent_usd,
        report.total_runs,
        if report.total_runs == 1 { "" } else { "s" }
    ));
    lines.push(String::new());

    if report.by_agent.is_empty() {
        lines.push("No per-agent cost data yet (run at least one real session).".to_string());
    } else {
        lines.push("By agent type:".to_string());
        // Determine column widths for alignment.
        let name_width = report
            .by_agent
            .iter()
            .map(|a| a.agent_type.len())
            .max()
            .unwrap_or(10)
            .max(10);
        lines.push(format!(
            "  {:<name_width$}  {:>10}  {:>8}  {:>12}",
            "agent",
            "total ($)",
            "sessions",
            "avg ($)",
            name_width = name_width
        ));
        lines.push(format!(
            "  {:-<name_width$}  {:-<10}  {:-<8}  {:-<12}",
            "",
            "",
            "",
            "",
            name_width = name_width
        ));
        for a in &report.by_agent {
            lines.push(format!(
                "  {:<name_width$}  {:>10.4}  {:>8}  {:>12.4}",
                a.agent_type,
                a.total_cost_usd,
                a.session_count,
                a.avg_cost_usd,
                name_width = name_width
            ));
        }
    }

    if !report.recent_runs.is_empty() {
        lines.push(String::new());
        lines.push(format!("Recent {} run(s):", report.recent_runs.len()));
        for r in &report.recent_runs {
            let obj_preview: String = r.objective.chars().take(60).collect();
            let ellipsis = if r.objective.len() > 60 { "…" } else { "" };
            lines.push(format!(
                "  {}  ${:.4}  {}{}",
                r.run_id, r.cost_used_usd, obj_preview, ellipsis
            ));
        }
    }

    let text = lines.join("\n");

    // ── JSON output ───────────────────────────────────────────────────────────
    let by_agent_json: Vec<serde_json::Value> = report
        .by_agent
        .iter()
        .map(|a| {
            json!({
                "agent_type": a.agent_type,
                "total_cost_usd": a.total_cost_usd,
                "session_count": a.session_count,
                "avg_cost_usd": a.avg_cost_usd,
            })
        })
        .collect();

    let recent_runs_json: Vec<serde_json::Value> = report
        .recent_runs
        .iter()
        .map(|r| {
            json!({
                "run_id": r.run_id,
                "cost_used_usd": r.cost_used_usd,
                "objective": r.objective,
                "created_at": r.created_at,
            })
        })
        .collect();

    let json_val = json!({
        "total_spent_usd": report.total_spent_usd,
        "total_runs": report.total_runs,
        "by_agent": by_agent_json,
        "recent_runs": recent_runs_json,
    });

    Ok(to_text_or_json(ctx.format, text, json_val))
}
