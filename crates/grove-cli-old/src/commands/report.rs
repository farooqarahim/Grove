use anyhow::Result;
use grove_core::db::DbHandle;
use grove_core::reporting;

use crate::cli::ReportArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, render_path, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &ReportArgs) -> Result<CommandOutput> {
    // Generate the JSON report file on disk.
    let path = reporting::generate_report(&ctx.project_root, &args.run_id)?;

    // Build the structured report for output.
    let handle = DbHandle::new(&ctx.project_root);
    let conn = handle.connect()?;
    let report = reporting::build_report(&conn, &args.run_id)?;
    let json = serde_json::to_value(&report)?;

    let text = format!(
        "Report generated\nrun_id: {}\npath: {}",
        args.run_id,
        render_path(&path)
    );

    Ok(to_text_or_json(ctx.format, text, json))
}
