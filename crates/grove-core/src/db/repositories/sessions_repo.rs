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
