use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::repositories::conversations_repo::ConversationRow;
use crate::db::repositories::messages_repo::MessageRow;
use crate::db::repositories::{conversations_repo, messages_repo};
use crate::errors::{GroveError, GroveResult};

/// A stable namespace UUID for deriving project IDs via UUID v5.
/// Generated once; must never change (would break existing conversations).
const GROVE_PROJECT_NS: Uuid = Uuid::from_bytes([
    0x67, 0x72, 0x6f, 0x76, 0x65, 0x2d, 0x70, 0x72, 0x6f, 0x6a, 0x65, 0x63, 0x74, 0x2d, 0x6e, 0x73,
]);

pub const RUN_CONVERSATION_KIND: &str = "run";
pub const CLI_CONVERSATION_KIND: &str = "cli";
pub const HIVE_LOOM_CONVERSATION_KIND: &str = "hive_loom";

/// Derive a stable project_id from the canonical path of the project root.
///
/// Uses UUID v5 (SHA-1 based, deterministic) so the same path always produces
/// the same project_id across sessions and machines.
pub fn derive_project_id(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let id = Uuid::new_v5(&GROVE_PROJECT_NS, canonical.to_string_lossy().as_bytes());
    id.simple().to_string()
}

/// Resolve which conversation to use for this run.
///
/// - If `conversation_id` is provided, verify it exists and return it.
/// - If `continue_last` is true, find the latest active conversation for the project.
/// - Otherwise, create a new conversation and return its id.
pub fn resolve_conversation(
    conn: &mut Connection,
    project_root: &Path,
    conversation_id: Option<&str>,
    continue_last: bool,
    branch_prefix: Option<&str>,
    session_name: Option<&str>,
    conversation_kind: &str,
) -> GroveResult<String> {
    if !matches!(
        conversation_kind,
        RUN_CONVERSATION_KIND | CLI_CONVERSATION_KIND | HIVE_LOOM_CONVERSATION_KIND
    ) {
        return Err(GroveError::Runtime(format!(
            "unsupported conversation kind '{conversation_kind}'"
        )));
    }

    // Auto-register workspace, user, and project on every conversation resolution.
    let workspace_id = super::workspace::ensure_workspace(conn)?;
    let user_id = super::workspace::ensure_user(conn)?;
    let _project_id_registered =
        super::workspace::ensure_project(conn, project_root, &workspace_id)?;

    let project_id = derive_project_id(project_root);

    // Explicit conversation ID — verify it exists AND belongs to this project.
    if let Some(id) = conversation_id {
        let row = conversations_repo::get(conn, id)?;
        if row.project_id != project_id {
            return Err(crate::errors::GroveError::Runtime(format!(
                "conversation '{id}' belongs to a different project (expected project_id '{project_id}', \
                 found '{}'). Conversations cannot be shared across projects.",
                row.project_id
            )));
        }
        if row.state != "active" {
            return Err(GroveError::Runtime(format!(
                "conversation '{id}' is not active (state: '{}').\
                 Only active conversations can be used for new runs.",
                row.state
            )));
        }
        if row.conversation_kind != conversation_kind {
            return Err(GroveError::Runtime(format!(
                "conversation '{id}' is a '{}' conversation and cannot be used as '{conversation_kind}'",
                row.conversation_kind
            )));
        }
        return Ok(id.to_string());
    }

    // Continue latest active conversation
    if continue_last {
        if let Some(row) = conversations_repo::get_latest_for_project_by_kind(
            conn,
            &project_id,
            conversation_kind,
        )? {
            // Touch updated_at so this conversation stays "latest"
            conversations_repo::set_state(conn, &row.id, "active")?;
            return Ok(row.id);
        }
        // No active conversation found — fall through to create a new one
    }

    // Create a new conversation with all mandatory IDs
    let conv_id = Uuid::new_v4().simple().to_string();
    let now = Utc::now().to_rfc3339();
    let branch_name =
        branch_prefix.map(|prefix| crate::worktree::paths::conv_branch_name_p(prefix, &conv_id));
    let row = ConversationRow {
        id: conv_id.clone(),
        project_id,
        title: session_name.map(|s| s.to_string()),
        state: "active".to_string(),
        conversation_kind: conversation_kind.to_string(),
        cli_provider: None,
        cli_model: None,
        branch_name,
        remote_branch_name: None,
        remote_registration_state: "local_only".to_string(),
        remote_registration_error: None,
        remote_registered_at: None,
        worktree_path: None,
        created_at: now.clone(),
        updated_at: now,
        workspace_id: Some(workspace_id.clone()),
        user_id: Some(user_id),
    };
    conversations_repo::insert(conn, &row)?;

    // Ensure .grove/ is ignored by git so it is never accidentally committed.
    // Done here (new-conversation path) so every project gets protected on first use,
    // regardless of how it was added. Errors are non-fatal.
    crate::worktree::gitignore::ensure_grove_gitignored(project_root);

    Ok(conv_id)
}

pub fn create_cli_conversation(
    conn: &mut Connection,
    project_root: &Path,
    branch_prefix: &str,
    session_name: Option<&str>,
    cli_provider: &str,
    cli_model: Option<&str>,
) -> GroveResult<String> {
    if cli_provider.trim().is_empty() {
        return Err(GroveError::Runtime(
            "CLI conversations require a provider".to_string(),
        ));
    }

    let conv_id = resolve_conversation(
        conn,
        project_root,
        None,
        false,
        Some(branch_prefix),
        session_name,
        CLI_CONVERSATION_KIND,
    )?;

    let n = conn.execute(
        "UPDATE conversations
         SET cli_provider=?1,
             cli_model=?2,
             updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id=?3",
        rusqlite::params![
            cli_provider.trim(),
            cli_model.filter(|model| !model.trim().is_empty()),
            conv_id,
        ],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("conversation {conv_id}")));
    }

    Ok(conv_id)
}

pub fn create_hive_loom_conversation(
    conn: &mut Connection,
    project_root: &Path,
    branch_prefix: &str,
    session_name: Option<&str>,
) -> GroveResult<String> {
    resolve_conversation(
        conn,
        project_root,
        None,
        false,
        Some(branch_prefix),
        session_name,
        HIVE_LOOM_CONVERSATION_KIND,
    )
}

/// Record the user's objective as a "user" message in the conversation.
///
/// The `user_id` is read from the global `~/.grove/user_id` file (already
/// guaranteed to exist after `resolve_conversation` runs `ensure_user`).
pub fn record_user_message(
    conn: &mut Connection,
    conversation_id: &str,
    run_id: &str,
    objective: &str,
) -> GroveResult<()> {
    let user_id = super::workspace::get_or_create_user_id().ok();
    let msg = MessageRow {
        id: format!("msg_{}", Uuid::new_v4().simple()),
        conversation_id: conversation_id.to_string(),
        run_id: Some(run_id.to_string()),
        role: "user".to_string(),
        agent_type: None,
        session_id: None,
        content: objective.to_string(),
        created_at: Utc::now().to_rfc3339(),
        user_id,
    };
    messages_repo::insert(conn, &msg)
}

/// Record an agent's output as an "agent" message in the conversation.
///
/// Content is truncated to 4096 characters to keep the messages table lean.
pub fn record_agent_message(
    conn: &mut Connection,
    conversation_id: &str,
    run_id: &str,
    agent_type: &str,
    session_id: &str,
    content: &str,
) -> GroveResult<()> {
    let truncated: String = content.chars().take(4096).collect();
    let msg = MessageRow {
        id: format!("msg_{}", Uuid::new_v4().simple()),
        conversation_id: conversation_id.to_string(),
        run_id: Some(run_id.to_string()),
        role: "agent".to_string(),
        agent_type: Some(agent_type.to_string()),
        session_id: Some(session_id.to_string()),
        content: truncated,
        created_at: Utc::now().to_rfc3339(),
        user_id: None,
    };
    messages_repo::insert(conn, &msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        let conn = crate::db::DbHandle::new(dir.path()).connect().unwrap();
        (dir, conn)
    }

    #[test]
    fn derive_project_id_is_stable() {
        let dir = tempfile::TempDir::new().unwrap();
        let id1 = derive_project_id(dir.path());
        let id2 = derive_project_id(dir.path());
        assert_eq!(id1, id2);
        assert!(!id1.is_empty());
    }

    #[test]
    fn derive_project_id_differs_for_different_paths() {
        let dir1 = tempfile::TempDir::new().unwrap();
        let dir2 = tempfile::TempDir::new().unwrap();
        let id1 = derive_project_id(dir1.path());
        let id2 = derive_project_id(dir2.path());
        assert_ne!(id1, id2);
    }

    #[test]
    fn resolve_creates_new_conversation() {
        let (dir, mut conn) = test_db();
        let conv_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();
        assert_eq!(conv_id.len(), 32); // plain UUID simple format

        // Verify it exists in DB with branch_name set
        let row = conversations_repo::get(&conn, &conv_id).unwrap();
        assert_eq!(row.state, "active");
        assert_eq!(
            row.branch_name.as_deref(),
            Some(&format!("grove/s_{conv_id}")[..]),
        );
    }

    #[test]
    fn resolve_continues_latest() {
        let (dir, mut conn) = test_db();
        // Create an initial conversation
        let first_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();

        // Continue should find it
        let continued_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            true,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();
        assert_eq!(first_id, continued_id);
    }

    #[test]
    fn resolve_by_explicit_id() {
        let (dir, mut conn) = test_db();
        let conv_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();

        // Resolve by explicit ID
        let resolved = resolve_conversation(
            &mut conn,
            dir.path(),
            Some(&conv_id),
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();
        assert_eq!(resolved, conv_id);
    }

    #[test]
    fn resolve_by_id_fails_for_nonexistent() {
        let (dir, mut conn) = test_db();
        let result = resolve_conversation(
            &mut conn,
            dir.path(),
            Some("nonexistent"),
            false,
            None,
            None,
            RUN_CONVERSATION_KIND,
        );
        assert!(result.is_err());
    }

    #[test]
    fn resolve_continue_creates_new_when_none_active() {
        let (dir, mut conn) = test_db();
        // No conversations exist, continue_last should create a new one
        let conv_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            true,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();
        assert_eq!(conv_id.len(), 32); // plain UUID simple format
    }

    #[test]
    fn record_user_message_roundtrip() {
        let (dir, mut conn) = test_db();
        let conv_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();

        // Insert a run to satisfy FK
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES ('run1', 'test', 'created', 1.0, 0.0, ?1, ?1)",
            [&now],
        ).unwrap();

        record_user_message(&mut conn, &conv_id, "run1", "build a widget").unwrap();

        let msgs = messages_repo::list_for_conversation(&conn, &conv_id, 100).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "build a widget");
    }

    #[test]
    fn record_agent_message_truncates() {
        let (dir, mut conn) = test_db();
        let conv_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();

        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES ('run1', 'test', 'created', 1.0, 0.0, ?1, ?1)",
            [&now],
        ).unwrap();

        let long_content = "x".repeat(8000);
        record_agent_message(
            &mut conn,
            &conv_id,
            "run1",
            "builder",
            "sess1",
            &long_content,
        )
        .unwrap();

        let msgs = messages_repo::list_for_run(&conn, "run1").unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.len(), 4096);
    }

    #[test]
    fn resolve_rejects_conversation_from_different_project() {
        let (dir, mut conn) = test_db();
        // Create a conversation manually with a different project_id
        let now = Utc::now().to_rfc3339();
        let foreign_conv = ConversationRow {
            id: "conv_foreign".to_string(),
            project_id: "some_other_project_id".to_string(),
            title: None,
            state: "active".to_string(),
            conversation_kind: RUN_CONVERSATION_KIND.to_string(),
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
        };
        conversations_repo::insert(&mut conn, &foreign_conv).unwrap();

        // Trying to use this conversation from our project should fail
        let result = resolve_conversation(
            &mut conn,
            dir.path(),
            Some("conv_foreign"),
            false,
            None,
            None,
            RUN_CONVERSATION_KIND,
        );
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("different project"),
            "expected project mismatch error, got: {err_msg}"
        );
    }

    #[test]
    fn resolve_rejects_archived_conversation() {
        let (dir, mut conn) = test_db();
        // Create a conversation in the correct project, then archive it
        let conv_id = resolve_conversation(
            &mut conn,
            dir.path(),
            None,
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        )
        .unwrap();
        conversations_repo::set_state(&conn, &conv_id, "archived").unwrap();

        // Trying to use an archived conversation should fail
        let result = resolve_conversation(
            &mut conn,
            dir.path(),
            Some(&conv_id),
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        );
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not active"),
            "expected state error, got: {err_msg}"
        );
    }

    #[test]
    fn resolve_rejects_mismatched_conversation_kind() {
        let (dir, mut conn) = test_db();
        let cli_conv_id = create_cli_conversation(
            &mut conn,
            dir.path(),
            "grove",
            Some("CLI Session"),
            "codex",
            Some("o4-mini"),
        )
        .unwrap();

        let result = resolve_conversation(
            &mut conn,
            dir.path(),
            Some(&cli_conv_id),
            false,
            Some("grove"),
            None,
            RUN_CONVERSATION_KIND,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be used"));
    }

    #[test]
    fn create_cli_conversation_persists_cli_metadata() {
        let (dir, mut conn) = test_db();
        let conv_id = create_cli_conversation(
            &mut conn,
            dir.path(),
            "grove",
            Some("CLI Session"),
            "codex",
            Some("o4-mini"),
        )
        .unwrap();

        let row = conversations_repo::get(&conn, &conv_id).unwrap();
        assert_eq!(row.conversation_kind, CLI_CONVERSATION_KIND);
        assert_eq!(row.cli_provider.as_deref(), Some("codex"));
        assert_eq!(row.cli_model.as_deref(), Some("o4-mini"));
    }
}
