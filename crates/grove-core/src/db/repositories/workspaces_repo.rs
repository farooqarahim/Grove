use rusqlite::{Connection, OptionalExtension, params};

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceRow {
    pub id: String,
    pub name: Option<String>,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    // LLM selection (added in migration 0015)
    pub credits_usd: f64,
    pub llm_provider: Option<String>,
    pub llm_model: Option<String>,
    /// "user_key" | "workspace_credits"
    pub llm_auth_mode: String,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceRow> {
    Ok(WorkspaceRow {
        id: r.get(0)?,
        name: r.get(1)?,
        state: r.get(2)?,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
        credits_usd: r.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
        llm_provider: r.get(6)?,
        llm_model: r.get(7)?,
        llm_auth_mode: r
            .get::<_, Option<String>>(8)?
            .unwrap_or_else(|| "user_key".to_string()),
    })
}

pub fn insert(conn: &Connection, row: &WorkspaceRow) -> GroveResult<()> {
    conn.execute(
        "INSERT INTO workspaces
             (id, name, state, created_at, updated_at,
              credits_usd, llm_provider, llm_model, llm_auth_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            row.id,
            row.name,
            row.state,
            row.created_at,
            row.updated_at,
            row.credits_usd,
            row.llm_provider,
            row.llm_model,
            row.llm_auth_mode,
        ],
    )?;
    Ok(())
}

pub fn upsert(conn: &Connection, row: &WorkspaceRow) -> GroveResult<()> {
    conn.execute(
        "INSERT INTO workspaces
             (id, name, state, created_at, updated_at,
              credits_usd, llm_provider, llm_model, llm_auth_mode)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET updated_at=excluded.updated_at",
        params![
            row.id,
            row.name,
            row.state,
            row.created_at,
            row.updated_at,
            row.credits_usd,
            row.llm_provider,
            row.llm_model,
            row.llm_auth_mode,
        ],
    )?;
    Ok(())
}

const SELECT_COLS: &str = "id, name, state, created_at, updated_at,
     credits_usd, llm_provider, llm_model, llm_auth_mode";

pub fn get(conn: &Connection, id: &str) -> GroveResult<WorkspaceRow> {
    let row = conn
        .query_row(
            &format!("SELECT {SELECT_COLS} FROM workspaces WHERE id=?1"),
            [id],
            map_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("workspace {id}")))
}

pub fn list(conn: &Connection, limit: i64) -> GroveResult<Vec<WorkspaceRow>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM workspaces ORDER BY updated_at DESC LIMIT ?1"
    ))?;
    let rows = stmt
        .query_map([limit], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn set_state(conn: &Connection, id: &str, state: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE workspaces SET state=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![state, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("workspace {id}")));
    }
    Ok(())
}

pub fn update_name(conn: &Connection, id: &str, name: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE workspaces SET name=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![name, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("workspace {id}")));
    }
    Ok(())
}

/// Persist the workspace LLM selection.
///
/// `auth_mode` must be `"user_key"` or `"workspace_credits"`.
pub fn update_llm_selection(
    conn: &Connection,
    id: &str,
    provider: &str,
    model: Option<&str>,
    auth_mode: &str,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE workspaces
         SET llm_provider=?1, llm_model=?2, llm_auth_mode=?3,
             updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id=?4",
        params![provider, model, auth_mode, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("workspace {id}")));
    }
    Ok(())
}

/// Add `amount_usd` to the workspace credit balance (always positive delta).
///
/// Uses a single `UPDATE … RETURNING` to make the operation atomic.
pub fn add_credits(conn: &Connection, id: &str, amount_usd: f64) -> GroveResult<f64> {
    let new_balance: f64 = conn
        .query_row(
            "UPDATE workspaces
         SET credits_usd = credits_usd + ?1,
             updated_at  = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?2
         RETURNING credits_usd",
            params![amount_usd, id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| GroveError::NotFound(format!("workspace {id}")))?;
    Ok(new_balance)
}

/// Check that `workspace_id` has at least `required_usd` credits, then deduct them.
///
/// Returns `GroveError::InsufficientCredits` if the balance is too low.
/// The check + deduct is a single atomic `UPDATE … WHERE credits_usd >= required … RETURNING`
/// so there is no TOCTOU window between two concurrent runners.
pub fn check_and_deduct_credits(
    conn: &Connection,
    id: &str,
    required_usd: f64,
) -> GroveResult<f64> {
    let new_balance: Option<f64> = conn
        .query_row(
            "UPDATE workspaces
         SET credits_usd = credits_usd - ?1,
             updated_at  = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?2 AND credits_usd >= ?1
         RETURNING credits_usd",
            params![required_usd, id],
            |r| r.get(0),
        )
        .optional()?;

    match new_balance {
        Some(bal) => Ok(bal),
        None => {
            // Row exists but balance was insufficient — read current balance for the error.
            let available: f64 = conn
                .query_row(
                    "SELECT credits_usd FROM workspaces WHERE id=?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| GroveError::NotFound(format!("workspace {id}")))?;
            Err(GroveError::InsufficientCredits {
                available_usd: available,
                required_usd,
            })
        }
    }
}

/// Refund `amount_usd` back to the workspace (used after actual cost is known
/// and was less than the pre-reserved amount).
///
/// Equivalent to adding a negative deduction back; safe to call with 0.
pub fn refund_credits(conn: &Connection, id: &str, amount_usd: f64) -> GroveResult<()> {
    if amount_usd <= 0.0 {
        return Ok(());
    }
    conn.execute(
        "UPDATE workspaces
         SET credits_usd = credits_usd + ?1,
             updated_at  = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?2",
        params![amount_usd, id],
    )?;
    Ok(())
}

pub fn credit_balance(conn: &Connection, id: &str) -> GroveResult<f64> {
    let row = get(conn, id)?;
    Ok(row.credits_usd)
}

pub fn delete(conn: &Connection, id: &str) -> GroveResult<()> {
    let n = conn.execute("DELETE FROM workspaces WHERE id=?1", [id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("workspace {id}")));
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

    fn make_row(id: &str) -> WorkspaceRow {
        let now = Utc::now().to_rfc3339();
        WorkspaceRow {
            id: id.to_string(),
            name: None,
            state: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
            credits_usd: 0.0,
            llm_provider: None,
            llm_model: None,
            llm_auth_mode: "user_key".to_string(),
        }
    }

    #[test]
    fn insert_and_get() {
        let conn = test_db();
        let row = make_row("ws_abc123");
        insert(&conn, &row).unwrap();
        let got = get(&conn, "ws_abc123").unwrap();
        assert_eq!(got.id, "ws_abc123");
        assert_eq!(got.state, "active");
        assert!(got.name.is_none());
        assert_eq!(got.credits_usd, 0.0);
        assert_eq!(got.llm_auth_mode, "user_key");
    }

    #[test]
    fn get_not_found() {
        let conn = test_db();
        let result = get(&conn, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn upsert_idempotent() {
        let conn = test_db();
        let row = make_row("ws_upsert");
        upsert(&conn, &row).unwrap();
        upsert(&conn, &row).unwrap();
        let got = get(&conn, "ws_upsert").unwrap();
        assert_eq!(got.id, "ws_upsert");
    }

    #[test]
    fn list_respects_limit() {
        let conn = test_db();
        for i in 0..5 {
            let mut row = make_row(&format!("ws_{i}"));
            row.updated_at = format!("2024-01-0{}T00:00:00Z", i + 1);
            insert(&conn, &row).unwrap();
        }
        let results = list(&conn, 3).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].id, "ws_4");
    }

    #[test]
    fn set_state_works() {
        let conn = test_db();
        insert(&conn, &make_row("ws_state")).unwrap();
        set_state(&conn, "ws_state", "archived").unwrap();
        let got = get(&conn, "ws_state").unwrap();
        assert_eq!(got.state, "archived");
    }

    #[test]
    fn update_name_works() {
        let conn = test_db();
        insert(&conn, &make_row("ws_name")).unwrap();
        update_name(&conn, "ws_name", "My Workspace").unwrap();
        let got = get(&conn, "ws_name").unwrap();
        assert_eq!(got.name, Some("My Workspace".to_string()));
    }

    #[test]
    fn update_llm_selection_works() {
        let conn = test_db();
        insert(&conn, &make_row("ws_llm")).unwrap();
        update_llm_selection(
            &conn,
            "ws_llm",
            "anthropic",
            Some("claude-sonnet-4-6"),
            "user_key",
        )
        .unwrap();
        let got = get(&conn, "ws_llm").unwrap();
        assert_eq!(got.llm_provider.as_deref(), Some("anthropic"));
        assert_eq!(got.llm_model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(got.llm_auth_mode, "user_key");
    }

    #[test]
    fn add_and_deduct_credits() {
        let conn = test_db();
        insert(&conn, &make_row("ws_credits")).unwrap();

        let bal = add_credits(&conn, "ws_credits", 10.0).unwrap();
        assert!((bal - 10.0).abs() < 1e-9);

        let bal = check_and_deduct_credits(&conn, "ws_credits", 3.0).unwrap();
        assert!((bal - 7.0).abs() < 1e-9);
    }

    #[test]
    fn insufficient_credits_returns_error() {
        let conn = test_db();
        insert(&conn, &make_row("ws_broke")).unwrap();
        // Balance is 0; requesting 1.0 should fail.
        let err = check_and_deduct_credits(&conn, "ws_broke", 1.0).unwrap_err();
        match err {
            GroveError::InsufficientCredits {
                available_usd,
                required_usd,
            } => {
                assert_eq!(available_usd, 0.0);
                assert_eq!(required_usd, 1.0);
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn refund_credits_works() {
        let conn = test_db();
        insert(&conn, &make_row("ws_refund")).unwrap();
        add_credits(&conn, "ws_refund", 5.0).unwrap();
        check_and_deduct_credits(&conn, "ws_refund", 5.0).unwrap();
        // Balance is now 0; refund 2.0 back.
        refund_credits(&conn, "ws_refund", 2.0).unwrap();
        let got = get(&conn, "ws_refund").unwrap();
        assert!((got.credits_usd - 2.0).abs() < 1e-9);
    }

    #[test]
    fn delete_works() {
        let conn = test_db();
        insert(&conn, &make_row("ws_del")).unwrap();
        delete(&conn, "ws_del").unwrap();
        assert!(get(&conn, "ws_del").is_err());
    }

    #[test]
    fn delete_not_found() {
        let conn = test_db();
        assert!(delete(&conn, "nonexistent").is_err());
    }
}
