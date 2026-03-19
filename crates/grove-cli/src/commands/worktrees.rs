use crate::cli::WorktreesArgs;
use crate::error::{CliError, CliResult};
use crate::output::{OutputMode, json as json_out, text};
use crate::transport::{GroveTransport, Transport};

pub fn dispatch_cmd(
    args: WorktreesArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    if args.clean {
        return clean_cmd(transport, mode);
    }
    if let Some(id) = args.delete {
        return delete_cmd(&id, transport, mode);
    }
    if args.delete_all {
        if !args.yes {
            // Prompt for confirmation when not bypassed with -y.
            eprint!("Delete ALL worktrees? This cannot be undone. [y/N] ");
            let mut input = String::new();
            std::io::stdin()
                .read_line(&mut input)
                .map_err(|e| CliError::Other(e.to_string()))?;
            if input.trim().to_lowercase() != "y" {
                println!("aborted");
                return Ok(());
            }
        }
        return delete_all_cmd(transport, mode);
    }
    list_cmd(transport, mode)
}

pub fn list_cmd(transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let worktrees = transport.list_worktrees()?;
    match mode {
        OutputMode::Json => {
            println!(
                "{}",
                json_out::emit_json(&serde_json::Value::Array(worktrees))
            );
        }
        OutputMode::Text { .. } => {
            if worktrees.is_empty() {
                println!("{}", text::dim("no worktrees"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = worktrees
                .iter()
                .map(|w| {
                    vec![
                        w.get("session_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .chars()
                            .take(8)
                            .collect(),
                        w.get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        w.get("size_bytes")
                            .and_then(|v| v.as_i64())
                            .map(format_bytes)
                            .unwrap_or_default(),
                        w.get("run_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .chars()
                            .take(8)
                            .collect(),
                        w.get("agent_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        w.get("state")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        w.get("created_at")
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
                text::render_table(
                    &[
                        "SESSION", "PATH", "SIZE", "RUN", "AGENT", "STATE", "CREATED"
                    ],
                    &rows
                )
            );
        }
    }
    Ok(())
}

fn clean_cmd(transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let result = transport.clean_worktrees()?;
    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json(&result));
        }
        OutputMode::Text { .. } => {
            let cleaned = result.get("cleaned").and_then(|v| v.as_i64()).unwrap_or(0);
            println!("cleaned {} worktree(s)", cleaned);
        }
    }
    Ok(())
}

fn delete_cmd(id: &str, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    transport.delete_worktree(id)?;
    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json(&serde_json::json!({"ok": true})));
        }
        OutputMode::Text { .. } => {
            println!("deleted worktree {}", id);
        }
    }
    Ok(())
}

fn delete_all_cmd(transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let result = transport.delete_all_worktrees()?;
    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json(&result));
        }
        OutputMode::Text { .. } => {
            let deleted = result.get("deleted").and_then(|v| v.as_i64()).unwrap_or(0);
            println!("deleted {} worktree(s)", deleted);
        }
    }
    Ok(())
}

fn format_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

// Keep old entry point for backwards compatibility with dispatch in mod.rs.
pub fn run(args: WorktreesArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    dispatch_cmd(args, transport, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TestTransport;

    #[test]
    fn worktrees_list_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn worktrees_list_json_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(t, crate::output::OutputMode::Json).is_ok());
    }

    #[test]
    fn worktrees_dispatch_no_flags_lists() {
        let args = WorktreesArgs {
            clean: false,
            delete: None,
            delete_all: false,
            yes: false,
        };
        let t = GroveTransport::Test(TestTransport::default());
        assert!(dispatch_cmd(args, t, OutputMode::Text { no_color: true }).is_ok());
    }
}
