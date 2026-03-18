use anyhow::Result;
use grove_core::config::GroveConfig;
use grove_core::db;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::RunArgs;
use crate::command_context::CommandContext;
use crate::commands::queue::drain_queue;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &RunArgs) -> Result<CommandOutput> {
    let cfg = GroveConfig::load_or_create(&ctx.project_root)?;
    db::initialize(&ctx.project_root)?;

    // Queue the task. The drain loop will pick it up for execution.
    let task = orchestrator::queue_task(
        &ctx.project_root,
        &args.objective,
        args.budget_usd,
        0, // default priority
        args.model.as_deref(),
        None, // no provider override from CLI
        args.conversation.as_deref(),
        None, // no session resumption from CLI run
        args.pipeline.as_deref(),
        None,  // no permission_mode override from CLI run
        false, // disable_phase_gates
    )?;

    eprintln!("[RUN] Task {} queued: {}", task.id, task.objective);

    // Drain: dequeues eligible tasks and runs them sequentially.
    // If this conversation already has an active run, the task stays queued
    // until the active run finishes (its process drains the queue on completion).
    drain_queue(ctx, &cfg)?;

    // After drain, check if our task was executed.
    let tasks = orchestrator::list_tasks(&ctx.project_root)?;
    let our_task = tasks.iter().find(|t| t.id == task.id);

    let (text, json) = match our_task {
        Some(t) if t.state == "completed" => {
            let run_id = t.run_id.as_deref().unwrap_or("<unknown>");
            let pipeline_kind = t
                .pipeline
                .as_deref()
                .and_then(orchestrator::pipeline::PipelineKind::from_str)
                .unwrap_or_default();
            let plan_agents: Vec<&str> =
                pipeline_kind.agents().iter().map(|a| a.as_str()).collect();
            let text = format!(
                "Run complete\nrun_id: {run_id}\nstate: completed\nobjective: {}",
                t.objective
            );
            let json = json!({
                "run_id": run_id,
                "state": "completed",
                "objective": t.objective,
                "task_id": t.id,
                "plan": plan_agents,
            });
            (text, json)
        }
        Some(t) if t.state == "failed" => {
            let text = format!("Run failed\ntask_id: {}\nobjective: {}", t.id, t.objective);
            let json = json!({
                "state": "failed",
                "objective": t.objective,
                "task_id": t.id,
            });
            (text, json)
        }
        _ => {
            // Task is still queued (another run is active on this conversation).
            eprintln!(
                "[RUN] A run is already active on this conversation. \
                 Task {} will execute when the current run finishes.",
                task.id
            );
            let text = format!(
                "Task queued\ntask_id: {}\nobjective: {}\nstate: queued",
                task.id, task.objective
            );
            let json = json!({
                "state": "queued",
                "objective": task.objective,
                "task_id": task.id,
                "message": "Task queued — will execute when the current run finishes"
            });
            (text, json)
        }
    };

    Ok(to_text_or_json(ctx.format, text, json))
}
