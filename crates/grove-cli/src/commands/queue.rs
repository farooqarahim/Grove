use anyhow::Result;
use grove_core::config::GroveConfig;
use grove_core::db;
use grove_core::orchestrator::{self, RunOptions};
use serde_json::json;

use crate::cli::QueueArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &QueueArgs) -> Result<CommandOutput> {
    let cfg = GroveConfig::load_or_create(&ctx.project_root)?;
    db::initialize(&ctx.project_root)?;

    // Resolve conversation_id: if --conversation is provided, use it directly.
    // If --continue is set, resolve the latest active conversation now so it's
    // stored with the task record (the task may execute later).
    let conversation_id = if args.conversation.is_some() {
        args.conversation.clone()
    } else if args.continue_last {
        use grove_core::orchestrator::conversation;
        let handle = grove_core::db::DbHandle::new(&ctx.project_root);
        let conn = handle.connect()?;
        let project_id = conversation::derive_project_id(&ctx.project_root);
        grove_core::db::repositories::conversations_repo::get_latest_for_project(
            &conn,
            &project_id,
        )?
        .map(|c| c.id)
    } else {
        None
    };

    // Add the task to the queue first.
    let task = orchestrator::queue_task(
        &ctx.project_root,
        &args.objective,
        args.budget_usd,
        args.priority,
        args.model.as_deref(),
        None, // no provider override from CLI
        conversation_id.as_deref(),
        None,  // no session resumption from CLI queue
        None,  // no pipeline override from CLI queue
        None,  // no permission_mode override from CLI queue
        false, // disable_phase_gates
    )?;

    eprintln!("[QUEUE] Task {} added: {}", task.id, task.objective);

    // If nothing is running right now, drain the queue immediately.
    // Otherwise the task will be picked up when the current run finishes.
    if !orchestrator::has_active_run(&ctx.project_root)? {
        eprintln!("[QUEUE] No active run — starting queue now…");
        drain_queue(ctx, &cfg)?;
    } else {
        eprintln!("[QUEUE] A run is in progress — task queued, will execute when it finishes.");
    }

    let json = json!({
        "task_id": task.id,
        "objective": task.objective,
        "state": task.state,
        "priority": task.priority,
        "message": "Task queued successfully"
    });

    let text = format!(
        "Task queued\ntask_id: {}\nobjective: {}\npriority: {}",
        task.id, task.objective, task.priority
    );

    Ok(to_text_or_json(ctx.format, text, json))
}

/// Execute every queued task in priority order until the queue is empty.
pub fn drain_queue(ctx: &CommandContext, cfg: &GroveConfig) -> Result<()> {
    while let Some(task) = orchestrator::dequeue_next_task(&ctx.project_root)? {
        eprintln!("\n[QUEUE] ▶ Starting task {}: {}", task.id, task.objective);
        let permission_mode = orchestrator::parse_permission_mode(task.permission_mode.as_deref());
        let provider = provider_from_config(
            cfg,
            &ctx.project_root,
            task.provider.as_deref(),
            permission_mode.clone(),
        )?;

        let result = orchestrator::execute_objective(
            &ctx.project_root,
            cfg,
            &task.objective,
            RunOptions {
                budget_usd: task.budget_usd,
                max_agents: None,
                model: task.model.clone(),
                provider: task.provider.clone(),
                interactive: false,
                pause_after: vec![],
                permission_mode,
                pipeline: task
                    .pipeline
                    .as_deref()
                    .and_then(grove_core::orchestrator::pipeline::PipelineKind::from_str),
                conversation_id: task.conversation_id.clone(),
                continue_last: false,
                db_path: None,
                abort_handle: None,
                issue_id: None,
                issue: None,
                resume_provider_session_id: task.resume_provider_session_id.clone(),
                on_run_created: None,
                input_handle_callback: None,
                run_control_callback: None,
                disable_phase_gates: task.disable_phase_gates,
            },
            provider,
        );

        match result {
            Ok(r) => {
                let task_state = orchestrator::task_terminal_state(&r.state);
                orchestrator::finish_task(
                    &ctx.project_root,
                    &task.id,
                    task_state,
                    Some(&r.run_id),
                )?;
                if task_state == "cancelled" {
                    eprintln!("[QUEUE] ⊘ Task {} aborted (run {})", task.id, r.run_id);
                } else {
                    eprintln!("[QUEUE] ✓ Task {} completed (run {})", task.id, r.run_id);
                }
            }
            Err(e) => {
                orchestrator::finish_task(&ctx.project_root, &task.id, "failed", None)?;
                eprintln!("[QUEUE] ✗ Task {} failed: {e}", task.id);
            }
        }
    }
    Ok(())
}

/// Build a `Provider` from the grove config by delegating to the canonical
/// `orchestrator::build_provider` which handles all provider types (claude_code,
/// coding agents, LLM direct, auto/workspace).
pub fn provider_from_config(
    cfg: &GroveConfig,
    project_root: &std::path::Path,
    provider_override: Option<&str>,
    permission_override: Option<grove_core::config::PermissionMode>,
) -> Result<std::sync::Arc<dyn grove_core::providers::Provider>> {
    grove_core::orchestrator::build_provider(
        cfg,
        project_root,
        provider_override,
        permission_override,
    )
    .map_err(|e| anyhow::anyhow!("failed to build provider: {e}"))
}
