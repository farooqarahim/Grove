use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    pub run_id: String,
    pub agent_type: String,
    pub state: String,
    pub worktree_path: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub provider_session_id: Option<String>,
    pub last_heartbeat: Option<String>,
    pub stalled_since: Option<String>,
    pub checkpoint_sha: Option<String>,
    pub parent_checkpoint_sha: Option<String>,
    pub branch: Option<String>,
    pub pid: Option<i64>,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        run_id: r.get(1)?,
        agent_type: r.get(2)?,
        state: r.get(3)?,
        worktree_path: r.get(4)?,
        started_at: r.get(5)?,
        ended_at: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
        provider_session_id: r.get(9)?,
        last_heartbeat: r.get(10)?,
        stalled_since: r.get(11)?,
        checkpoint_sha: r.get(12)?,
        parent_checkpoint_sha: r.get(13)?,
        branch: r.get(14).ok(),
        pid: r.get(15).ok(),
    })
}

pub fn insert(conn: &mut Connection, row: &SessionRow) -> GroveResult<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO sessions
         (id, run_id, agent_type, state, worktree_path, started_at, ended_at, created_at, updated_at, provider_session_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            row.id,
            row.run_id,
            row.agent_type,
            row.state,
            row.worktree_path,
            row.started_at,
            row.ended_at,
            row.created_at,
            row.updated_at,
            row.provider_session_id,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> GroveResult<SessionRow> {
    let row = conn
        .query_row(
            "SELECT id, run_id, agent_type, state, worktree_path,
                    started_at, ended_at, created_at, updated_at, provider_session_id,
                    last_heartbeat, stalled_since, checkpoint_sha, parent_checkpoint_sha
             FROM sessions WHERE id=?1",
            [id],
            map_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("session {id}")))
}

pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<SessionRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, run_id, agent_type, state, worktree_path,
                started_at, ended_at, created_at, updated_at, provider_session_id,
                last_heartbeat, stalled_since, checkpoint_sha, parent_checkpoint_sha
         FROM sessions WHERE run_id=?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([run_id], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Return the most recent `provider_session_id` that is safe to resume for a
/// given conversation. A session is resumable when it reached `state='completed'`
/// *and* the provider recorded a `provider_session_id` (failed/aborted sessions
/// may have left the provider in an unrecoverable state and are skipped).
///
/// Returns `None` when the conversation has no resumable session, in which case
/// callers typically proceed with a fresh provider session.
pub fn latest_resumable_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<Option<String>> {
    let row = conn
        .query_row(
            "SELECT s.provider_session_id
             FROM sessions s
             JOIN runs r ON r.id = s.run_id
             WHERE r.conversation_id = ?1
               AND s.state = 'completed'
               AND s.provider_session_id IS NOT NULL
             ORDER BY COALESCE(s.ended_at, s.updated_at) DESC, s.created_at DESC
             LIMIT 1",
            [conversation_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(row.flatten())
}

pub fn set_state(
    conn: &Connection,
    id: &str,
    state: &str,
    started_at: Option<&str>,
    ended_at: Option<&str>,
    updated_at: &str,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE sessions
         SET state=?1,
             started_at = COALESCE(?2, started_at),
             ended_at   = COALESCE(?3, ended_at),
             updated_at = ?4
         WHERE id=?5",
        params![state, started_at, ended_at, updated_at, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("session {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    struct TestEnv {
        conn: Connection,
        _tmp: tempfile::TempDir,
    }

    fn open_env() -> TestEnv {
        let tmp = tempfile::tempdir().expect("tempdir");
        db::initialize(tmp.path()).expect("db init");
        let conn = db::DbHandle::new(tmp.path()).connect().expect("connect");
        TestEnv { conn, _tmp: tmp }
    }

    fn seed_conversation(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO conversations (id, project_id, state, conversation_kind, remote_registration_state, created_at, updated_at)
             VALUES (?1, 'proj-test', 'active', 'run', 'local_only', '2024-01-01', '2024-01-01')",
            [id],
        )
        .expect("insert conversation");
    }

    fn seed_run(conn: &Connection, run_id: &str, conv_id: &str) {
        conn.execute(
            "INSERT INTO runs (id, conversation_id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, ?2, 'obj', 'completed', 10.0, 0.0, '2024-01-01', '2024-01-01')",
            rusqlite::params![run_id, conv_id],
        )
        .expect("insert run");
    }

    fn seed_session(
        conn: &mut Connection,
        id: &str,
        run_id: &str,
        state: &str,
        provider_session_id: Option<&str>,
        ended_at: Option<&str>,
    ) {
        let row = SessionRow {
            id: id.into(),
            run_id: run_id.into(),
            agent_type: "coder".into(),
            state: state.into(),
            worktree_path: format!("/tmp/{id}"),
            started_at: Some("2024-01-01".into()),
            ended_at: ended_at.map(|s| s.into()),
            created_at: "2024-01-01".into(),
            updated_at: "2024-01-01".into(),
            provider_session_id: provider_session_id.map(|s| s.into()),
            last_heartbeat: None,
            stalled_since: None,
            checkpoint_sha: None,
            parent_checkpoint_sha: None,
            branch: None,
            pid: None,
        };
        insert(conn, &row).expect("insert session");
    }

    #[test]
    fn latest_resumable_returns_none_when_no_sessions() {
        let env = open_env();
        seed_conversation(&env.conn, "conv-1");
        let got = latest_resumable_for_conversation(&env.conn, "conv-1").expect("query");
        assert_eq!(got, None);
    }

    #[test]
    fn latest_resumable_skips_failed_sessions() {
        let mut env = open_env();
        seed_conversation(&env.conn, "conv-2");
        seed_run(&env.conn, "run-2", "conv-2");
        seed_session(
            &mut env.conn,
            "sess-failed",
            "run-2",
            "failed",
            Some("provider-abc"),
            Some("2024-01-02"),
        );
        let got = latest_resumable_for_conversation(&env.conn, "conv-2").expect("query");
        assert_eq!(got, None, "failed sessions must not be resumed");
    }

    #[test]
    fn latest_resumable_skips_completed_without_provider_id() {
        let mut env = open_env();
        seed_conversation(&env.conn, "conv-3");
        seed_run(&env.conn, "run-3", "conv-3");
        seed_session(&mut env.conn, "sess-3", "run-3", "completed", None, None);
        let got = latest_resumable_for_conversation(&env.conn, "conv-3").expect("query");
        assert_eq!(got, None);
    }

    #[test]
    fn latest_resumable_returns_completed_sessions_provider_id() {
        let mut env = open_env();
        seed_conversation(&env.conn, "conv-4");
        seed_run(&env.conn, "run-4", "conv-4");
        seed_session(
            &mut env.conn,
            "sess-4",
            "run-4",
            "completed",
            Some("provider-42"),
            Some("2024-02-01"),
        );
        let got = latest_resumable_for_conversation(&env.conn, "conv-4").expect("query");
        assert_eq!(got, Some("provider-42".into()));
    }

    #[test]
    fn latest_resumable_picks_most_recent_by_ended_at() {
        let mut env = open_env();
        seed_conversation(&env.conn, "conv-5");
        seed_run(&env.conn, "run-5", "conv-5");
        seed_session(
            &mut env.conn,
            "sess-older",
            "run-5",
            "completed",
            Some("provider-older"),
            Some("2024-01-10"),
        );
        seed_session(
            &mut env.conn,
            "sess-newer",
            "run-5",
            "completed",
            Some("provider-newer"),
            Some("2024-03-15"),
        );
        let got = latest_resumable_for_conversation(&env.conn, "conv-5").expect("query");
        assert_eq!(got, Some("provider-newer".into()));
    }

    #[test]
    fn latest_resumable_scopes_to_conversation() {
        let mut env = open_env();
        seed_conversation(&env.conn, "conv-a");
        seed_conversation(&env.conn, "conv-b");
        seed_run(&env.conn, "run-a", "conv-a");
        seed_run(&env.conn, "run-b", "conv-b");
        seed_session(
            &mut env.conn,
            "sess-a",
            "run-a",
            "completed",
            Some("provider-from-a"),
            Some("2024-01-01"),
        );
        seed_session(
            &mut env.conn,
            "sess-b",
            "run-b",
            "completed",
            Some("provider-from-b"),
            Some("2024-02-01"),
        );
        let got_a = latest_resumable_for_conversation(&env.conn, "conv-a").expect("query");
        assert_eq!(got_a, Some("provider-from-a".into()));
        let got_b = latest_resumable_for_conversation(&env.conn, "conv-b").expect("query");
        assert_eq!(got_b, Some("provider-from-b".into()));
    }
}
