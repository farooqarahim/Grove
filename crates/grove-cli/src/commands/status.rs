use crate::cli::{
    AbortArgs, ConflictsArgs, LogsArgs, MergeStatusArgs, OwnershipArgs, PlanArgs, PublishArgs,
    ReportArgs, ResumeArgs, SessionsArgs, StatusArgs, SubtasksArgs,
};
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

// ── status ────────────────────────────────────────────────────────────────────

pub fn status_cmd(args: StatusArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    // --watch: delegate to TUI run-watch (only with feature = "tui")
    #[cfg(feature = "tui")]
    if args.watch {
        return crate::tui::status_watch::run(transport);
    }
    #[cfg(not(feature = "tui"))]
    if args.watch {
        return Err(CliError::Other(
            "TUI mode requires feature 'tui'. Reinstall with: cargo install grove-cli --features tui"
                .into(),
        ));
    }

    let runs = transport.list_runs(args.limit)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::to_value(&runs).map_err(|e| CliError::Other(e.to_string()))?;
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if runs.is_empty() {
                println!("{}", text::dim("no runs"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = runs
                .iter()
                .map(|r| {
                    vec![
                        r.id.chars().take(8).collect(),
                        r.objective.chars().take(50).collect(),
                        r.state.clone(),
                        r.current_agent.as_deref().unwrap_or("").to_string(),
                        r.created_at.chars().take(19).collect(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["ID", "OBJECTIVE", "STATE", "AGENT", "CREATED"], &rows)
            );
        }
    }
    Ok(())
}

// ── logs ──────────────────────────────────────────────────────────────────────

pub fn logs_cmd(args: LogsArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let events = transport.get_logs(&args.run_id, args.all)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(events);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if events.is_empty() {
                println!("{}", text::dim("no events"));
                return Ok(());
            }
            for event in &events {
                let ts = event
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let et = event
                    .get("event_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let payload = event
                    .get("payload")
                    .map(|p| serde_json::to_string(p).unwrap_or_else(|_| "{}".to_string()))
                    .unwrap_or_default();
                println!("{} {} {}", ts, et, payload);
            }
        }
    }
    Ok(())
}

// ── report ────────────────────────────────────────────────────────────────────

pub fn report_cmd(args: ReportArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let report = transport.get_report(&args.run_id)?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&report));
        }
        OutputMode::Text { .. } => {
            let total = report
                .get("total_spent_usd")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let total_runs = report
                .get("total_runs")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            println!("Cost Report (all completed runs)");
            println!("total cost : ${:.4}", total);
            println!("total runs : {}", total_runs);

            if let Some(by_agent) = report.get("by_agent").and_then(|v| v.as_array()) {
                if !by_agent.is_empty() {
                    println!("\nby agent:");
                    for entry in by_agent {
                        let agent = entry
                            .get("agent_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let cost = entry
                            .get("total_cost_usd")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let count = entry
                            .get("session_count")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        println!("  {:<20} ${:.4}  ({} sessions)", agent, cost, count);
                    }
                }
            }
        }
    }
    Ok(())
}

// ── plan ──────────────────────────────────────────────────────────────────────

pub fn plan_cmd(args: PlanArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let plan = transport.get_plan(args.run_id.as_deref())?;

    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json_pretty(&plan));
        }
        OutputMode::Text { .. } => {
            if let Some(steps) = plan.as_array() {
                if steps.is_empty() {
                    println!("{}", text::dim("no plan steps"));
                    return Ok(());
                }
                for step in steps {
                    let wave = step.get("wave").and_then(|v| v.as_i64()).unwrap_or(0);
                    let idx = step.get("step_index").and_then(|v| v.as_i64()).unwrap_or(0);
                    let title = step.get("title").and_then(|v| v.as_str()).unwrap_or("");
                    let agent = step
                        .get("agent_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let status = step.get("status").and_then(|v| v.as_str()).unwrap_or("");
                    println!("  [wave {wave}] {idx:>2}. [{agent}] {title}  ({status})");
                }
            } else {
                println!("{}", text::dim("no plan"));
            }
        }
    }
    Ok(())
}

// ── subtasks ──────────────────────────────────────────────────────────────────

pub fn subtasks_cmd(
    args: SubtasksArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    let subtasks = transport.get_subtasks(args.run_id.as_deref())?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(subtasks);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if subtasks.is_empty() {
                println!("{}", text::dim("no subtasks"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = subtasks
                .iter()
                .map(|s| {
                    vec![
                        s.get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("agent_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("depends_on")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            })
                            .unwrap_or_default(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["TITLE", "STATUS", "AGENT", "DEPENDS"], &rows)
            );
        }
    }
    Ok(())
}

// ── sessions ──────────────────────────────────────────────────────────────────

pub fn sessions_cmd(
    args: SessionsArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    let sessions = transport.get_sessions(&args.run_id)?;

    match mode {
        OutputMode::Json => {
            let val = serde_json::Value::Array(sessions);
            println!("{}", json_out::emit_json(&val));
        }
        OutputMode::Text { .. } => {
            if sessions.is_empty() {
                println!("{}", text::dim("no sessions"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = sessions
                .iter()
                .map(|s| {
                    vec![
                        s.get("id")
                            .and_then(|v| v.as_str())
                            .map(|id| id.chars().take(8).collect::<String>())
                            .unwrap_or_default(),
                        s.get("agent_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("state")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("started_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("ended_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        // cost_usd is not in SessionRecord — leave blank
                        String::new(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["ID", "AGENT", "STATE", "STARTED", "ENDED", "COST"], &rows)
            );
        }
    }
    Ok(())
}

// ── resume ────────────────────────────────────────────────────────────────────

pub fn resume_cmd(args: ResumeArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    transport.resume_run(&args.run_id)?;
    match mode {
        OutputMode::Json => println!("{}", serde_json::json!({"ok": true, "run_id": args.run_id})),
        OutputMode::Text { .. } => println!(
            "resumed {}",
            args.run_id.chars().take(8).collect::<String>()
        ),
    }
    Ok(())
}

// ── abort ─────────────────────────────────────────────────────────────────────

pub fn abort_cmd(args: AbortArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    transport.abort_run(&args.run_id)?;
    match mode {
        OutputMode::Json => println!("{}", serde_json::json!({"ok": true, "run_id": args.run_id})),
        OutputMode::Text { .. } => println!(
            "aborted {}",
            args.run_id.chars().take(8).collect::<String>()
        ),
    }
    Ok(())
}

// ── not-yet-implemented stubs ─────────────────────────────────────────────────

pub fn ownership_cmd(_a: OwnershipArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn conflicts_cmd(_a: ConflictsArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn merge_status_cmd(_a: MergeStatusArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

pub fn publish_cmd(_a: PublishArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Err(CliError::Other("not yet implemented".into()))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn status_cmd_empty_list_renders_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = status_cmd(
            crate::cli::StatusArgs {
                limit: 20,
                watch: false,
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn status_cmd_json_mode_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = status_cmd(
            crate::cli::StatusArgs {
                limit: 20,
                watch: false,
            },
            t,
            crate::output::OutputMode::Json,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn logs_cmd_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = logs_cmd(
            crate::cli::LogsArgs {
                run_id: "test-run".into(),
                all: false,
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn logs_cmd_json_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = logs_cmd(
            crate::cli::LogsArgs {
                run_id: "test-run".into(),
                all: true,
            },
            t,
            crate::output::OutputMode::Json,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn report_cmd_null_report_text_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = report_cmd(
            crate::cli::ReportArgs {
                run_id: "test-run".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn report_cmd_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = report_cmd(
            crate::cli::ReportArgs {
                run_id: "test-run".into(),
            },
            t,
            crate::output::OutputMode::Json,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn plan_cmd_null_plan_text_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = plan_cmd(
            crate::cli::PlanArgs { run_id: None },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn subtasks_cmd_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = subtasks_cmd(
            crate::cli::SubtasksArgs { run_id: None },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn sessions_cmd_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = sessions_cmd(
            crate::cli::SessionsArgs {
                run_id: "test-run".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }

    #[test]
    fn resume_cmd_returns_err_for_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = resume_cmd(
            crate::cli::ResumeArgs {
                run_id: "test-run".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn abort_cmd_returns_err_for_test_transport() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = abort_cmd(
            crate::cli::AbortArgs {
                run_id: "test-run".into(),
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }

    #[test]
    fn ownership_cmd_returns_not_yet_implemented() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = ownership_cmd(
            crate::cli::OwnershipArgs { run_id: None },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_err());
    }
}
