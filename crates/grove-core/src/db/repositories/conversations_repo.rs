use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConversationRow {
    pub id: String,
    pub project_id: String,
    pub title: Option<String>,
    pub state: String,
    pub conversation_kind: String,
    pub cli_provider: Option<String>,
    pub cli_model: Option<String>,
    pub branch_name: Option<String>,
    pub remote_branch_name: Option<String>,
    pub remote_registration_state: String,
    pub remote_registration_error: Option<String>,
    pub remote_registered_at: Option<String>,
    pub worktree_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub workspace_id: Option<String>,
    pub user_id: Option<String>,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ConversationRow> {
    Ok(ConversationRow {
        id: r.get(0)?,
        project_id: r.get(1)?,
        title: r.get(2)?,
        state: r.get(3)?,
        conversation_kind: r.get(4)?,
        cli_provider: r.get(5)?,
        cli_model: r.get(6)?,
        branch_name: r.get(7)?,
        remote_branch_name: r.get(8)?,
        remote_registration_state: r.get(9)?,
        remote_registration_error: r.get(10)?,
        remote_registered_at: r.get(11)?,
        worktree_path: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
        workspace_id: r.get(15)?,
        user_id: r.get(16)?,
    })
}

pub fn insert(conn: &mut Connection, row: &ConversationRow) -> GroveResult<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO conversations (id, project_id, title, state, conversation_kind, cli_provider, cli_model, branch_name, remote_branch_name, remote_registration_state, remote_registration_error, remote_registered_at, worktree_path, created_at, updated_at, workspace_id, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            row.id,
            row.project_id,
            row.title,
            row.state,
            row.conversation_kind,
            row.cli_provider,
            row.cli_model,
            row.branch_name,
            row.remote_branch_name,
            row.remote_registration_state,
            row.remote_registration_error,
            row.remote_registered_at,
            row.worktree_path,
            row.created_at,
            row.updated_at,
            row.workspace_id,
            row.user_id,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> GroveResult<ConversationRow> {
    let row = conn
        .query_row(
            "SELECT id, project_id, title, state, conversation_kind, cli_provider, cli_model, branch_name, remote_branch_name, remote_registration_state, remote_registration_error, remote_registered_at, worktree_path, created_at, updated_at, workspace_id, user_id
             FROM conversations WHERE id=?1",
            [id],
            map_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("conversation {id}")))
}

/// Fetch multiple conversations by ID in a single query.
pub fn get_batch(
    conn: &Connection,
    ids: &[&str],
) -> GroveResult<std::collections::HashMap<String, ConversationRow>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "SELECT id, project_id, title, state, conversation_kind, cli_provider, cli_model, \
                branch_name, remote_branch_name, remote_registration_state, \
                remote_registration_error, remote_registered_at, worktree_path, created_at, \
                updated_at, workspace_id, user_id \
         FROM conversations WHERE id IN ({})",
        placeholders.join(",")
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    let map = rows.into_iter().map(|r| (r.id.clone(), r)).collect();
    Ok(map)
}

pub fn list_for_project(
    conn: &Connection,
    project_id: &str,
    limit: i64,
) -> GroveResult<Vec<ConversationRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, project_id, title, state, conversation_kind, cli_provider, cli_model, branch_name, remote_branch_name, remote_registration_state, remote_registration_error, remote_registered_at, worktree_path, created_at, updated_at, workspace_id, user_id
         FROM conversations
         WHERE project_id=?1
         ORDER BY updated_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![project_id, limit], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn get_latest_for_project(
    conn: &Connection,
    project_id: &str,
) -> GroveResult<Option<ConversationRow>> {
    get_latest_for_project_by_kind(conn, project_id, "run")
}

pub fn get_latest_for_project_by_kind(
    conn: &Connection,
    project_id: &str,
    conversation_kind: &str,
) -> GroveResult<Option<ConversationRow>> {
    let row = conn
        .query_row(
            "SELECT id, project_id, title, state, conversation_kind, cli_provider, cli_model, branch_name, remote_branch_name, remote_registration_state, remote_registration_error, remote_registered_at, worktree_path, created_at, updated_at, workspace_id, user_id
             FROM conversations
             WHERE project_id=?1 AND state='active' AND conversation_kind=?2
             ORDER BY updated_at DESC
             LIMIT 1",
            params![project_id, conversation_kind],
            map_row,
        )
        .optional()?;
    Ok(row)
}

pub fn set_state(conn: &Connection, id: &str, state: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE conversations SET state=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![state, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("conversation {id}")));
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> GroveResult<()> {
    // Delete messages first (child rows), then the conversation itself.
    conn.execute("DELETE FROM messages WHERE conversation_id=?1", [id])?;
    let n = conn.execute("DELETE FROM conversations WHERE id=?1", [id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("conversation {id}")));
    }
    Ok(())
}

pub fn update_worktree_metadata(
    conn: &Connection,
    id: &str,
    branch_name: &str,
    worktree_path: &str,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE conversations SET branch_name=?1, worktree_path=?2, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?3",
        params![branch_name, worktree_path, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("conversation {id}")));
    }
    Ok(())
}

pub fn update_remote_registration(
    conn: &Connection,
    id: &str,
    state: &str,
    remote_branch_name: Option<&str>,
    remote_registration_error: Option<&str>,
    remote_registered_at: Option<&str>,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE conversations
         SET remote_registration_state=?1,
             remote_branch_name=?2,
             remote_registration_error=?3,
             remote_registered_at=?4,
             updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id=?5",
        params![
            state,
            remote_branch_name,
            remote_registration_error,
            remote_registered_at,
            id,
        ],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("conversation {id}")));
    }
    Ok(())
}

pub fn update_title(conn: &Connection, id: &str, title: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE conversations SET title=?1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?2",
        params![title, id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("conversation {id}")));
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

    fn make_row(id: &str, project_id: &str) -> ConversationRow {
        let now = Utc::now().to_rfc3339();
        ConversationRow {
            id: id.to_string(),
            project_id: project_id.to_string(),
            title: None,
            state: "active".to_string(),
            conversation_kind: "run".to_string(),
            cli_provider: None,
            cli_model: None,
            branch_name: None,
            remote_branch_name: None,
            remote_registration_state: "local_only".to_string(),
            remote_registration_error: None,
            remote_registered_at: None,
            worktree_path: None,
            created_at: now.clone(),
            updated_at: now,
            workspace_id: None,
            user_id: None,
        }
    }

    #[test]
    fn insert_and_get() {
        let mut conn = test_db();
        let row = make_row("conv1", "proj1");
        insert(&mut conn, &row).unwrap();
        let got = get(&conn, "conv1").unwrap();
        assert_eq!(got.id, "conv1");
        assert_eq!(got.project_id, "proj1");
        assert_eq!(got.state, "active");
        assert_eq!(got.conversation_kind, "run");
    }

    #[test]
    fn get_not_found() {
        let conn = test_db();
        let result = get(&conn, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn list_for_project_ordering_and_limit() {
        let mut conn = test_db();
        for i in 0..5 {
            let mut row = make_row(&format!("conv{i}"), "proj1");
            row.updated_at = format!("2024-01-0{}T00:00:00Z", i + 1);
            insert(&mut conn, &row).unwrap();
        }
        let results = list_for_project(&conn, "proj1", 3).unwrap();
        assert_eq!(results.len(), 3);
        // Most recent first
        assert_eq!(results[0].id, "conv4");
    }

    #[test]
    fn get_latest_for_project_returns_most_recent_active() {
        let mut conn = test_db();
        let mut old = make_row("old", "proj1");
        old.updated_at = "2024-01-01T00:00:00Z".to_string();
        insert(&mut conn, &old).unwrap();

        let mut new = make_row("new", "proj1");
        new.updated_at = "2024-06-01T00:00:00Z".to_string();
        insert(&mut conn, &new).unwrap();

        let latest = get_latest_for_project(&conn, "proj1").unwrap().unwrap();
        assert_eq!(latest.id, "new");
    }

    #[test]
    fn get_latest_excludes_archived() {
        let mut conn = test_db();
        let row = make_row("arch", "proj1");
        insert(&mut conn, &row).unwrap();
        set_state(&conn, "arch", "archived").unwrap();

        let latest = get_latest_for_project(&conn, "proj1").unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn get_latest_for_project_by_kind_filters_conversation_kind() {
        let mut conn = test_db();
        let mut run_row = make_row("run_conv", "proj1");
        run_row.updated_at = "2024-01-01T00:00:00Z".to_string();
        insert(&mut conn, &run_row).unwrap();

        let mut cli_row = make_row("cli_conv", "proj1");
        cli_row.conversation_kind = "cli".to_string();
        cli_row.cli_provider = Some("codex".to_string());
        cli_row.updated_at = "2024-06-01T00:00:00Z".to_string();
        insert(&mut conn, &cli_row).unwrap();

        let latest = get_latest_for_project(&conn, "proj1").unwrap().unwrap();
        assert_eq!(latest.id, "run_conv");

        let latest_cli = get_latest_for_project_by_kind(&conn, "proj1", "cli")
            .unwrap()
            .unwrap();
        assert_eq!(latest_cli.id, "cli_conv");
    }

    #[test]
    fn set_state_archive_and_reactivate() {
        let mut conn = test_db();
        let row = make_row("conv1", "proj1");
        insert(&mut conn, &row).unwrap();

        set_state(&conn, "conv1", "archived").unwrap();
        let got = get(&conn, "conv1").unwrap();
        assert_eq!(got.state, "archived");

        set_state(&conn, "conv1", "active").unwrap();
        let got = get(&conn, "conv1").unwrap();
        assert_eq!(got.state, "active");
    }

    #[test]
    fn update_title_works() {
        let mut conn = test_db();
        let row = make_row("conv1", "proj1");
        insert(&mut conn, &row).unwrap();

        update_title(&conn, "conv1", "My Conversation").unwrap();
        let got = get(&conn, "conv1").unwrap();
        assert_eq!(got.title, Some("My Conversation".to_string()));
    }

    #[test]
    fn delete_cascades_messages() {
        let mut conn = test_db();
        let row = make_row("conv_del", "proj1");
        insert(&mut conn, &row).unwrap();

        // Insert a message
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES ('run_del', 'test', 'completed', 1.0, 0.0, ?1, ?1)",
            [&now],
        ).unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, run_id, role, content, created_at)
             VALUES ('msg_del', 'conv_del', 'run_del', 'user', 'hello', ?1)",
            [&now],
        )
        .unwrap();

        delete(&conn, "conv_del").unwrap();
        assert!(get(&conn, "conv_del").is_err());

        // Messages should be gone too
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id='conv_del'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn delete_not_found() {
        let conn = test_db();
        assert!(delete(&conn, "nonexistent").is_err());
    }
}
