use std::process::Command;

use anyhow::Result;
use grove_core::config::{self, GroveConfig};
use grove_core::db;
use serde_json::json;

use crate::cli::DoctorArgs;
use crate::command_context::CommandContext;
use crate::commands::{CommandOutput, to_text_or_json};

pub fn handle(ctx: &CommandContext, args: &DoctorArgs) -> Result<CommandOutput> {
    let mut fixes_applied: Vec<String> = Vec::new();
    let apply_fixes = args.fix || args.fix_all;

    if apply_fixes {
        if !config::config_path(&ctx.project_root).exists() {
            GroveConfig::write_default(&ctx.project_root)?;
            fixes_applied.push("config_created".to_string());
        }

        let db_existed = db::db_path(&ctx.project_root).exists();
        db::initialize(&ctx.project_root)?;
        if !db_existed {
            fixes_applied.push("db_initialized".to_string());
        } else if args.fix_all {
            fixes_applied.push("db_migrations_applied".to_string());
        }
    }

    // ── Core checks ──────────────────────────────────────────────────────────
    let git_ok = check_git();
    let config_ok = config::config_path(&ctx.project_root).exists();
    let db_ok = db::db_path(&ctx.project_root).exists();

    // ── Extended checks (only when DB exists) ────────────────────────────────
    let (sqlite_ok, integrity_detail, fk_violations) = check_sqlite_full(&ctx.project_root);
    let wal_size_kb = check_wal_size(&ctx.project_root);
    let (worktree_sync_ok, orphan_sessions) = check_worktree_sync(&ctx.project_root);
    let stale_ownership = check_stale_ownership(&ctx.project_root);
    let (zombie_sessions, zombie_pids) = check_zombie_sessions(&ctx.project_root);
    let (leaked_count, leaked_event_types) = check_leaked_secrets(&ctx.project_root);

    let mut checks = vec![
        json!({"name": "git",            "status": pass(git_ok)}),
        json!({"name": "config",         "status": pass(config_ok)}),
        json!({"name": "db_exists",      "status": pass(db_ok)}),
        json!({"name": "integrity",      "status": pass(sqlite_ok),   "detail": integrity_detail}),
        json!({"name": "foreign_keys",   "status": pass(fk_violations == 0), "violations": fk_violations}),
        json!({"name": "wal_size_kb",    "status": if wal_size_kb < 100_000 { "pass" } else { "warn" },
                                          "size_kb": wal_size_kb}),
        json!({"name": "worktree_sync",  "status": pass(worktree_sync_ok),
                                          "orphan_sessions": orphan_sessions}),
        json!({"name": "stale_ownership","status": pass(stale_ownership == 0),
                                          "stale_records": stale_ownership}),
        json!({"name": "zombie_sessions","status": pass(zombie_sessions == 0),
                                          "zombie_count": zombie_sessions, "pids": zombie_pids}),
        json!({"name": "secret_scan",    "status": pass(leaked_count == 0),
                                          "leaked_events": leaked_count,
                                          "event_types": leaked_event_types.iter().take(5).collect::<Vec<_>>()}),
    ];

    if apply_fixes {
        checks.push(json!({"name": "fix_config", "status": pass(config_ok)}));
        checks.push(json!({"name": "fix_db",     "status": pass(db_ok)}));
    }

    let critical_ok = git_ok && db_ok && sqlite_ok && fk_violations == 0 && leaked_count == 0;
    let ok = if apply_fixes {
        critical_ok && config_ok
    } else {
        critical_ok
    };

    let json = json!({
        "ok": ok,
        "checks": checks,
        "fixes_applied": fixes_applied
    });

    let fix_label = if args.fix_all {
        "all"
    } else if args.fix {
        "attempted"
    } else {
        "none"
    };
    let text = format!(
        "Doctor\n\
         ok: {ok}\n\
         git: {}\n\
         config: {}\n\
         db_exists: {}\n\
         integrity: {} {integrity_detail}\n\
         foreign_keys: {} ({fk_violations} violations)\n\
         wal_size_kb: {wal_size_kb}\n\
         worktree_sync: {} ({orphan_sessions} orphans)\n\
         stale_ownership: {} ({stale_ownership} stale records)\n\
         zombie_sessions: {} ({zombie_sessions} zombies)\n\
         secret_scan: {} ({leaked_count} leaked events)\n\
         fixes: {fix_label}",
        pass(git_ok),
        pass(config_ok),
        pass(db_ok),
        pass(sqlite_ok),
        pass(fk_violations == 0),
        pass(worktree_sync_ok),
        pass(stale_ownership == 0),
        pass(zombie_sessions == 0),
        pass(leaked_count == 0),
    );

    Ok(to_text_or_json(ctx.format, text, json))
}

fn pass(ok: bool) -> &'static str {
    if ok { "pass" } else { "fail" }
}

fn check_git() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run full SQLite health check: integrity + foreign key violations.
fn check_sqlite_full(project_root: &std::path::Path) -> (bool, String, usize) {
    let db_path = db::db_path(project_root);
    if !db_path.exists() {
        return (false, "db missing".to_string(), 0);
    }
    let handle = db::DbHandle::new(project_root);
    let conn = match handle.connect() {
        Ok(c) => c,
        Err(e) => return (false, e.to_string(), 0),
    };
    match grove_core::db::integrity::check(&conn) {
        Ok(report) => (
            report.integrity_ok && report.foreign_key_violations.is_empty(),
            report.integrity_detail,
            report.foreign_key_violations.len(),
        ),
        Err(e) => (false, e.to_string(), 0),
    }
}

/// Return WAL file size in KB (0 if absent or non-git mode).
fn check_wal_size(project_root: &std::path::Path) -> u64 {
    let wal = db::db_path(project_root).with_extension("db-wal");
    std::fs::metadata(wal).map(|m| m.len() / 1024).unwrap_or(0)
}

/// Check that every running/queued session's worktree_path exists on disk.
/// Returns (all_ok, orphan_count).
fn check_worktree_sync(project_root: &std::path::Path) -> (bool, usize) {
    let handle = db::DbHandle::new(project_root);
    let conn = match handle.connect() {
        Ok(c) => c,
        Err(_) => return (true, 0), // can't check — treat as ok
    };
    let mut stmt = match conn.prepare(
        "SELECT worktree_path FROM sessions
         WHERE state IN ('running','queued','waiting') AND worktree_path IS NOT NULL",
    ) {
        Ok(s) => s,
        Err(_) => return (true, 0),
    };
    let paths: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    let orphans = paths
        .iter()
        .filter(|p| !std::path::Path::new(p).exists())
        .count();
    (orphans == 0, orphans)
}

/// Count ownership_lock records for runs that are no longer active.
fn check_stale_ownership(project_root: &std::path::Path) -> usize {
    let handle = db::DbHandle::new(project_root);
    let conn = match handle.connect() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    conn.query_row(
        "SELECT COUNT(*) FROM ownership_locks
         WHERE run_id NOT IN (
             SELECT id FROM runs
             WHERE state IN ('executing','planning','verifying','merging')
         )",
        [],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0) as usize
}

/// Count active sessions whose stored PID is no longer alive.
/// Returns (zombie_count, list_of_dead_pids).
fn check_zombie_sessions(project_root: &std::path::Path) -> (usize, Vec<i64>) {
    let handle = db::DbHandle::new(project_root);
    let conn = match handle.connect() {
        Ok(c) => c,
        Err(_) => return (0, vec![]),
    };
    let mut stmt = match conn.prepare(
        "SELECT pid FROM sessions
         WHERE state IN ('running','queued') AND pid IS NOT NULL",
    ) {
        Ok(s) => s,
        Err(_) => return (0, vec![]),
    };
    let pids: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();

    let dead: Vec<i64> = pids
        .into_iter()
        .filter(|&pid| !pid_is_alive(pid as u32))
        .collect();
    let count = dead.len();
    (count, dead)
}

/// Scan the most recent 1 000 event payloads for patterns that would be
/// redacted by `redaction::redact`.  Returns `(count, event_types)` where
/// `count` is the number of events with leaked secrets and `event_types` is
/// a deduplicated list of the offending event type names.
fn check_leaked_secrets(project_root: &std::path::Path) -> (usize, Vec<String>) {
    let handle = grove_core::db::DbHandle::new(project_root);
    let conn = match handle.connect() {
        Ok(c) => c,
        Err(_) => return (0, vec![]),
    };
    let mut stmt = match conn.prepare(
        "SELECT event_type, payload_json FROM events
         ORDER BY created_at DESC LIMIT 1000",
    ) {
        Ok(s) => s,
        Err(_) => return (0, vec![]),
    };
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();

    let mut leaking_types: Vec<String> = rows
        .into_iter()
        .filter(|(_, payload)| grove_core::events::redaction::contains_secret(payload))
        .map(|(event_type, _)| event_type)
        .collect();

    leaking_types.dedup();
    let count = leaking_types.len();
    (count, leaking_types)
}

/// Return true if the process with `pid` is alive.
///
/// Uses `kill -0 <pid>` which sends no signal but returns exit code 0 only
/// when the process exists and the caller has permission to signal it.
/// On non-Unix platforms, conservatively returns `true`.
fn pid_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}
