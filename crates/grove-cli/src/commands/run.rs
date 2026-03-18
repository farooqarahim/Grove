use crate::cli::{PermissionModeArg, QueueArgs, RunArgs, TaskCancelArgs, TasksArgs};
use crate::error::{CliError, CliResult};
use crate::output::{text, OutputMode};
use crate::transport::{GroveTransport, StartRunRequest, Transport};

pub fn run_cmd(args: RunArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let pb = match &mode {
        OutputMode::Text { .. } => Some(text::spinner("Starting run…")),
        OutputMode::Json => None,
    };

    let req = StartRunRequest {
        objective: args.objective.clone(),
        pipeline: args.pipeline.clone(),
        model: args.model.clone(),
        permission_mode: args.permission_mode.map(|m| match m {
            PermissionModeArg::SkipAll => "skip_all".to_string(),
            PermissionModeArg::HumanGate => "human_gate".to_string(),
            PermissionModeArg::AutonomousGate => "autonomous_gate".to_string(),
        }),
        conversation_id: args.conversation.clone(),
        continue_last: args.continue_last,
        issue_id: args.issue.clone(),
        max_agents: args.max_agents,
    };

    let result = transport.start_run(req);
    if let Some(pb) = pb {
        pb.finish_and_clear();
    }
    let result = result?;

    // --watch: delegate to TUI run-watch (only with feature=tui)
    #[cfg(feature = "tui")]
    if args.watch {
        return crate::tui::run_watch::run(result.run_id, transport);
    }
    #[cfg(not(feature = "tui"))]
    if args.watch {
        return Err(CliError::Other(
            "TUI mode requires feature 'tui'. Reinstall with: cargo install grove-cli --features tui".into(),
        ));
    }

    match mode {
        OutputMode::Json => println!(
            "{}",
            serde_json::json!({
                "run_id": result.run_id,
                "state": result.state,
                "objective": result.objective,
            })
        ),
        OutputMode::Text { .. } => {
            println!(
                "run {} started ({})",
                result.run_id.chars().take(8).collect::<String>(),
                result.state
            );
        }
    }
    Ok(())
}

pub fn queue_cmd(args: QueueArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let task = transport.queue_task(
        &args.objective,
        args.priority,
        args.model.as_deref(),
        args.conversation.as_deref(),
        None,
        None,
    )?;
    match mode {
        OutputMode::Json => println!("{}", serde_json::to_string(&task).map_err(|e| CliError::Other(e.to_string()))?),
        OutputMode::Text { .. } => println!(
            "queued {} (priority {})",
            task.id.chars().take(8).collect::<String>(),
            task.priority
        ),
    }
    Ok(())
}

pub fn tasks_cmd(args: TasksArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let all_tasks = transport.list_tasks()?;
    let limit = usize::try_from(args.limit).unwrap_or(0);
    let tasks: Vec<_> = all_tasks.into_iter().take(limit).collect();
    match mode {
        OutputMode::Json => println!("{}", serde_json::to_string(&tasks).map_err(|e| CliError::Other(e.to_string()))?),
        OutputMode::Text { .. } => {
            if tasks.is_empty() {
                println!("{}", text::dim("no tasks"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = tasks
                .iter()
                .map(|t| {
                    vec![
                        t.id.chars().take(8).collect(),
                        t.objective.chars().take(50).collect(),
                        t.state.clone(),
                        t.priority.to_string(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["ID", "OBJECTIVE", "STATE", "PRI"], &rows)
            );
        }
    }
    Ok(())
}

pub fn task_cancel_cmd(
    args: TaskCancelArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.cancel_task(&args.task_id)?;
    match mode {
        OutputMode::Json => println!(
            "{}",
            serde_json::json!({"ok": true, "task_id": args.task_id})
        ),
        OutputMode::Text { .. } => {
            println!(
                "cancelled {}",
                args.task_id.chars().take(8).collect::<String>()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn tasks_cmd_with_empty_transport_renders_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = tasks_cmd(
            crate::cli::TasksArgs {
                limit: 10,
                refresh: false,
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn tasks_cmd_json_mode_renders_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = tasks_cmd(
            crate::cli::TasksArgs { limit: 10, refresh: false },
            t,
            crate::output::OutputMode::Json,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn task_cancel_cmd_returns_not_implemented_for_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = task_cancel_cmd(
            crate::cli::TaskCancelArgs { task_id: "abc123".into() },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }
}
