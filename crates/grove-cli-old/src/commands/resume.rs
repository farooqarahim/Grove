use anyhow::Result;
use grove_core::orchestrator;
use serde_json::json;

use crate::cli::ResumeArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &ResumeArgs) -> Result<CommandOutput> {
    let result = orchestrator::resume_run(&ctx.project_root, &args.run_id)?;

    let json = json!({
      "run_id": result.run_id,
      "state": result.state,
      "objective": result.objective,
      "report_path": result.report_path,
      "plan": result.plan
    });

    let text = format!(
        "Run resumed\nrun_id: {}\nstate: {}\nreport_path: {}",
        result.run_id,
        result.state,
        result.report_path.as_deref().unwrap_or("<none>")
    );

    Ok(to_text_or_json(ctx.format, text, json))
}
