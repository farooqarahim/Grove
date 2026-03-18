use std::fs;
use std::path::Path;

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::repositories::projects_repo::ProjectRow;
use crate::db::repositories::users_repo::UserRow;
use crate::db::repositories::workspaces_repo::WorkspaceRow;
use crate::db::repositories::{projects_repo, users_repo, workspaces_repo};
use crate::errors::GroveResult;

use super::conversation::derive_project_id;

/// Directory for global Grove state shared across all projects.
fn grove_global_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".grove")
}

/// Read or create the machine-level workspace ID.
///
/// Stored at `~/.grove/workspace_id` as a 64-char hex string
/// (two UUID v4 simple strings concatenated).
pub fn get_or_create_workspace_id() -> GroveResult<String> {
    let global_dir = grove_global_dir();
    let id_path = global_dir.join("workspace_id");

    if let Ok(existing) = fs::read_to_string(&id_path) {
        let trimmed = existing.trim().to_string();
        if trimmed.len() == 64 {
            return Ok(trimmed);
        }
    }

    // Generate: two UUID v4 simple strings concatenated = 64 hex chars.
    let id = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

    fs::create_dir_all(&global_dir)?;
    fs::write(&id_path, &id)?;

    Ok(id)
}

/// Ensure the workspace record exists in the DB.
///
/// Reads the workspace ID from `~/.grove/workspace_id` (creating it if needed),
/// then upserts a row into the `workspaces` table.
///
/// Returns the workspace ID.
pub fn ensure_workspace(conn: &Connection) -> GroveResult<String> {
    let workspace_id = get_or_create_workspace_id()?;
    let now = Utc::now().to_rfc3339();

    workspaces_repo::upsert(
        conn,
        &WorkspaceRow {
            id: workspace_id.clone(),
            name: None,
            state: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
            credits_usd: 0.0,
            llm_provider: None,
            llm_model: None,
            llm_auth_mode: "user_key".to_string(),
        },
    )?;

    Ok(workspace_id)
}

/// Read or create the machine-level user ID.
///
/// Stored at `~/.grove/user_id` as a 64-char hex string
/// (two UUID v4 simple strings concatenated). Only one user per machine.
pub fn get_or_create_user_id() -> GroveResult<String> {
    let global_dir = grove_global_dir();
    let id_path = global_dir.join("user_id");

    if let Ok(existing) = fs::read_to_string(&id_path) {
        let trimmed = existing.trim().to_string();
        if trimmed.len() == 64 {
            return Ok(trimmed);
        }
    }

    let id = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());

    fs::create_dir_all(&global_dir)?;
    fs::write(&id_path, &id)?;

    Ok(id)
}

/// Ensure the user record exists in the DB.
///
/// Reads the user ID from `~/.grove/user_id` (creating it if needed),
/// then upserts a row into the `users` table.
///
/// Returns the user ID.
pub fn ensure_user(conn: &Connection) -> GroveResult<String> {
    let user_id = get_or_create_user_id()?;
    let now = Utc::now().to_rfc3339();

    users_repo::upsert(
        conn,
        &UserRow {
            id: user_id.clone(),
            name: None,
            state: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
        },
    )?;

    Ok(user_id)
}

/// Ensure the project record exists in the DB.
///
/// Derives the project ID from the filesystem path, then upserts a row into
/// the `projects` table. The name defaults to the directory name of
/// `project_root`.
///
/// Returns the project ID.
pub fn ensure_project(
    conn: &Connection,
    project_root: &Path,
    workspace_id: &str,
) -> GroveResult<String> {
    let project_id = derive_project_id(project_root);
    let now = Utc::now().to_rfc3339();

    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());

    let name = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unnamed")
        .to_string();

    projects_repo::upsert(
        conn,
        &ProjectRow {
            id: project_id.clone(),
            workspace_id: workspace_id.to_string(),
            name: Some(name),
            root_path: canonical.to_string_lossy().to_string(),
            state: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
            base_ref: None,
            source_kind: "local".to_string(),
            source_details: None,
        },
    )?;

    Ok(project_id)
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
    fn workspace_id_is_64_chars() {
        let id = get_or_create_workspace_id().unwrap();
        assert_eq!(id.len(), 64, "workspace ID must be 64 hex chars");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn workspace_id_is_stable() {
        let id1 = get_or_create_workspace_id().unwrap();
        let id2 = get_or_create_workspace_id().unwrap();
        assert_eq!(id1, id2, "workspace ID should be stable across calls");
    }

    #[test]
    fn ensure_workspace_idempotent() {
        let (_dir, conn) = test_db();
        let id1 = ensure_workspace(&conn).unwrap();
        let id2 = ensure_workspace(&conn).unwrap();
        assert_eq!(id1, id2);

        let row = workspaces_repo::get(&conn, &id1).unwrap();
        assert_eq!(row.state, "active");
    }

    #[test]
    fn ensure_project_idempotent() {
        let (dir, conn) = test_db();
        let ws_id = ensure_workspace(&conn).unwrap();
        let proj_id1 = ensure_project(&conn, dir.path(), &ws_id).unwrap();
        let proj_id2 = ensure_project(&conn, dir.path(), &ws_id).unwrap();
        assert_eq!(proj_id1, proj_id2);

        let row = projects_repo::get(&conn, &proj_id1).unwrap();
        assert_eq!(row.state, "active");
        assert_eq!(row.workspace_id, ws_id);
    }

    #[test]
    fn ensure_project_name_from_dir() {
        let (dir, conn) = test_db();
        let ws_id = ensure_workspace(&conn).unwrap();
        let proj_id = ensure_project(&conn, dir.path(), &ws_id).unwrap();
        let row = projects_repo::get(&conn, &proj_id).unwrap();
        assert!(row.name.is_some());
        assert!(!row.name.as_ref().unwrap().is_empty());
    }

    #[test]
    fn user_id_is_64_chars() {
        let id = get_or_create_user_id().unwrap();
        assert_eq!(id.len(), 64, "user ID must be 64 hex chars");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn user_id_is_stable() {
        let id1 = get_or_create_user_id().unwrap();
        let id2 = get_or_create_user_id().unwrap();
        assert_eq!(id1, id2, "user ID should be stable across calls");
    }

    #[test]
    fn ensure_user_idempotent() {
        let (_dir, conn) = test_db();
        let id1 = ensure_user(&conn).unwrap();
        let id2 = ensure_user(&conn).unwrap();
        assert_eq!(id1, id2);

        let row = users_repo::get(&conn, &id1).unwrap();
        assert_eq!(row.state, "active");
    }
}
