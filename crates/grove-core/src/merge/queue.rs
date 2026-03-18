use chrono::Utc;
use rusqlite::Connection;

use crate::db::repositories::merge_queue_repo::{self, MergeQueueRow};
use crate::errors::GroveResult;

/// A pending or active merge entry.
#[derive(Debug, Clone)]
pub struct MergeEntry {
    pub id: i64,
    pub conversation_id: String,
    pub branch_name: String,
    pub target_branch: String,
    pub status: String,
    pub strategy: String,
    pub pr_url: Option<String>,
}

impl From<MergeQueueRow> for MergeEntry {
    fn from(r: MergeQueueRow) -> Self {
        Self {
            id: r.id,
            conversation_id: r.conversation_id,
            branch_name: r.branch_name,
            target_branch: r.target_branch,
            status: r.status,
            strategy: r.strategy,
            pr_url: r.pr_url,
        }
    }
}

/// Add a conversation branch to the merge queue. Returns the new row id.
pub fn enqueue(
    conn: &mut Connection,
    conversation_id: &str,
    branch_name: &str,
    target_branch: &str,
    strategy: &str,
) -> GroveResult<i64> {
    let now = Utc::now().to_rfc3339();
    merge_queue_repo::enqueue(
        conn,
        conversation_id,
        branch_name,
        target_branch,
        strategy,
        &now,
    )
}

/// Atomically dequeue the next queued entry and mark it `running`.
/// Returns `None` if the queue is empty.
pub fn dequeue_next(conn: &mut Connection) -> GroveResult<Option<MergeEntry>> {
    let now = Utc::now().to_rfc3339();
    let row = merge_queue_repo::dequeue_next(conn, &now)?;
    Ok(row.map(MergeEntry::from))
}

/// Mark a merge entry as successfully completed.
pub fn mark_done(conn: &Connection, id: i64) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    merge_queue_repo::set_status(conn, id, "completed", None, &now)
}

/// Mark a merge entry as failed with a reason.
pub fn mark_failed(conn: &Connection, id: i64, reason: &str) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    merge_queue_repo::set_status(conn, id, "failed", Some(reason), &now)
}

/// Mark a merge entry as having a conflict that needs resolution.
pub fn mark_conflict(conn: &Connection, id: i64, files: &[String]) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    let files_str = files.join(", ");
    merge_queue_repo::set_status(conn, id, "conflict", Some(&files_str), &now)
}

/// Set the PR URL on a merge entry (for GitHub strategy).
pub fn set_pr_url(conn: &Connection, id: i64, pr_url: &str) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    merge_queue_repo::set_pr_url(conn, id, pr_url, &now)
}

/// List all pending (queued or running) merge entries.
pub fn list_pending(conn: &Connection) -> GroveResult<Vec<MergeEntry>> {
    let rows = merge_queue_repo::list_pending(conn)?;
    Ok(rows.into_iter().map(MergeEntry::from).collect())
}
