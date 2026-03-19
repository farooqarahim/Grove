use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;

use crate::errors::GroveResult;

use super::git_ops;

// ── Ghost session detection ───────────────────────────────────────────────────

/// Detect sessions whose worktree directory has disappeared from disk (e.g.
/// after a host reboot or manual deletion) while the DB still records them as
/// active (`running`, `executing`, or `planning`).
///
/// Each ghost session is transitioned to `failed` and its parent run is marked
/// `failed` (if not already in a terminal state). Returns the number of ghost
/// sessions recovered.
///
/// This function is called by `sweep_orphaned_resources` on every GC sweep.
pub fn detect_ghost_sessions(conn: &Connection) -> GroveResult<usize> {
    // Fetch all sessions that are in an active state and have a worktree_path.
    let mut stmt = conn.prepare(
        "SELECT s.id, s.run_id, s.worktree_path
         FROM sessions s
         WHERE s.state IN ('running', 'waiting')
           AND s.worktree_path IS NOT NULL
           AND s.worktree_path != ''",
    )?;

    struct GhostCandidate {
        session_id: String,
        run_id: String,
        worktree_path: String,
    }

    let candidates: Vec<GhostCandidate> = stmt
        .query_map([], |r| {
            Ok(GhostCandidate {
                session_id: r.get(0)?,
                run_id: r.get(1)?,
                worktree_path: r.get(2)?,
            })
        })?
        .filter_map(|r| r.ok())
        .filter(|c| !std::path::Path::new(&c.worktree_path).exists())
        .collect();

    if candidates.is_empty() {
        return Ok(0);
    }

    let now = Utc::now().to_rfc3339();
    let mut recovered = 0usize;

    for candidate in &candidates {
        tracing::warn!(
            session_id = %candidate.session_id,
            run_id = %candidate.run_id,
            worktree_path = %candidate.worktree_path,
            "ghost session detected: worktree missing on disk — marking failed"
        );

        // Mark session failed.
        let _ = conn.execute(
            "UPDATE sessions
             SET state = 'failed', ended_at = ?1, updated_at = ?1
             WHERE id = ?2 AND state IN ('running', 'waiting')",
            rusqlite::params![now, candidate.session_id],
        );

        // Mark parent run failed if it's still in an active state.
        let _ = conn.execute(
            "UPDATE runs
             SET state = 'failed', updated_at = ?1
             WHERE id = ?2
               AND state IN ('executing', 'waiting_for_gate', 'planning', 'verifying', 'publishing', 'merging')",
            rusqlite::params![now, candidate.run_id],
        );

        recovered += 1;
    }

    if recovered > 0 {
        tracing::info!(
            count = recovered,
            "ghost session recovery: marked sessions and runs as failed"
        );
    }

    Ok(recovered)
}

/// Delete `grove/*` branches whose worktree directory no longer exists.
///
/// Called after worktree cleanup to catch branches left behind by crashes
/// or incomplete cleanup. Branches are batch-deleted in a single `git branch -D` call.
pub fn cleanup_orphaned_branches(project_root: &Path) {
    let branches = git_ops::git_list_branches(project_root, "grove/*");
    if branches.is_empty() {
        return;
    }

    let worktrees_dir = project_root.join(".grove").join("worktrees");
    let orphaned: Vec<&str> = branches
        .iter()
        .filter(|branch| {
            let session_part = branch.strip_prefix("grove/").unwrap_or(branch);
            !worktrees_dir.join(session_part).exists()
        })
        .map(|s| s.as_str())
        .collect();

    if orphaned.is_empty() {
        return;
    }

    // Batch delete: `git branch -D branch1 branch2 ...`
    let mut args = vec!["branch", "-D"];
    args.extend(orphaned.iter());
    let _ = std::process::Command::new("git")
        .args(&args)
        .current_dir(project_root)
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn setup_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let handle = crate::db::DbHandle::new(dir.path());
        let conn = handle.connect().unwrap();
        // workspace + project rows satisfy FK constraints.
        conn.execute(
            "INSERT INTO workspaces (id, name, state, created_at, updated_at)
             VALUES ('ws1', 'test', 'active', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects (id, workspace_id, name, root_path, state, created_at, updated_at)
             VALUES ('proj1', 'ws1', 'test', '/tmp/test', 'active', datetime('now'), datetime('now'))",
            [],
        ).unwrap();
        (dir, conn)
    }

    fn insert_run(conn: &Connection, run_id: &str) {
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'test', 'executing', 1.0, 0.0, datetime('now'), datetime('now'))",
            rusqlite::params![run_id],
        ).unwrap();
    }

    fn insert_session(conn: &Connection, id: &str, run_id: &str, state: &str, worktree_path: &str) {
        let now = Utc::now().to_rfc3339();
        // Valid session states: queued | running | waiting | completed | failed | killed
        conn.execute(
            "INSERT INTO sessions (id, run_id, agent_type, state, worktree_path, started_at, ended_at, created_at, updated_at)
             VALUES (?1, ?2, 'builder', ?3, ?4, ?5, NULL, ?5, ?5)",
            rusqlite::params![id, run_id, state, worktree_path, now],
        ).unwrap();
    }

    #[test]
    fn detect_ghost_sessions_marks_missing_worktree_as_failed() {
        let (_dir, conn) = setup_db();
        insert_run(&conn, "run1");
        // Valid active state: 'running'. Worktree path does not exist on disk.
        insert_session(
            &conn,
            "sess1",
            "run1",
            "running",
            "/nonexistent/worktree/abc123",
        );

        let recovered = detect_ghost_sessions(&conn).unwrap();
        assert_eq!(recovered, 1, "one ghost session should be recovered");

        // Verify session is now failed.
        let state: String = conn
            .query_row("SELECT state FROM sessions WHERE id = 'sess1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(state, "failed");

        // Verify parent run is now failed.
        let run_state: String = conn
            .query_row("SELECT state FROM runs WHERE id = 'run1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(run_state, "failed");
    }

    #[test]
    fn detect_ghost_sessions_ignores_existing_worktrees() {
        let (dir, conn) = setup_db();
        insert_run(&conn, "run2");
        // Point to a path that DOES exist.
        let wt_path = dir.path().join("real_worktree");
        std::fs::create_dir_all(&wt_path).unwrap();
        insert_session(&conn, "sess2", "run2", "running", wt_path.to_str().unwrap());

        let recovered = detect_ghost_sessions(&conn).unwrap();
        assert_eq!(recovered, 0, "no ghost: worktree exists on disk");

        let state: String = conn
            .query_row("SELECT state FROM sessions WHERE id = 'sess2'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(state, "running", "session must remain in running state");
    }

    #[test]
    fn detect_ghost_sessions_ignores_terminal_sessions() {
        let (_dir, conn) = setup_db();
        insert_run(&conn, "run3");
        // A completed session with a missing worktree must NOT be touched.
        insert_session(&conn, "sess3", "run3", "completed", "/nonexistent/path");

        let recovered = detect_ghost_sessions(&conn).unwrap();
        assert_eq!(recovered, 0, "terminal sessions must not be re-processed");
    }
}
