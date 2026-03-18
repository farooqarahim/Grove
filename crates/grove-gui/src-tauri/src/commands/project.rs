use tauri::State;

use grove_core::db::repositories::projects_repo::ProjectRow;
use grove_core::db::repositories::workspaces_repo::WorkspaceRow;

use super::{DoctorResultDto, WorktreeCleanResultDto, WorktreeEntryDto};
use crate::state::AppState;

// ── Project / Workspace ──────────────────────────────────────────────────────

#[tauri::command]
pub fn get_project(state: State<'_, AppState>) -> Result<Option<ProjectRow>, String> {
    // In the centralized model, get_project should NOT auto-create a project
    // from the virtual workspace root. Just return the first active project or None.
    let projects = grove_core::orchestrator::list_projects(state.workspace_root())
        .map_err(|e| e.to_string())?;
    Ok(projects.into_iter().next())
}

#[tauri::command]
pub fn get_workspace(state: State<'_, AppState>) -> Result<Option<WorkspaceRow>, String> {
    grove_core::orchestrator::get_workspace(state.workspace_root())
        .map(Some)
        .or_else(|e| {
            if matches!(e, grove_core::errors::GroveError::NotFound(_)) {
                Ok(None)
            } else {
                Err(e.to_string())
            }
        })
}

#[tauri::command]
pub fn get_workspace_root(state: State<'_, AppState>) -> String {
    state.workspace_root().to_string_lossy().to_string()
}

// ── Workspace / Project management ───────────────────────────────────────────

#[tauri::command]
pub fn update_workspace_name(state: State<'_, AppState>, name: String) -> Result<(), String> {
    grove_core::orchestrator::update_workspace_name(state.workspace_root(), &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_project_name(
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), String> {
    grove_core::orchestrator::update_project_name(state.workspace_root(), &id, &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_project_settings(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<grove_core::db::repositories::projects_repo::ProjectSettings, String> {
    grove_core::orchestrator::get_project_settings(state.workspace_root(), &project_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_project_settings(
    state: State<'_, AppState>,
    project_id: String,
    settings: grove_core::db::repositories::projects_repo::ProjectSettings,
) -> Result<(), String> {
    grove_core::orchestrator::update_project_settings(
        state.workspace_root(),
        &project_id,
        &settings,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn credit_balance(state: State<'_, AppState>) -> Result<f64, String> {
    grove_core::orchestrator::credit_balance(state.workspace_root()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_credits(state: State<'_, AppState>, amount: f64) -> Result<f64, String> {
    grove_core::orchestrator::add_credits(state.workspace_root(), amount).map_err(|e| e.to_string())
}

// ── Projects ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectRow>, String> {
    grove_core::orchestrator::list_projects(state.workspace_root()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    root_path: String,
    name: Option<String>,
) -> Result<ProjectRow, String> {
    grove_core::orchestrator::create_project(state.workspace_root(), &root_path, name.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_project_from_source(
    state: State<'_, AppState>,
    request: grove_core::orchestrator::ProjectCreateRequest,
) -> Result<ProjectRow, String> {
    grove_core::orchestrator::create_project_from_source(state.workspace_root(), request)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_project(state: State<'_, AppState>, id: String) -> Result<(), String> {
    grove_core::orchestrator::archive_project(state.workspace_root(), &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), String> {
    grove_core::orchestrator::delete_project(state.workspace_root(), &id).map_err(|e| e.to_string())
}

// ── Worktrees ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_worktrees(state: State<'_, AppState>) -> Result<Vec<WorktreeEntryDto>, String> {
    let entries = grove_core::worktree::list_worktrees(state.workspace_root(), false)
        .map_err(|e| e.to_string())?;
    Ok(entries
        .iter()
        .map(|e| WorktreeEntryDto {
            session_id: e.session_id.clone(),
            path: e.path.to_string_lossy().to_string(),
            size_bytes: e.size_bytes,
            size_display: e.size_display(),
            run_id: e.run_id.clone(),
            agent_type: e.agent_type.clone(),
            state: e.state.clone(),
            created_at: e.created_at.clone(),
            ended_at: e.ended_at.clone(),
            is_active: e.is_active(),
            conversation_id: e.conversation_id.clone(),
            project_id: e.project_id.clone(),
        })
        .collect())
}

#[tauri::command]
pub fn clean_worktrees(state: State<'_, AppState>) -> Result<WorktreeCleanResultDto, String> {
    let (count, bytes) = grove_core::worktree::delete_finished_worktrees(state.workspace_root())
        .map_err(|e| e.to_string())?;
    Ok(WorktreeCleanResultDto {
        deleted_count: count,
        freed_bytes: bytes,
    })
}

#[tauri::command]
pub fn clean_worktrees_scoped(
    state: State<'_, AppState>,
    project_id: Option<String>,
    conversation_id: Option<String>,
) -> Result<WorktreeCleanResultDto, String> {
    let filter = grove_core::worktree::CleanupFilter {
        project_id,
        conversation_id,
    };
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let (count, bytes) = grove_core::worktree::delete_finished_worktrees_filtered(
        state.workspace_root(),
        &conn,
        &filter,
    )
    .map_err(|e| e.to_string())?;
    Ok(WorktreeCleanResultDto {
        deleted_count: count,
        freed_bytes: bytes,
    })
}

#[tauri::command]
pub fn delete_worktree(state: State<'_, AppState>, session_id: String) -> Result<u64, String> {
    grove_core::worktree::delete_worktree(state.workspace_root(), &session_id)
        .map_err(|e| e.to_string())
}

// ── Doctor / Health Check ────────────────────────────────────────────────────

#[tauri::command]
pub async fn doctor_check(state: State<'_, AppState>) -> Result<DoctorResultDto, String> {
    let workspace_root = state.workspace_root().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let git_ok = std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        let db_path = grove_core::config::db_path(&workspace_root);
        let db_exists = db_path.exists();

        let sqlite_ok = if db_exists {
            grove_core::db::integrity_check(&workspace_root)
                .map(|r| r.eq_ignore_ascii_case("ok"))
                .unwrap_or(false)
        } else {
            false
        };

        let config_ok = grove_core::config::config_path(&workspace_root).exists();

        Ok(DoctorResultDto {
            ok: git_ok && sqlite_ok && config_ok && db_exists,
            git: git_ok,
            sqlite: sqlite_ok,
            config: config_ok,
            db: db_exists,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
