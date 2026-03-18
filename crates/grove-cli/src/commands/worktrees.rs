use std::io::{self, Write as _};

use anyhow::Result;
use grove_core::db;
use grove_core::worktree;
use serde_json::json;

use crate::cli::WorktreesArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &WorktreesArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    // ── Delete ALL worktrees (with confirmation) ──────────────────────────────
    if args.delete_all {
        if !args.yes {
            eprint!(
                "This will delete all agent worktrees. Active (queued/running) sessions are skipped automatically. Proceed? [y/N] "
            );
            io::stderr().flush()?;
            let mut input = String::new();
            let n = io::stdin().read_line(&mut input)?;
            if n == 0 {
                // stdin is closed (non-interactive / piped context without --yes).
                let json = json!({ "aborted": true, "reason": "stdin_closed" });
                return Ok(to_text_or_json(
                    ctx.format,
                    "Aborted (stdin is closed; pass --yes to skip the prompt).".to_string(),
                    json,
                ));
            }
            if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                let json = json!({ "aborted": true, "reason": "user_declined" });
                return Ok(to_text_or_json(ctx.format, "Aborted.".to_string(), json));
            }
        }
        let (count, freed) = worktree::delete_all_worktrees(&ctx.project_root)?;
        let text = if count == 0 {
            "No deletable worktrees found.".to_string()
        } else {
            format!(
                "Deleted {count} worktree(s) — freed {}",
                format_bytes(freed)
            )
        };
        let json = json!({ "deleted": count, "freed_bytes": freed });
        return Ok(to_text_or_json(ctx.format, text, json));
    }

    // ── Delete one specific worktree ──────────────────────────────────────────
    if let Some(ref session_id) = args.delete {
        let freed = worktree::delete_worktree(&ctx.project_root, session_id)?;
        let text = format!(
            "Deleted worktree {session_id} — freed {}",
            format_bytes(freed)
        );
        let json = json!({ "session_id": session_id, "freed_bytes": freed });
        return Ok(to_text_or_json(ctx.format, text, json));
    }

    // ── Clean all finished worktrees ──────────────────────────────────────────
    if args.clean {
        let (count, freed) = worktree::delete_finished_worktrees(&ctx.project_root)?;
        let text = if count == 0 {
            "No finished worktrees to clean.".to_string()
        } else {
            format!(
                "Cleaned {count} worktree(s) — freed {}",
                format_bytes(freed)
            )
        };
        let json = json!({ "deleted": count, "freed_bytes": freed });
        return Ok(to_text_or_json(ctx.format, text, json));
    }

    // ── List all worktrees ────────────────────────────────────────────────────
    let entries = worktree::list_worktrees(&ctx.project_root, true)?;

    let total_bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
    let json_entries: Vec<_> = entries
        .iter()
        .map(|e| {
            json!({
                "session_id": e.session_id,
                "path": e.path,
                "size_bytes": e.size_bytes,
                "size": e.size_display(),
                "run_id": e.run_id,
                "agent_type": e.agent_type,
                "state": e.state,
                "created_at": e.created_at,
                "ended_at": e.ended_at,
            })
        })
        .collect();

    let json = json!({
        "worktrees": json_entries,
        "total": entries.len(),
        "total_size_bytes": total_bytes,
        "total_size": format_bytes(total_bytes),
    });

    if entries.is_empty() {
        return Ok(to_text_or_json(
            ctx.format,
            "No worktrees found. Run a task first.".to_string(),
            json,
        ));
    }

    let mut lines = vec![format!(
        "{} worktree(s)  —  total {}  (use --clean to free space)\n",
        entries.len(),
        format_bytes(total_bytes)
    )];

    for e in &entries {
        let state_icon = match e.state.as_deref() {
            Some("running") | Some("queued") | Some("waiting") => "▶",
            Some("completed") => "✓",
            Some("failed") | Some("killed") => "✗",
            _ => "?",
        };
        let agent = e.agent_type.as_deref().unwrap_or("unknown");
        let run = e
            .run_id
            .as_deref()
            .map(|r| format!("  run: {r}"))
            .unwrap_or_default();
        let ended = e
            .ended_at
            .as_deref()
            .map(|t| format!("  ended: {t}"))
            .unwrap_or_default();

        lines.push(format!(
            "  {state_icon} {:10}  {:9}  {}{}{}",
            e.state.as_deref().unwrap_or("unknown"),
            e.size_display(),
            e.session_id,
            run,
            ended,
        ));
        lines.push(format!("      agent: {agent}  path: {}", e.path.display()));
    }

    lines.push(String::new());
    lines.push("  grove worktrees --clean           delete all finished worktrees".to_string());
    lines.push("  grove worktrees --delete <id>     delete one worktree by session ID".to_string());
    lines.push(
        "  grove worktrees --delete-all      delete all worktrees (prompts for confirmation)"
            .to_string(),
    );

    Ok(to_text_or_json(ctx.format, lines.join("\n"), json))
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1_024;
    const MB: u64 = 1_024 * KB;
    const GB: u64 = 1_024 * MB;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
