use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, Clone)]
pub struct RunRow {
    pub id: String,
    pub objective: String,
    pub state: String,
    pub budget_usd: f64,
    pub cost_used_usd: f64,
    pub publish_status: String,
    pub publish_error: Option<String>,
    pub final_commit_sha: Option<String>,
    pub pr_url: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub conversation_id: Option<String>,
    /// The coding agent / provider used for this run (e.g. `"claude_code"`, `"codex"`).
    pub provider: Option<String>,
    /// The model override that was in effect when the run started (e.g. `"claude-sonnet-4-6"`).
    pub model: Option<String>,
    /// Canonical provider-native thread ID carried across safe detached resumes.
    pub provider_thread_id: Option<String>,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RunRow> {
    Ok(RunRow {
        id: r.get(0)?,
        objective: r.get(1)?,
        state: r.get(2)?,
        budget_usd: r.get(3)?,
        cost_used_usd: r.get(4)?,
        publish_status: r.get(5)?,
        publish_error: r.get(6)?,
        final_commit_sha: r.get(7)?,
        pr_url: r.get(8)?,
        published_at: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
        conversation_id: r.get(12)?,
        provider: r.get(13)?,
        model: r.get(14)?,
        provider_thread_id: r.get(15)?,
    })
}

pub fn insert(conn: &mut Connection, row: &RunRow) -> GroveResult<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, publish_status, \
                           publish_error, final_commit_sha, pr_url, published_at, \
                           provider, model, provider_thread_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            row.id,
            row.objective,
            row.state,
            row.budget_usd,
            row.cost_used_usd,
            row.publish_status,
            row.publish_error,
            row.final_commit_sha,
            row.pr_url,
            row.published_at,
            row.provider,
            row.model,
            row.provider_thread_id,
            row.created_at,
            row.updated_at,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> GroveResult<RunRow> {
    let row = conn
        .query_row(
            "SELECT id, objective, state, budget_usd, cost_used_usd, publish_status, \
                    publish_error, final_commit_sha, pr_url, published_at, \
                    created_at, updated_at, conversation_id, provider, model, provider_thread_id
             FROM runs WHERE id=?1",
            [id],
            map_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("run {id}")))
}

pub fn list(conn: &Connection, limit: i64) -> GroveResult<Vec<RunRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, objective, state, budget_usd, cost_used_usd, publish_status, \
                publish_error, final_commit_sha, pr_url, published_at, \
                created_at, updated_at, conversation_id, provider, model, provider_thread_id
         FROM runs ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn set_state(conn: &Connection, id: &str, state: &str, updated_at: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE runs SET state=?1, updated_at=?2 WHERE id=?3",
        params![state, updated_at, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("run {id}")));
    }
    Ok(())
}

pub fn update_cost(
    conn: &Connection,
    id: &str,
    cost_used_usd: f64,
    updated_at: &str,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE runs SET cost_used_usd=?1, updated_at=?2 WHERE id=?3",
        params![cost_used_usd, updated_at, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("run {id}")));
    }
    Ok(())
}

pub fn update_publish(
    conn: &Connection,
    id: &str,
    publish_status: &str,
    publish_error: Option<&str>,
    final_commit_sha: Option<&str>,
    pr_url: Option<&str>,
    published_at: Option<&str>,
    updated_at: &str,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE runs
         SET publish_status=?1, publish_error=?2, final_commit_sha=?3, pr_url=?4, \
             published_at=?5, updated_at=?6
         WHERE id=?7",
        params![
            publish_status,
            publish_error,
            final_commit_sha,
            pr_url,
            published_at,
            updated_at,
            id
        ],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("run {id}")));
    }
    Ok(())
}

pub fn get_provider_thread_id(conn: &Connection, id: &str) -> GroveResult<Option<String>> {
    let thread_id = conn
        .query_row(
            "SELECT provider_thread_id FROM runs WHERE id=?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    thread_id.ok_or_else(|| GroveError::NotFound(format!("run {id}")))
}

pub fn set_provider_thread_id(
    conn: &Connection,
    id: &str,
    provider_thread_id: Option<&str>,
    updated_at: &str,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE runs SET provider_thread_id=?1, updated_at=?2 WHERE id=?3",
        params![provider_thread_id, updated_at, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("run {id}")));
    }
    Ok(())
}
