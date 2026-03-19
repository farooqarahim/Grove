use crate::cli::HookArgs;
use crate::error::CliResult;
use crate::output::{OutputMode, json as json_out};
use crate::transport::{GroveTransport, Transport};

pub fn run(args: HookArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    run_cmd(
        &args.event,
        Some(&args.agent_type),
        args.run_id.as_deref(),
        args.session_id.as_deref(),
        args.tool.as_deref(),
        args.file_path.as_deref(),
        transport,
        mode,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_cmd(
    event: &str,
    agent_type: Option<&str>,
    run_id: Option<&str>,
    session_id: Option<&str>,
    tool: Option<&str>,
    file_path: Option<&str>,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    transport.run_hook(event, agent_type, run_id, session_id, tool, file_path)?;
    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json(&serde_json::json!({"ok": true})));
        }
        OutputMode::Text { .. } => {
            // On success, print nothing (exit 0).
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TestTransport;

    #[test]
    fn hook_run_cmd_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(
            run_cmd(
                "PostToolUse",
                Some("builder"),
                None,
                None,
                None,
                None,
                t,
                OutputMode::Text { no_color: true }
            )
            .is_ok()
        );
    }

    #[test]
    fn hook_run_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(
            run_cmd(
                "PostToolUse",
                Some("builder"),
                Some("run-1"),
                Some("sess-1"),
                Some("Edit"),
                Some("/tmp/foo.rs"),
                t,
                OutputMode::Json
            )
            .is_ok()
        );
    }
}
