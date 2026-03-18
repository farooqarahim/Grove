use std::io::{self, Write as _};

use anyhow::Result;
use grove_core::db;
use grove_core::worktree::{self, CleanupFilter};
use serde_json::json;

use crate::cli::CleanupArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &CleanupArgs) -> Result<CommandOutput> {
    db::initialize(&ctx.project_root)?;

    let handle = db::DbHandle::new(&ctx.project_root);
    let mut conn = handle.connect()?;

    // ── Force mode: release all pool slots and delete all worktree dirs ──
    if args.force {
        return handle_force(ctx, args, &mut conn);
    }

    let filter = CleanupFilter {
        project_id: args.project.clone(),
        conversation_id: args.conversation.clone(),
    };
    let conn = conn; // rebind as immutable

    // Preview: list what would be deleted.
    let entries = worktree::list_worktrees_with_conn(&ctx.project_root, &conn, false)?;
    let allowed = {
        // Re-derive the allowed set to compute the preview.
        let has_filter = filter.project_id.is_some() || filter.conversation_id.is_some();
        if has_filter {
            // Build the same session set that delete_finished_worktrees_filtered uses.
            let mut ids = std::collections::HashSet::new();
            if let Some(ref cid) = filter.conversation_id {
                let mut stmt = conn.prepare(
                    "SELECT s.id FROM sessions s JOIN runs r ON s.run_id = r.id WHERE r.conversation_id = ?1",
                )?;
                let rows = stmt.query_map([cid], |r| r.get::<_, String>(0))?;
                for row in rows.flatten() {
                    ids.insert(row);
                }
            } else if let Some(ref pid) = filter.project_id {
                let mut stmt = conn.prepare(
                    "SELECT s.id FROM sessions s JOIN runs r ON s.run_id = r.id JOIN conversations c ON r.conversation_id = c.id WHERE c.project_id = ?1",
                )?;
                let rows = stmt.query_map([pid], |r| r.get::<_, String>(0))?;
                for row in rows.flatten() {
                    ids.insert(row);
                }
            }
            Some(ids)
        } else {
            None
        }
    };

    let candidates: Vec<_> = entries
        .iter()
        .filter(|e| !e.is_active())
        .filter(|e| !e.session_id.starts_with("run_"))
        .filter(|e| {
            if let Some(ref ids) = allowed {
                ids.contains(&e.session_id)
            } else {
                true
            }
        })
        .collect();

    let candidate_count = candidates.len();
    let candidate_bytes: u64 = candidates.iter().map(|e| e.size_bytes).sum();

    if candidate_count == 0 {
        let json = json!({ "deleted": 0, "freed_bytes": 0 });
        return Ok(to_text_or_json(
            ctx.format,
            "No finished worktrees matching the filter.".to_string(),
            json,
        ));
    }

    // ── Dry run ──────────────────────────────────────────────────────────────
    if args.dry_run {
        let mut lines = vec![format!(
            "Would delete {} worktree(s), freeing {}\n",
            candidate_count,
            format_bytes(candidate_bytes),
        )];
        for c in &candidates {
            let agent = c.agent_type.as_deref().unwrap_or("unknown");
            lines.push(format!(
                "  {} ({}, {})",
                c.session_id,
                agent,
                c.size_display()
            ));
        }
        let json = json!({
            "dry_run": true,
            "would_delete": candidate_count,
            "would_free_bytes": candidate_bytes,
        });
        return Ok(to_text_or_json(ctx.format, lines.join("\n"), json));
    }

    // ── Confirmation ─────────────────────────────────────────────────────────
    if !args.yes {
        let scope = if filter.conversation_id.is_some() {
            format!(
                " (conversation: {})",
                filter.conversation_id.as_ref().unwrap()
            )
        } else if filter.project_id.is_some() {
            format!(" (project: {})", filter.project_id.as_ref().unwrap())
        } else {
            String::new()
        };
        eprint!(
            "Delete {} finished worktree(s){scope}, freeing {}? [y/N] ",
            candidate_count,
            format_bytes(candidate_bytes),
        );
        io::stderr().flush()?;
        let mut input = String::new();
        let n = io::stdin().read_line(&mut input)?;
        if n == 0 {
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

    // ── Delete ───────────────────────────────────────────────────────────────
    let (count, freed) =
        worktree::delete_finished_worktrees_filtered(&ctx.project_root, &conn, &filter)?;

    let text = if count == 0 {
        "No finished worktrees to clean.".to_string()
    } else {
        format!(
            "Cleaned {} worktree(s) — freed {}",
            count,
            format_bytes(freed)
        )
    };
    let json = json!({ "deleted": count, "freed_bytes": freed });
    Ok(to_text_or_json(ctx.format, text, json))
}

fn handle_force(
    ctx: &CommandContext,
    args: &CleanupArgs,
    conn: &mut rusqlite::Connection,
) -> Result<CommandOutput> {
    // List ALL worktrees (including active ones).
    let entries = worktree::list_worktrees_with_conn(&ctx.project_root, conn, true)?;

    if args.dry_run {
        let total_bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
        let text = format!(
            "Would delete {} worktree dir(s), freeing {}",
            entries.len(),
            format_bytes(total_bytes),
        );
        let json = json!({
            "dry_run": true,
            "force": true,
            "would_delete_dirs": entries.len(),
            "would_free_bytes": total_bytes,
        });
        return Ok(to_text_or_json(ctx.format, text, json));
    }

    // Confirmation (unless --yes).
    if !args.yes {
        eprint!(
            "WARNING: This will delete ALL worktree directories.\n\
             Any running agents will lose their working directory. Continue? [y/N] "
        );
        io::stderr().flush()?;
        let mut input = String::new();
        let n = io::stdin().read_line(&mut input)?;
        if n == 0 || !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
            let json = json!({ "aborted": true, "reason": "user_declined" });
            return Ok(to_text_or_json(ctx.format, "Aborted.".to_string(), json));
        }
    }

    // Delete all worktree directories.
    let worktrees_base = grove_core::config::grove_dir(&ctx.project_root).join("worktrees");
    let mut dirs_deleted = 0u64;
    let mut freed_bytes = 0u64;
    if worktrees_base.exists() {
        for entry in std::fs::read_dir(&worktrees_base)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let size = dir_size(&path);
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    eprintln!("warning: failed to remove {}: {e}", path.display());
                } else {
                    dirs_deleted += 1;
                    freed_bytes += size;
                }
            }
        }
    }

    // 3. Prune git worktree references.
    if worktree::git_ops::is_git_repo(&ctx.project_root) {
        let _ = worktree::git_ops::git_worktree_prune(&ctx.project_root);
    }

    let text = format!(
        "Force cleanup: deleted {} worktree dir(s), freed {}",
        dirs_deleted,
        format_bytes(freed_bytes),
    );
    let json = json!({
        "force": true,
        "dirs_deleted": dirs_deleted,
        "freed_bytes": freed_bytes,
    });
    Ok(to_text_or_json(ctx.format, text, json))
}

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let ft = entry.file_type().unwrap_or_else(|_| {
                // Fallback — shouldn't happen on macOS/Linux.
                std::fs::symlink_metadata(entry.path())
                    .map(|m| m.file_type())
                    .unwrap_or_else(|_| entry.file_type().unwrap())
            });
            if ft.is_dir() {
                total += dir_size(&entry.path());
            } else {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
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
