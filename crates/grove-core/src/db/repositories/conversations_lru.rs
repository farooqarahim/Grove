use rusqlite::{Connection, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SweepCandidate {
    pub id: String,
    pub last_access_at: i64,
    pub cached_size_bytes: Option<i64>,
    pub pinned: bool,
}

/// Update `last_access_at` to `now` for `conv_id` iff the stored value is
/// strictly older (or NULL). Commutative under a monotonic clock.
pub fn touch_last_access(conn: &Connection, conv_id: &str, now: i64) -> GroveResult<()> {
    conn.execute(
        "UPDATE conversations
            SET last_access_at = ?1
          WHERE id = ?2
            AND (last_access_at IS NULL OR last_access_at < ?1)",
        params![now, conv_id],
    )?;
    Ok(())
}

/// Returns every conversation row with its LRU columns.
pub fn list_sweep_candidates(conn: &Connection) -> GroveResult<Vec<SweepCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT id,
                COALESCE(last_access_at, 0) AS last_access_at,
                cached_size_bytes,
                pinned
           FROM conversations
          ORDER BY last_access_at ASC, id ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SweepCandidate {
            id: row.get(0)?,
            last_access_at: row.get(1)?,
            cached_size_bytes: row.get::<_, Option<i64>>(2)?,
            pinned: row.get::<_, i64>(3)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Set or clear the pin flag. Returns the number of rows affected.
pub fn mark_pinned(conn: &Connection, conv_id: &str, pinned: bool) -> GroveResult<usize> {
    let n = conn.execute(
        "UPDATE conversations SET pinned = ?1 WHERE id = ?2",
        params![if pinned { 1i64 } else { 0i64 }, conv_id],
    )?;
    Ok(n)
}

/// Mark a conversation as evicted: stamp `last_access_at` forward and NULL-out
/// the cached size so `list` no longer reports stale disk.
pub fn mark_evicted(conn: &Connection, conv_id: &str, now: i64) -> GroveResult<()> {
    conn.execute(
        "UPDATE conversations
            SET last_access_at = ?1,
                cached_size_bytes = NULL
          WHERE id = ?2",
        params![now, conv_id],
    )?;
    Ok(())
}

/// Set the cached size in bytes. `None` clears the value.
pub fn set_cached_size(conn: &Connection, conv_id: &str, bytes: Option<i64>) -> GroveResult<()> {
    conn.execute(
        "UPDATE conversations SET cached_size_bytes = ?1 WHERE id = ?2",
        params![bytes, conv_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        crate::db::initialize(dir.path()).expect("init");
        let handle = crate::db::DbHandle::new(dir.path());
        let conn = handle.connect().expect("connect");
        (dir, conn)
    }

    fn insert_conv(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO conversations (id, project_id, title, state, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, "p", "t", "active", "2026-04-15T00:00:00Z", "2026-04-15T00:00:00Z"],
        ).unwrap();
    }

    #[test]
    fn touch_last_access_updates_row() {
        let (_d, conn) = fresh_db();
        insert_conv(&conn, "a");
        touch_last_access(&conn, "a", 1_000).unwrap();
        let got: i64 = conn.query_row(
            "SELECT last_access_at FROM conversations WHERE id='a'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(got, 1_000);
    }

    #[test]
    fn touch_with_older_timestamp_is_noop() {
        let (_d, conn) = fresh_db();
        insert_conv(&conn, "a");
        touch_last_access(&conn, "a", 2_000).unwrap();
        touch_last_access(&conn, "a", 1_500).unwrap();
        let got: i64 = conn.query_row(
            "SELECT last_access_at FROM conversations WHERE id='a'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(got, 2_000);
    }

    #[test]
    fn touch_missing_conv_id_is_ok() {
        let (_d, conn) = fresh_db();
        let out = touch_last_access(&conn, "nope", 1_000);
        assert!(out.is_ok());
    }

    #[test]
    fn list_sweep_candidates_returns_all() {
        let (_d, conn) = fresh_db();
        insert_conv(&conn, "a");
        insert_conv(&conn, "b");
        insert_conv(&conn, "c");
        touch_last_access(&conn, "a", 100).unwrap();
        touch_last_access(&conn, "b", 200).unwrap();
        touch_last_access(&conn, "c", 300).unwrap();
        let got = list_sweep_candidates(&conn).unwrap();
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].id, "a");
        assert_eq!(got[2].id, "c");
    }

    #[test]
    fn mark_pinned_toggle() {
        let (_d, conn) = fresh_db();
        insert_conv(&conn, "a");
        assert_eq!(mark_pinned(&conn, "a", true).unwrap(), 1);
        let v: i64 = conn.query_row(
            "SELECT pinned FROM conversations WHERE id='a'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v, 1);
        assert_eq!(mark_pinned(&conn, "a", false).unwrap(), 1);
        let v: i64 = conn.query_row(
            "SELECT pinned FROM conversations WHERE id='a'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(v, 0);
    }

    #[test]
    fn mark_evicted_clears_cached_size() {
        let (_d, conn) = fresh_db();
        insert_conv(&conn, "a");
        set_cached_size(&conn, "a", Some(123_456)).unwrap();
        mark_evicted(&conn, "a", 9_999).unwrap();
        let size: Option<i64> = conn.query_row(
            "SELECT cached_size_bytes FROM conversations WHERE id='a'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert!(size.is_none());
        let last: i64 = conn.query_row(
            "SELECT last_access_at FROM conversations WHERE id='a'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(last, 9_999);
    }
}
