use crate::cli::{SignalAction, SignalArgs};
use crate::error::CliResult;
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

pub fn dispatch(args: SignalArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    match args.action {
        SignalAction::Send {
            run_id,
            from,
            to,
            signal_type,
            payload,
            priority,
        } => send_cmd(
            &run_id,
            &from,
            &to,
            &signal_type,
            payload.as_deref(),
            priority.map(i64::from),
            transport,
            mode,
        ),
        SignalAction::Check { run_id, agent } => check_cmd(&run_id, &agent, transport, mode),
        SignalAction::List { run_id } => list_cmd(&run_id, transport, mode),
    }
}

#[allow(clippy::too_many_arguments)]
fn send_cmd(
    run_id: &str,
    from: &str,
    to: &str,
    signal_type: &str,
    payload: Option<&str>,
    priority: Option<i64>,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.send_signal(run_id, from, to, signal_type, payload, priority)?;
    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json(&serde_json::json!({"ok": true})));
        }
        OutputMode::Text { .. } => {
            text::success("signal sent");
        }
    }
    Ok(())
}

fn check_cmd(
    run_id: &str,
    agent: &str,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    let signals = transport.check_signals(run_id, agent)?;
    render_signals(signals, mode)
}

pub fn list_cmd(run_id: &str, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let signals = transport.list_signals(run_id)?;
    render_signals(signals, mode)
}

fn render_signals(signals: Vec<serde_json::Value>, mode: OutputMode) -> CliResult<()> {
    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::Value::Array(signals))
            );
        }
        OutputMode::Text { .. } => {
            if signals.is_empty() {
                println!("{}", text::dim("no signals"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = signals
                .iter()
                .map(|s| {
                    vec![
                        s.get("signal_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("from_agent")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        s.get("priority")
                            .and_then(|v| v.as_i64())
                            .map(|n| n.to_string())
                            .unwrap_or_default(),
                        s.get("created_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .chars()
                            .take(19)
                            .collect(),
                    ]
                })
                .collect();
            println!(
                "{}",
                text::render_table(&["TYPE", "FROM", "PRIORITY", "CREATED"], &rows)
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TestTransport;

    #[test]
    fn signal_list_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(
            list_cmd(
                "run-abc",
                t,
                crate::output::OutputMode::Text { no_color: true }
            )
            .is_ok()
        );
    }

    #[test]
    fn signal_check_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(
            check_cmd(
                "run-abc",
                "builder",
                t,
                crate::output::OutputMode::Text { no_color: true }
            )
            .is_ok()
        );
    }

    #[test]
    fn signal_list_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd("run-abc", t, crate::output::OutputMode::Json).is_ok());
    }
}
