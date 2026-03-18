use anyhow::Result;
use grove_core::config::GroveConfig;
use grove_core::db;
use grove_core::orchestrator::{self, RunOptions};
use grove_core::tracker::ci::{CiOverall, failing_checks_to_objective, get_ci_status, wait_for_ci};
use serde_json::json;

use crate::cli::CiArgs;
use crate::command_context::CommandContext;
use crate::commands::queue::provider_from_config;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &CiArgs) -> Result<CommandOutput> {
    let cfg = GroveConfig::load_or_create(&ctx.project_root)?;

    let branch = match &args.branch {
        Some(b) => b.clone(),
        None => detect_current_branch(&ctx.project_root)?,
    };

    eprintln!("[ci] Checking CI status for branch '{branch}'…");

    let status = if args.wait {
        eprintln!(
            "[ci] Waiting up to {}s for checks to complete…",
            args.timeout
        );
        wait_for_ci(&ctx.project_root, &branch, args.timeout).map_err(|e| anyhow::anyhow!("{e}"))?
    } else {
        get_ci_status(&ctx.project_root, &branch).map_err(|e| anyhow::anyhow!("{e}"))?
    };

    if args.fix && status.overall == CiOverall::Failing {
        eprintln!("[ci] CI is failing — spawning agent to fix…");
        db::initialize(&ctx.project_root)?;

        let objective = failing_checks_to_objective(&status);
        let provider = provider_from_config(&cfg, &ctx.project_root, None, None)?;
        let run_result = orchestrator::execute_objective(
            &ctx.project_root,
            &cfg,
            &objective,
            RunOptions {
                budget_usd: args.budget_usd,
                max_agents: None,
                model: args.model.clone(),
                provider: None,
                interactive: false,
                pause_after: vec![],
                permission_mode: None,
                pipeline: None,
                conversation_id: None,
                continue_last: false,
                db_path: None,
                abort_handle: None,
                issue_id: None,
                issue: None,
                resume_provider_session_id: None,
                on_run_created: None,
                input_handle_callback: None,
                run_control_callback: None,
                disable_phase_gates: false,
            },
            provider,
        )?;

        let json_val = json!({
            "branch": branch,
            "overall": status.overall.as_str(),
            "checks_count": status.checks.len(),
            "fix_run_id": run_result.run_id,
            "fix_state": run_result.state,
        });

        let text = format!(
            "CI: {} ({})\nFix run: {} ({})",
            status.overall.as_str(),
            branch,
            run_result.run_id,
            run_result.state,
        );
        return Ok(to_text_or_json(ctx.format, text, json_val));
    }

    let checks_json: Vec<_> = status
        .checks
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "status": c.status,
                "conclusion": c.conclusion,
                "url": c.url,
            })
        })
        .collect();

    let json_val = json!({
        "branch": branch,
        "overall": status.overall.as_str(),
        "checks": checks_json,
    });

    let mut text = format!(
        "CI status for '{}': {}\n\n",
        branch,
        status.overall.as_str()
    );
    if status.checks.is_empty() {
        text.push_str("  No checks found.\n");
    } else {
        for check in &status.checks {
            let conclusion = check.conclusion.as_deref().unwrap_or("—");
            text.push_str(&format!(
                "  {} — {} ({})\n",
                check.name, check.status, conclusion,
            ));
        }
    }

    if status.overall == CiOverall::Failing {
        text.push_str("\nUse --fix to spawn an agent to fix failing checks.");
    }

    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn detect_current_branch(project_root: &std::path::Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("failed to detect current git branch");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
