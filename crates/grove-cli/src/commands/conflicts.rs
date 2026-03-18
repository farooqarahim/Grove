use anyhow::Result;
use serde_json::json;

use crate::cli::ConflictsArgs;
use crate::command_context::CommandContext;

use super::CommandOutput;

pub fn handle(ctx: &CommandContext, args: &ConflictsArgs) -> Result<CommandOutput> {
    let grove_dir = grove_core::config::grove_dir(&ctx.project_root);

    if let Some(ref path) = args.show {
        return show_conflict(&grove_dir, path);
    }

    if let Some(ref path) = args.resolve {
        return resolve_conflict(&grove_dir, path);
    }

    // Default: list all conflicts.
    list_conflicts(&grove_dir)
}

fn list_conflicts(grove_dir: &std::path::Path) -> Result<CommandOutput> {
    let conflicts = grove_core::worktree::conflict_ui::read_conflicts_manifest(grove_dir);

    match conflicts {
        Some(records) if !records.is_empty() => {
            let mut text = format!("{} unresolved conflict(s):\n\n", records.len());
            for c in &records {
                text.push_str(&format!(
                    "  {} — agents: [{}] — resolution: {:?}\n",
                    c.path,
                    c.agents.join(", "),
                    c.resolution,
                ));
            }
            text.push_str("\nArtifacts saved in .grove/conflicts/");
            text.push_str(
                "\nUse --show <path> to view details or --resolve <path> to mark resolved.",
            );

            let json_records: Vec<_> = records
                .iter()
                .map(|c| {
                    json!({
                        "path": c.path,
                        "agents": c.agents,
                        "resolution": format!("{:?}", c.resolution),
                    })
                })
                .collect();

            Ok(CommandOutput {
                text,
                json: json!({ "conflicts": json_records, "count": records.len() }),
            })
        }
        _ => Ok(CommandOutput {
            text: "No unresolved conflicts.".to_string(),
            json: json!({ "conflicts": [], "count": 0 }),
        }),
    }
}

fn show_conflict(grove_dir: &std::path::Path, rel_path: &str) -> Result<CommandOutput> {
    let conflict_dir = grove_dir.join("conflicts");

    let base = std::fs::read_to_string(conflict_dir.join(format!("{rel_path}.base"))).ok();
    let ours = std::fs::read_to_string(conflict_dir.join(format!("{rel_path}.ours"))).ok();
    let theirs = std::fs::read_to_string(conflict_dir.join(format!("{rel_path}.theirs"))).ok();

    if base.is_none() && ours.is_none() && theirs.is_none() {
        return Ok(CommandOutput {
            text: format!("No conflict artifacts found for '{rel_path}'."),
            json: json!({ "error": "not_found", "path": rel_path }),
        });
    }

    let mut text = format!("--- Conflict: {rel_path} ---\n\n");

    if let Some(ref b) = base {
        text.push_str("=== BASE (common ancestor) ===\n");
        text.push_str(b);
        text.push('\n');
    }
    if let Some(ref o) = ours {
        text.push_str("=== OURS (first agent) ===\n");
        text.push_str(o);
        text.push('\n');
    }
    if let Some(ref t) = theirs {
        text.push_str("=== THEIRS (second agent) ===\n");
        text.push_str(t);
        text.push('\n');
    }

    Ok(CommandOutput {
        text,
        json: json!({
            "path": rel_path,
            "base": base,
            "ours": ours,
            "theirs": theirs,
        }),
    })
}

fn resolve_conflict(grove_dir: &std::path::Path, rel_path: &str) -> Result<CommandOutput> {
    match grove_core::worktree::conflict_ui::resolve_conflict_artifacts(grove_dir, rel_path) {
        Ok(true) => Ok(CommandOutput {
            text: format!("Conflict '{rel_path}' marked as resolved. Artifacts removed."),
            json: json!({ "resolved": rel_path, "success": true }),
        }),
        Ok(false) => Ok(CommandOutput {
            text: format!("No conflict artifacts found for '{rel_path}'."),
            json: json!({ "resolved": rel_path, "success": false, "reason": "not_found" }),
        }),
        Err(e) => Ok(CommandOutput {
            text: format!("Error resolving '{rel_path}': {e}"),
            json: json!({ "resolved": rel_path, "success": false, "error": e.to_string() }),
        }),
    }
}
