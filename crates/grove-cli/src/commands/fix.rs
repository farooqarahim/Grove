use std::sync::Arc;

use anyhow::Result;
use grove_core::config::GroveConfig;
use grove_core::db;
use grove_core::orchestrator::issue_context::enrich_objective;
use grove_core::orchestrator::{self, RunOptions};
use grove_core::tracker::registry::TrackerRegistry;
use serde_json::json;

use crate::cli::FixArgs;
use crate::command_context::CommandContext;
use crate::commands::queue::provider_from_config;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &FixArgs) -> Result<CommandOutput> {
    let cfg = GroveConfig::load_or_create(&ctx.project_root)?;
    db::initialize(&ctx.project_root)?;

    let registry = TrackerRegistry::from_config(&cfg, &ctx.project_root);
    if !registry.is_active() {
        anyhow::bail!(
            "no tracker providers enabled — run `grove connect github|jira|linear` first, \
             or set tracker.mode in grove.yaml"
        );
    }

    if args.ready {
        handle_ready_batch(ctx, args, &cfg, &registry)
    } else if let Some(ref issue_id) = args.issue_id {
        handle_single_issue(ctx, args, &cfg, &registry, issue_id)
    } else {
        anyhow::bail!("provide an issue ID or use --ready to fix all ready issues")
    }
}

fn handle_single_issue(
    ctx: &CommandContext,
    args: &FixArgs,
    cfg: &GroveConfig,
    registry: &TrackerRegistry,
    issue_id: &str,
) -> Result<CommandOutput> {
    let issue = registry
        .find_issue(issue_id)?
        .ok_or_else(|| anyhow::anyhow!("issue '{issue_id}' not found in any connected tracker"))?;

    eprintln!(
        "[fix] Found issue #{} [{}]: {}",
        issue.external_id, issue.provider, issue.title
    );

    let user_prompt = args.prompt.as_deref().unwrap_or("");
    let objective = enrich_objective(&issue, user_prompt);

    let provider = provider_from_config(cfg, &ctx.project_root, None, None)?;
    let result = orchestrator::execute_objective(
        &ctx.project_root,
        cfg,
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
            issue_id: Some(issue.external_id.clone()),
            issue: Some(issue),
            resume_provider_session_id: None,
            on_run_created: None,
            input_handle_callback: None,
            run_control_callback: None,
            disable_phase_gates: false,
        },
        provider,
    )?;

    let json_val = json!({
        "run_id": result.run_id,
        "state": result.state,
        "issue_id": issue_id,
        "objective": result.objective,
    });

    let text = format!(
        "Fix complete\nrun_id: {}\nstate: {}\nissue: #{issue_id}",
        result.run_id, result.state,
    );

    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn handle_ready_batch(
    ctx: &CommandContext,
    args: &FixArgs,
    cfg: &GroveConfig,
    registry: &TrackerRegistry,
) -> Result<CommandOutput> {
    let all_ready = registry.list_all_ready()?;
    if all_ready.is_empty() {
        return Ok(CommandOutput {
            text: "No issues marked as ready found.".into(),
            json: json!({ "ready_count": 0, "results": [] }),
        });
    }

    let issues: Vec<_> = if let Some(max) = args.max {
        all_ready.into_iter().take(max).collect()
    } else {
        all_ready
    };

    eprintln!("[fix] Found {} ready issue(s)", issues.len());

    if args.parallel {
        queue_as_tasks(ctx, cfg, &issues, args)?;
    } else {
        return run_sequentially(ctx, cfg, &issues, args);
    }

    let json_val = json!({
        "ready_count": issues.len(),
        "mode": if args.parallel { "queued" } else { "sequential" },
    });
    let text = format!("Queued {} ready issue(s) as tasks", issues.len());
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn run_sequentially(
    ctx: &CommandContext,
    cfg: &GroveConfig,
    issues: &[grove_core::tracker::Issue],
    args: &FixArgs,
) -> Result<CommandOutput> {
    let provider = provider_from_config(cfg, &ctx.project_root, None, None)?;
    let mut results = Vec::new();

    for issue in issues {
        eprintln!(
            "\n[fix] Starting fix for #{} [{}]: {}",
            issue.external_id, issue.provider, issue.title
        );

        let user_prompt = args.prompt.as_deref().unwrap_or("");
        let objective = enrich_objective(issue, user_prompt);

        let result = orchestrator::execute_objective(
            &ctx.project_root,
            cfg,
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
                issue_id: Some(issue.external_id.clone()),
                issue: Some(issue.clone()),
                resume_provider_session_id: None,
                on_run_created: None,
                input_handle_callback: None,
                run_control_callback: None,
                disable_phase_gates: false,
            },
            Arc::clone(&provider),
        );

        match result {
            Ok(r) => {
                eprintln!("[fix] #{} completed (run {})", issue.external_id, r.run_id);
                results.push(json!({
                    "issue_id": issue.external_id,
                    "run_id": r.run_id,
                    "state": r.state,
                }));
            }
            Err(e) => {
                eprintln!("[fix] #{} failed: {e}", issue.external_id);
                results.push(json!({
                    "issue_id": issue.external_id,
                    "state": "failed",
                    "error": e.to_string(),
                }));
            }
        }
    }

    let json_val = json!({
        "ready_count": issues.len(),
        "results": results,
    });
    let text = format!("Fixed {} issue(s)", issues.len());
    Ok(to_text_or_json(ctx.format, text, json_val))
}

fn queue_as_tasks(
    ctx: &CommandContext,
    cfg: &GroveConfig,
    issues: &[grove_core::tracker::Issue],
    args: &FixArgs,
) -> Result<()> {
    db::initialize(&ctx.project_root)?;
    let _ = cfg; // config already loaded

    for issue in issues {
        let user_prompt = args.prompt.as_deref().unwrap_or("");
        let objective = enrich_objective(issue, user_prompt);

        let task = orchestrator::queue_task(
            &ctx.project_root,
            &objective,
            args.budget_usd,
            0, // default priority
            args.model.as_deref(),
            None,  // no provider override
            None,  // no conversation for batch tasks
            None,  // no session resumption for batch tasks
            None,  // no pipeline override for batch fix
            None,  // no permission_mode override for batch fix
            false, // disable_phase_gates
        )?;
        eprintln!(
            "[fix] Queued task {} for issue #{}",
            task.id, issue.external_id
        );
    }

    // Drain the queue if no run is active
    if !orchestrator::has_active_run(&ctx.project_root)? {
        eprintln!("[fix] No active run — starting queue now…");
        crate::commands::queue::drain_queue(ctx, cfg)?;
    }

    Ok(())
}
