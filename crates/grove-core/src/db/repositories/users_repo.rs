use rusqlite::{Connection, OptionalExtension, params};

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: String,
    pub name: Option<String>,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<UserRow> {
    Ok(UserRow {
        id: r.get(0)?,
        name: r.get(1)?,
        state: r.get(2)?,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
    })
}

pub fn insert(conn: &Connection, row: &UserRow) -> GroveResult<()> {
    conn.execute(
        "INSERT INTO users (id, name, state, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![row.id, row.name, row.state, row.created_at, row.updated_at],
    )?;
    Ok(())
}

pub fn upsert(conn: &Connection, row: &UserRow) -> GroveResult<()> {
    conn.execute(
        "INSERT INTO users (id, name, state, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at",
        params![row.id, row.name, row.state, row.created_at, row.updated_at],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> GroveResult<UserRow> {
    let row = conn
        .query_row(
            "SELECT id, name, state, created_at, updated_at
             FROM users WHERE id=?1",
            [id],
            map_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("user {id}")))
}

pub fn update_name(conn: &Connection, id: &str, name: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE users SET name=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![name, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("user {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_db() -> Connection {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        crate::db::DbHandle::new(dir.path()).connect().unwrap()
    }

    fn make_row(id: &str) -> UserRow {
        let now = Utc::now().to_rfc3339();
        UserRow {
            id: id.to_string(),
            name: None,
            state: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn insert_and_get() {
        let conn = test_db();
        insert(&conn, &make_row("user_abc")).unwrap();
        let got = get(&conn, "user_abc").unwrap();
        assert_eq!(got.id, "user_abc");
        assert_eq!(got.state, "active");
    }

    #[test]
    fn get_not_found() {
        let conn = test_db();
        assert!(get(&conn, "nonexistent").is_err());
    }

    #[test]
    fn upsert_idempotent() {
        let conn = test_db();
        let row = make_row("user_upsert");
        upsert(&conn, &row).unwrap();
        upsert(&conn, &row).unwrap();
        let got = get(&conn, "user_upsert").unwrap();
        assert_eq!(got.id, "user_upsert");
    }

    #[test]
    fn update_name_works() {
        let conn = test_db();
        insert(&conn, &make_row("user_name")).unwrap();
        update_name(&conn, "user_name", "Alice").unwrap();
        let got = get(&conn, "user_name").unwrap();
        assert_eq!(got.name, Some("Alice".to_string()));
    }
}
