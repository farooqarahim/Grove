use crate::cli::{CleanupArgs, GcArgs};
use crate::error::CliResult;
use crate::output::{OutputMode, json as json_out};
use crate::transport::{GroveTransport, Transport};

pub fn cleanup_cmd(
    args: CleanupArgs,
    transport: GroveTransport,
    mode: OutputMode,
) -> CliResult<()> {
    let result = transport.run_cleanup(args.project, args.conversation, args.dry_run)?;
    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json(&result));
        }
        OutputMode::Text { .. } => {
            let removed = result.get("removed").and_then(|v| v.as_i64()).unwrap_or(0);
            let dry = if args.dry_run { " (dry-run)" } else { "" };
            println!("removed {} item(s){}", removed, dry);
            if let Some(details) = result.get("details").and_then(|v| v.as_array()) {
                for d in details {
                    let kind = d.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                    let id = d.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    println!("  {} {}", kind, id);
                }
            }
        }
    }
    Ok(())
}

pub fn gc_cmd(args: GcArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let result = transport.run_gc(args.dry_run)?;
    match mode {
        OutputMode::Json => {
            println!("{}", json_out::emit_json(&result));
        }
        OutputMode::Text { .. } => {
            let freed = result
                .get("freed_bytes")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let dry = if args.dry_run { " (dry-run)" } else { "" };
            println!("gc: freed {} bytes{}", freed, dry);
            if let Some(items) = result.get("items_removed").and_then(|v| v.as_i64()) {
                println!("items removed: {}", items);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TestTransport;

    #[test]
    fn cleanup_cmd_ok() {
        let args = CleanupArgs {
            project: false,
            conversation: false,
            dry_run: true,
            yes: false,
            force: false,
        };
        let t = GroveTransport::Test(TestTransport::default());
        assert!(cleanup_cmd(args, t, OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn gc_cmd_ok() {
        let args = GcArgs { dry_run: true };
        let t = GroveTransport::Test(TestTransport::default());
        assert!(gc_cmd(args, t, OutputMode::Text { no_color: true }).is_ok());
    }

    #[test]
    fn cleanup_cmd_json_ok() {
        let args = CleanupArgs {
            project: true,
            conversation: false,
            dry_run: false,
            yes: true,
            force: false,
        };
        let t = GroveTransport::Test(TestTransport::default());
        assert!(cleanup_cmd(args, t, OutputMode::Json).is_ok());
    }
}
