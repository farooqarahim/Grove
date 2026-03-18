use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MergeQueueRow {
    pub id: i64,
    pub conversation_id: String,
    pub branch_name: String,
    pub target_branch: String,
    pub status: String,
    pub strategy: String,
    pub pr_url: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MergeQueueRow> {
    Ok(MergeQueueRow {
        id: r.get(0)?,
        conversation_id: r.get(1)?,
        branch_name: r.get(2)?,
        target_branch: r.get(3)?,
        status: r.get(4)?,
        strategy: r.get(5)?,
        pr_url: r.get(6)?,
        error: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

pub fn enqueue(
    conn: &mut Connection,
    conversation_id: &str,
    branch_name: &str,
    target_branch: &str,
    strategy: &str,
    created_at: &str,
) -> GroveResult<i64> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO merge_queue (conversation_id, branch_name, target_branch, status, strategy, created_at, updated_at)
         VALUES (?1, ?2, ?3, 'queued', ?4, ?5, ?5)",
        params![conversation_id, branch_name, target_branch, strategy, created_at],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(id)
}

/// Dequeue the oldest queued entry (FIFO by id). Marks it as `running`.
pub fn dequeue_next(conn: &mut Connection, updated_at: &str) -> GroveResult<Option<MergeQueueRow>> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    let next: Option<MergeQueueRow> = tx
        .query_row(
            "SELECT id, conversation_id, branch_name, target_branch, status, strategy, pr_url, error, created_at, updated_at
             FROM merge_queue WHERE status='queued' ORDER BY id ASC LIMIT 1",
            [],
            map_row,
        )
        .optional()?;

    if let Some(ref row) = next {
        tx.execute(
            "UPDATE merge_queue SET status='running', updated_at=?1 WHERE id=?2",
            params![updated_at, row.id],
        )?;
    }
    tx.commit()?;
    Ok(next)
}

pub fn set_status(
    conn: &Connection,
    id: i64,
    status: &str,
    error: Option<&str>,
    updated_at: &str,
) -> GroveResult<()> {
    conn.execute(
        "UPDATE merge_queue SET status=?1, error=?2, updated_at=?3 WHERE id=?4",
        params![status, error, updated_at, id],
    )?;
    Ok(())
}

pub fn set_pr_url(conn: &Connection, id: i64, pr_url: &str, updated_at: &str) -> GroveResult<()> {
    conn.execute(
        "UPDATE merge_queue SET pr_url=?1, updated_at=?2 WHERE id=?3",
        params![pr_url, updated_at, id],
    )?;
    Ok(())
}

pub fn list_pending(conn: &Connection) -> GroveResult<Vec<MergeQueueRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, branch_name, target_branch, status, strategy, pr_url, error, created_at, updated_at
         FROM merge_queue WHERE status IN ('queued','running') ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], map_row)?.collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn list_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<Vec<MergeQueueRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, branch_name, target_branch, status, strategy, pr_url, error, created_at, updated_at
         FROM merge_queue WHERE conversation_id=?1 ORDER BY id DESC",
    )?;
    let rows = stmt
        .query_map([conversation_id], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}
