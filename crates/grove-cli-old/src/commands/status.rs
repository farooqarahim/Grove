use anyhow::Result;
use grove_core::capability;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::StatusArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &StatusArgs) -> Result<CommandOutput> {
    let runs = orchestrator::list_runs(&ctx.project_root, args.limit)?;

    // Capability detection
    let cfg = grove_core::config::GroveConfig::load_or_create(&ctx.project_root)?;
    let cap_report = capability::detect_capabilities(&cfg, &ctx.project_root);

    let checks_json: Vec<serde_json::Value> = cap_report
        .checks
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "available": c.available,
                "message": c.message,
            })
        })
        .collect();

    let json = json!({
        "runs": runs,
        "capability": {
            "level": cap_report.level.as_str(),
            "checks": checks_json,
        },
    });

    let mut text = format!(
        "Status (capability: {})\nruns: {}",
        cap_report.level.as_str(),
        runs.len()
    );

    for check in &cap_report.checks {
        if !check.available {
            text.push_str(&format!("\n  [!] {}: {}", check.name, check.message));
        }
    }
    for run in runs {
        let conv_info = run
            .conversation_id
            .as_deref()
            .map(|c| format!(" conv={c}"))
            .unwrap_or_default();
        text.push_str(&format!(
            "\n- {} [{}] budget=${:.2} used=${:.2}{conv_info}",
            run.id, run.state, run.budget_usd, run.cost_used_usd
        ));
    }

    Ok(to_text_or_json(ctx.format, text, json))
}
