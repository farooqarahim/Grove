use rusqlite::{Connection, TransactionBehavior, params};

use crate::errors::GroveResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub conversation_id: String,
    pub run_id: Option<String>,
    pub role: String,
    pub agent_type: Option<String>,
    pub session_id: Option<String>,
    pub content: String,
    pub created_at: String,
    pub user_id: Option<String>,
}

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
    Ok(MessageRow {
        id: r.get(0)?,
        conversation_id: r.get(1)?,
        run_id: r.get(2)?,
        role: r.get(3)?,
        agent_type: r.get(4)?,
        session_id: r.get(5)?,
        content: r.get(6)?,
        created_at: r.get(7)?,
        user_id: r.get(8)?,
    })
}

pub fn insert(conn: &mut Connection, row: &MessageRow) -> GroveResult<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO messages (id, conversation_id, run_id, role, agent_type, session_id, content, created_at, user_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            row.id,
            row.conversation_id,
            row.run_id,
            row.role,
            row.agent_type,
            row.session_id,
            row.content,
            row.created_at,
            row.user_id,
        ],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn list_for_conversation(
    conn: &Connection,
    conversation_id: &str,
    limit: i64,
) -> GroveResult<Vec<MessageRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, run_id, role, agent_type, session_id, content, created_at, user_id
         FROM messages
         WHERE conversation_id=?1
         ORDER BY created_at ASC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![conversation_id, limit], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn list_for_run(conn: &Connection, run_id: &str) -> GroveResult<Vec<MessageRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, run_id, role, agent_type, session_id, content, created_at, user_id
         FROM messages
         WHERE run_id=?1
         ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map([run_id], map_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_db() -> Connection {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let conn = crate::db::DbHandle::new(dir.path()).connect().unwrap();
        // Insert a conversation to satisfy FK
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO conversations (id, project_id, state, created_at, updated_at)
             VALUES ('conv1', 'proj1', 'active', ?1, ?1)",
            [&now],
        )
        .unwrap();
        // Insert a run to satisfy FK
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES ('run1', 'test', 'completed', 1.0, 0.0, ?1, ?1)",
            [&now],
        )
        .unwrap();
        conn
    }

    fn make_user_msg(id: &str, run_id: &str) -> MessageRow {
        MessageRow {
            id: id.to_string(),
            conversation_id: "conv1".to_string(),
            run_id: Some(run_id.to_string()),
            role: "user".to_string(),
            agent_type: None,
            session_id: None,
            content: format!("user message {id}"),
            created_at: Utc::now().to_rfc3339(),
            user_id: None,
        }
    }

    fn make_agent_msg(id: &str, run_id: &str, agent: &str) -> MessageRow {
        MessageRow {
            id: id.to_string(),
            conversation_id: "conv1".to_string(),
            run_id: Some(run_id.to_string()),
            role: "agent".to_string(),
            agent_type: Some(agent.to_string()),
            session_id: Some(format!("sess_{id}")),
            content: format!("agent response {id}"),
            created_at: Utc::now().to_rfc3339(),
            user_id: None,
        }
    }

    #[test]
    fn insert_and_list_for_conversation() {
        let mut conn = test_db();
        insert(&mut conn, &make_user_msg("m1", "run1")).unwrap();
        insert(&mut conn, &make_agent_msg("m2", "run1", "builder")).unwrap();

        let msgs = list_for_conversation(&conn, "conv1", 100).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "agent");
        assert_eq!(msgs[1].agent_type, Some("builder".to_string()));
    }

    #[test]
    fn list_for_run_filters_correctly() {
        let mut conn = test_db();
        // Insert a second run
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES ('run2', 'test2', 'completed', 1.0, 0.0, ?1, ?1)",
            [&now],
        )
        .unwrap();

        insert(&mut conn, &make_user_msg("m1", "run1")).unwrap();
        insert(&mut conn, &make_user_msg("m2", "run2")).unwrap();
        insert(&mut conn, &make_agent_msg("m3", "run1", "architect")).unwrap();

        let run1_msgs = list_for_run(&conn, "run1").unwrap();
        assert_eq!(run1_msgs.len(), 2);

        let run2_msgs = list_for_run(&conn, "run2").unwrap();
        assert_eq!(run2_msgs.len(), 1);
    }

    #[test]
    fn role_types_user_agent_system() {
        let mut conn = test_db();
        insert(&mut conn, &make_user_msg("m1", "run1")).unwrap();
        insert(&mut conn, &make_agent_msg("m2", "run1", "builder")).unwrap();

        let sys_msg = MessageRow {
            id: "m3".to_string(),
            conversation_id: "conv1".to_string(),
            run_id: Some("run1".to_string()),
            role: "system".to_string(),
            agent_type: None,
            session_id: None,
            content: "system note".to_string(),
            created_at: Utc::now().to_rfc3339(),
            user_id: None,
        };
        insert(&mut conn, &sys_msg).unwrap();

        let msgs = list_for_conversation(&conn, "conv1", 100).unwrap();
        let roles: Vec<&str> = msgs.iter().map(|m| m.role.as_str()).collect();
        assert!(roles.contains(&"user"));
        assert!(roles.contains(&"agent"));
        assert!(roles.contains(&"system"));
    }

    #[test]
    fn list_for_conversation_respects_limit() {
        let mut conn = test_db();
        for i in 0..10 {
            insert(&mut conn, &make_user_msg(&format!("m{i}"), "run1")).unwrap();
        }
        let msgs = list_for_conversation(&conn, "conv1", 3).unwrap();
        assert_eq!(msgs.len(), 3);
    }
}
