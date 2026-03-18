use std::fs;

use anyhow::Result;
use grove_core::config::{self, GroveConfig};
use grove_core::db;
use grove_core::worktree::gitignore::DEFAULT_GITIGNORE;
use serde_json::json;

use crate::cli::InitArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, render_path, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &InitArgs) -> Result<CommandOutput> {
    let grove_dir = config::grove_dir(&ctx.project_root);
    let created = !grove_dir.exists();

    if args.force {
        // Backup existing database before any destructive changes.
        // Non-fatal: warn and continue if the DB doesn't exist yet.
        if grove_core::db::db_path(&ctx.project_root).exists() {
            match grove_core::db::backup(&ctx.project_root) {
                Ok(p) => eprintln!("Database backed up to {}", p.display()),
                Err(e) => eprintln!("warning: could not back up database: {e}"),
            }
        }

        // Guard: refuse to overwrite config while any run is active.
        // An active run holds file locks, open DB connections, and worktrees —
        // overwriting grove.yaml mid-run would change the effective configuration
        // under a live agent, producing unpredictable results.
        let db_path = config::grove_dir(&ctx.project_root).join("grove.db");
        if db_path.exists() {
            let handle = db::DbHandle::from_db_path(db_path);
            if let Ok(conn) = handle.connect() {
                let active_runs: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM runs \
                     WHERE state IN ('executing','planning','verifying','merging')",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if active_runs > 0 {
                    anyhow::bail!(
                        "cannot reinitialise Grove while {active_runs} run(s) are active. \
                         Wait for all runs to finish or abort them first:\n\
                         \n  grove status          # check what is running\
                         \n  grove abort <run-id>  # abort a specific run\
                         \n  grove abort --all     # abort all runs"
                    );
                }
            }
        }
        config::GroveConfig::write_default(&ctx.project_root)?;
    } else {
        GroveConfig::load_or_create(&ctx.project_root)?;
    }

    fs::create_dir_all(grove_dir.join("logs"))?;
    fs::create_dir_all(grove_dir.join("reports"))?;
    fs::create_dir_all(grove_dir.join("checkpoints"))?;
    fs::create_dir_all(grove_dir.join("worktrees"))?;

    // Ensure .grove/ is in .gitignore so `git add -A` never commits Grove's
    // internal database, worktrees, and logs to the project's git history.
    // If the file doesn't exist yet, write the full default template.
    // If it does exist, append .grove/ only if it's not already listed.
    let gitignore_path = ctx.project_root.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, DEFAULT_GITIGNORE)?;
        eprintln!("Created .gitignore");
    } else {
        let existing = fs::read_to_string(&gitignore_path).unwrap_or_default();
        let already_ignored = existing
            .lines()
            .any(|l| matches!(l.trim(), ".grove" | ".grove/"));
        if !already_ignored {
            let mut appended = existing;
            if !appended.ends_with('\n') {
                appended.push('\n');
            }
            appended.push_str("\n# Grove internals\n.grove/\n");
            fs::write(&gitignore_path, appended)?;
            eprintln!("Added .grove/ to .gitignore");
        }
    }

    // If git is available, ensure project_root is a git repo so run output
    // is committed and the next run starts from a versioned baseline.
    // If git is absent, Grove works fine — worktrees use plain directories.
    let git_dir = ctx.project_root.join(".git");
    if grove_core::worktree::git_available() {
        if !git_dir.exists() {
            std::process::Command::new("git")
                .args(["init", "-b", "main"])
                .current_dir(&ctx.project_root)
                .output()
                .ok();
            std::process::Command::new("git")
                .args(["commit", "--allow-empty", "-m", "grove: initial commit"])
                .current_dir(&ctx.project_root)
                .output()
                .ok();
        }

        // Untrack .grove/ if it was accidentally committed before .gitignore
        // covered it. This is idempotent — safe to run every time.
        std::process::Command::new("git")
            .args(["rm", "-r", "--cached", "--ignore-unmatch", ".grove"])
            .current_dir(&ctx.project_root)
            .output()
            .ok();
    }

    let init = db::initialize(&ctx.project_root)?;

    // Auto-register workspace, user, and project so they exist from first init.
    {
        let handle = db::DbHandle::new(&ctx.project_root);
        let conn = handle.connect()?;
        let workspace_id = grove_core::orchestrator::workspace::ensure_workspace(&conn)?;
        grove_core::orchestrator::workspace::ensure_user(&conn)?;
        grove_core::orchestrator::workspace::ensure_project(
            &conn,
            &ctx.project_root,
            &workspace_id,
        )?;
    }

    let json = json!({
      "project_root": render_path(&ctx.project_root),
      "grove_dir": render_path(&grove_dir),
      "db_path": render_path(&init.db_path),
      "schema_version": init.schema_version,
      "created": created
    });

    let text = format!(
        "Initialized Grove\nproject_root: {}\ngrove_dir: {}\ndb_path: {}\nschema_version: {}\ncreated: {}",
        render_path(&ctx.project_root),
        render_path(&grove_dir),
        render_path(&init.db_path),
        init.schema_version,
        created
    );

    Ok(to_text_or_json(ctx.format, text, json))
}
