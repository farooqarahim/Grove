use anyhow::Result;
use serde_json::json;

use super::CommandOutput;
use crate::cli::GcArgs;
use crate::command_context::CommandContext;

pub fn handle(ctx: &CommandContext, args: &GcArgs) -> Result<CommandOutput> {
    let handle = grove_core::db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;

    let is_git = grove_core::worktree::git_ops::is_git_repo(&ctx.project_root);

    if args.dry_run {
        let text = format!(
            "Dry run:\n  git gc --auto: {}",
            if is_git {
                "would run"
            } else {
                "skipped (not a git repo)"
            }
        );
        return Ok(CommandOutput {
            json: json!({
                "dry_run": true,
                "git_gc": is_git,
            }),
            text,
        });
    }

    // Backup the database before the destructive sweep in case something
    // goes wrong.
    match grove_core::db::backup(&ctx.project_root) {
        Ok(p) => tracing::info!(backup = %p.display(), "pre-GC database backup created"),
        Err(e) => tracing::warn!(error = %e, "pre-GC backup failed — continuing anyway"),
    }

    // Consolidated sweep: ghost sessions + orphaned branches/dirs + git maintenance.
    let report = grove_core::worktree::sweep_orphaned_resources(&ctx.project_root, &mut conn)?;

    let text = format!(
        "GC complete:\n  Orphaned branches deleted: {}\n  Orphaned dirs removed: {}\n  git gc --auto: {}",
        report.orphaned_branches_deleted,
        report.orphaned_dirs_removed,
        if report.git_gc_ran { "done" } else { "skipped" }
    );

    Ok(CommandOutput {
        json: json!({
            "orphaned_branches_deleted": report.orphaned_branches_deleted,
            "orphaned_dirs_removed": report.orphaned_dirs_removed,
            "git_gc": report.git_gc_ran,
        }),
        text,
    })
}
