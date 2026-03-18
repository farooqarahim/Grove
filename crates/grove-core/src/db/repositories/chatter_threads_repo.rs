use crate::errors::{GroveError, GroveResult};
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatterThreadRow {
    pub id: String,
    pub conversation_id: String,
    pub coding_agent: String,
    pub ordinal: i64,
    pub state: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    /// Provider-native session ID (e.g. Claude `session_id`, Codex `thread_id`).
    /// Persisted so conversations can be resumed after app restart.
    pub provider_session_id: Option<String>,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ChatterThreadRow> {
    Ok(ChatterThreadRow {
        id: r.get(0)?,
        conversation_id: r.get(1)?,
        coding_agent: r.get(2)?,
        ordinal: r.get(3)?,
        state: r.get(4)?,
        started_at: r.get(5)?,
        ended_at: r.get(6)?,
        provider_session_id: r.get(7)?,
    })
}

pub fn insert(conn: &Connection, row: &ChatterThreadRow) -> GroveResult<()> {
    conn.execute(
        "INSERT INTO chatter_threads (id, conversation_id, coding_agent, ordinal, state, started_at, ended_at, provider_session_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![row.id, row.conversation_id, row.coding_agent, row.ordinal, row.state, row.started_at, row.ended_at, row.provider_session_id],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> GroveResult<ChatterThreadRow> {
    conn.query_row(
        "SELECT id, conversation_id, coding_agent, ordinal, state, started_at, ended_at, provider_session_id
         FROM chatter_threads WHERE id=?1",
        [id],
        map_row,
    )
    .optional()?
    .ok_or_else(|| GroveError::NotFound(format!("chatter_thread {id}")))
}

pub fn list_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<Vec<ChatterThreadRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, conversation_id, coding_agent, ordinal, state, started_at, ended_at, provider_session_id
         FROM chatter_threads WHERE conversation_id=?1 ORDER BY ordinal ASC",
    )?;
    let rows = stmt
        .query_map([conversation_id], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn get_latest_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<Option<ChatterThreadRow>> {
    conn.query_row(
        "SELECT id, conversation_id, coding_agent, ordinal, state, started_at, ended_at, provider_session_id
         FROM chatter_threads WHERE conversation_id=?1 ORDER BY ordinal DESC LIMIT 1",
        [conversation_id],
        map_row,
    )
    .optional()
    .map_err(Into::into)
}

pub fn next_ordinal(conn: &Connection, conversation_id: &str) -> GroveResult<i64> {
    let max: Option<i64> = conn
        .query_row(
            "SELECT MAX(ordinal) FROM chatter_threads WHERE conversation_id=?1",
            [conversation_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(max.map(|m| m + 1).unwrap_or(0))
}

pub fn set_provider_session_id(
    conn: &Connection,
    id: &str,
    provider_session_id: &str,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE chatter_threads SET provider_session_id=?1 WHERE id=?2",
        params![provider_session_id, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("chatter_thread {id}")));
    }
    Ok(())
}

pub fn set_ended(conn: &Connection, id: &str, ended_at: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE chatter_threads SET state='ended', ended_at=?1 WHERE id=?2",
        params![ended_at, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("chatter_thread {id}")));
    }
    Ok(())
}

pub fn delete_for_conversation(conn: &Connection, conversation_id: &str) -> GroveResult<u64> {
    let n = conn.execute(
        "DELETE FROM chatter_threads WHERE conversation_id=?1",
        [conversation_id],
    )?;
    Ok(n as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        crate::db::DbHandle::new(dir.path()).connect().unwrap()
    }

    /// Insert a minimal conversation row to satisfy FKs.
    fn seed_conversation(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO conversations (id, project_id, state, conversation_kind, remote_registration_state, created_at, updated_at)
             VALUES (?1, 'proj1', 'active', 'chat', 'none', '2026-03-12T00:00:00Z', '2026-03-12T00:00:00Z')",
            [id],
        ).unwrap();
    }

    fn make_row(id: &str, conv_id: &str, ordinal: i64) -> ChatterThreadRow {
        ChatterThreadRow {
            id: id.to_string(),
            conversation_id: conv_id.to_string(),
            coding_agent: "claude_code".to_string(),
            ordinal,
            state: "active".to_string(),
            started_at: "2026-03-12T00:00:00Z".to_string(),
            ended_at: None,
            provider_session_id: None,
        }
    }

    #[test]
    fn insert_and_get() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let row = make_row("ct1", "conv1", 0);
        insert(&conn, &row).unwrap();
        let got = get(&conn, "ct1").unwrap();
        assert_eq!(got.id, "ct1");
        assert_eq!(got.conversation_id, "conv1");
        assert_eq!(got.coding_agent, "claude_code");
        assert_eq!(got.ordinal, 0);
        assert_eq!(got.state, "active");
        assert!(got.ended_at.is_none());
    }

    #[test]
    fn get_not_found() {
        let conn = test_db();
        let result = get(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("chatter_thread"));
    }

    #[test]
    fn list_for_conversation_ordered() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        seed_conversation(&conn, "conv2");
        insert(&conn, &make_row("ct2", "conv1", 1)).unwrap();
        insert(&conn, &make_row("ct1", "conv1", 0)).unwrap();
        insert(&conn, &make_row("ct3", "conv2", 0)).unwrap();

        let rows = list_for_conversation(&conn, "conv1").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].ordinal, 0);
        assert_eq!(rows[1].ordinal, 1);
    }

    #[test]
    fn list_empty() {
        let conn = test_db();
        let rows = list_for_conversation(&conn, "conv1").unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn latest_for_conversation_returns_highest_ordinal() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        insert(&conn, &make_row("ct1", "conv1", 0)).unwrap();
        insert(&conn, &make_row("ct2", "conv1", 1)).unwrap();

        let latest = get_latest_for_conversation(&conn, "conv1")
            .unwrap()
            .unwrap();
        assert_eq!(latest.id, "ct2");
        assert_eq!(latest.ordinal, 1);
    }

    #[test]
    fn latest_for_conversation_none_when_empty() {
        let conn = test_db();
        let latest = get_latest_for_conversation(&conn, "conv1").unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn next_ordinal_starts_at_zero() {
        let conn = test_db();
        assert_eq!(next_ordinal(&conn, "conv1").unwrap(), 0);
    }

    #[test]
    fn next_ordinal_increments() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        insert(&conn, &make_row("ct1", "conv1", 0)).unwrap();
        assert_eq!(next_ordinal(&conn, "conv1").unwrap(), 1);
        insert(&conn, &make_row("ct2", "conv1", 1)).unwrap();
        assert_eq!(next_ordinal(&conn, "conv1").unwrap(), 2);
    }

    #[test]
    fn set_ended_updates_state() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        insert(&conn, &make_row("ct1", "conv1", 0)).unwrap();
        set_ended(&conn, "ct1", "2026-03-12T01:00:00Z").unwrap();

        let got = get(&conn, "ct1").unwrap();
        assert_eq!(got.state, "ended");
        assert_eq!(got.ended_at.as_deref(), Some("2026-03-12T01:00:00Z"));
    }

    #[test]
    fn set_ended_not_found() {
        let conn = test_db();
        let result = set_ended(&conn, "nonexistent", "2026-03-12T01:00:00Z");
        assert!(result.is_err());
    }

    #[test]
    fn delete_for_conversation_removes_all() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        seed_conversation(&conn, "conv2");
        insert(&conn, &make_row("ct1", "conv1", 0)).unwrap();
        insert(&conn, &make_row("ct2", "conv1", 1)).unwrap();
        insert(&conn, &make_row("ct3", "conv2", 0)).unwrap();

        let deleted = delete_for_conversation(&conn, "conv1").unwrap();
        assert_eq!(deleted, 2);

        let remaining = list_for_conversation(&conn, "conv1").unwrap();
        assert!(remaining.is_empty());

        // conv2's thread is untouched
        let conv2 = list_for_conversation(&conn, "conv2").unwrap();
        assert_eq!(conv2.len(), 1);
    }

    #[test]
    fn delete_for_conversation_returns_zero_when_empty() {
        let conn = test_db();
        let deleted = delete_for_conversation(&conn, "conv1").unwrap();
        assert_eq!(deleted, 0);
    }
}
