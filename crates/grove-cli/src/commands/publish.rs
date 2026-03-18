use anyhow::Result;
use serde_json::json;

use crate::cli::{PublishAction, PublishArgs};
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &PublishArgs) -> Result<CommandOutput> {
    match &args.action {
        PublishAction::Retry(retry) => {
            let result =
                grove_core::orchestrator::retry_publish_run(&ctx.project_root, &retry.run_id)?;

            let mut text = format!(
                "Publish retried\nrun_id: {}\nstatus: {}",
                result.run_id, result.publish_status
            );
            if let Some(ref sha) = result.final_commit_sha {
                text.push_str(&format!("\ncommit: {sha}"));
            }
            if let Some(ref pr_url) = result.pr_url {
                text.push_str(&format!("\npr: {pr_url}"));
            }
            if let Some(ref error) = result.error {
                text.push_str(&format!("\nerror: {error}"));
            }

            let json = json!({
                "run_id": result.run_id,
                "publish_status": result.publish_status,
                "final_commit_sha": result.final_commit_sha,
                "pr_url": result.pr_url,
                "published_at": result.published_at,
                "error": result.error,
            });

            Ok(to_text_or_json(ctx.format, text, json))
        }
    }
}
