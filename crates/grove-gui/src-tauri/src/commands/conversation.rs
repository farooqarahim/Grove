use tauri::State;

use grove_core::db::repositories::conversations_repo::ConversationRow;

use super::{
    CreateConversationResult, emit, ensure_project_is_valid_run_target, project_root_for_id,
    resolve_cli_launch_command,
};
use crate::state::AppState;

// ── Conversations ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_conversations(
    state: State<'_, AppState>,
    limit: i64,
    project_id: Option<String>,
) -> Result<Vec<ConversationRow>, String> {
    let pid = match project_id {
        Some(id) => id,
        None => return Ok(vec![]),
    };
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::conversations_repo::list_for_project(&conn, &pid, limit)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<ConversationRow>, String> {
    grove_core::orchestrator::get_conversation(state.workspace_root(), &id)
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
pub fn create_conversation(
    state: State<'_, AppState>,
    project_id: String,
    session_name: Option<String>,
    conversation_kind: String,
    cli_provider: Option<String>,
    cli_model: Option<String>,
) -> Result<CreateConversationResult, String> {
    if conversation_kind != grove_core::orchestrator::conversation::CLI_CONVERSATION_KIND {
        return Err("create_conversation currently supports only CLI conversations.".to_string());
    }

    let project = project_root_for_id(&state, &project_id)?;
    ensure_project_is_valid_run_target(&project)?;

    let provider_id = cli_provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "CLI conversations require a provider.".to_string())?;
    let project_root = std::path::PathBuf::from(&project.root_path);
    let cfg = grove_core::config::GroveConfig::load_or_create(&project_root)
        .map_err(|e| e.to_string())?;
    let _launch = resolve_cli_launch_command(&project_root, provider_id, cli_model.as_deref())?;

    let conversation_id = {
        let mut conn = state.pool().get().map_err(|e| e.to_string())?;
        grove_core::orchestrator::conversation::create_cli_conversation(
            &mut conn,
            &project_root,
            &cfg.worktree.branch_prefix,
            session_name.as_deref(),
            provider_id,
            cli_model.as_deref(),
        )
        .map_err(|e| e.to_string())?
    };

    let branch_name = grove_core::worktree::paths::conv_branch_name_p(
        &cfg.worktree.branch_prefix,
        &conversation_id,
    );
    let worktree_path = if grove_core::worktree::git_ops::is_git_repo(&project_root)
        && grove_core::worktree::git_ops::has_commits(&project_root)
    {
        let start_point = if grove_core::worktree::git_ops::git_branch_exists(
            &project_root,
            &cfg.project.default_branch,
        )
        .unwrap_or(false)
        {
            cfg.project.default_branch.as_str()
        } else {
            "HEAD"
        };
        grove_core::worktree::git_ops::git_create_branch(&project_root, &branch_name, start_point)
            .map_err(|e| e.to_string())?;
        grove_core::worktree::conversation::ensure_conversation_worktree(
            &project_root,
            &conversation_id,
            &cfg.worktree.branch_prefix,
        )
        .map_err(|e| e.to_string())?
    } else {
        project_root.clone()
    };

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let worktree_display = worktree_path.to_string_lossy().to_string();
    grove_core::db::repositories::conversations_repo::update_worktree_metadata(
        &conn,
        &conversation_id,
        &branch_name,
        &worktree_display,
    )
    .map_err(|e| e.to_string())?;

    Ok(CreateConversationResult { conversation_id })
}

/// Create a Hive Loom conversation for graph-based execution.
///
/// Unlike CLI/chat conversations, hive_loom does not require a provider or model —
/// the graph executor will resolve agents per-step from the graph spec.
#[tauri::command]
pub fn create_hive_loom_conversation(
    state: State<'_, AppState>,
    project_id: String,
    session_name: String,
) -> Result<serde_json::Value, String> {
    let project = project_root_for_id(&state, &project_id)?;
    ensure_project_is_valid_run_target(&project)?;

    let project_root = std::path::PathBuf::from(&project.root_path);
    let cfg = grove_core::config::GroveConfig::load_or_create(&project_root)
        .map_err(|e| e.to_string())?;

    let conversation_id = {
        let mut conn = state.pool().get().map_err(|e| e.to_string())?;
        grove_core::orchestrator::conversation::create_hive_loom_conversation(
            &mut conn,
            &project_root,
            &cfg.worktree.branch_prefix,
            Some(session_name.as_str()),
        )
        .map_err(|e| e.to_string())?
    };

    // Set up branch and worktree (same pattern as CLI/chat conversations)
    let branch_name = grove_core::worktree::paths::conv_branch_name_p(
        &cfg.worktree.branch_prefix,
        &conversation_id,
    );
    let worktree_path = if grove_core::worktree::git_ops::is_git_repo(&project_root)
        && grove_core::worktree::git_ops::has_commits(&project_root)
    {
        let start_point = if grove_core::worktree::git_ops::git_branch_exists(
            &project_root,
            &cfg.project.default_branch,
        )
        .unwrap_or(false)
        {
            cfg.project.default_branch.as_str()
        } else {
            "HEAD"
        };
        grove_core::worktree::git_ops::git_create_branch(&project_root, &branch_name, start_point)
            .map_err(|e| e.to_string())?;
        grove_core::worktree::conversation::ensure_conversation_worktree(
            &project_root,
            &conversation_id,
            &cfg.worktree.branch_prefix,
        )
        .map_err(|e| e.to_string())?
    } else {
        project_root.clone()
    };

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let worktree_display = worktree_path.to_string_lossy().to_string();
    grove_core::db::repositories::conversations_repo::update_worktree_metadata(
        &conn,
        &conversation_id,
        &branch_name,
        &worktree_display,
    )
    .map_err(|e| e.to_string())?;

    // Read back the full conversation row for the frontend
    let row = grove_core::db::repositories::conversations_repo::get(&conn, &conversation_id)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://conversations-changed",
        serde_json::json!({ "project_id": project_id }),
    );

    serde_json::to_value(&row).map_err(|e| e.to_string())
}

// ── Conversation management ──────────────────────────────────────────────────

#[tauri::command]
pub fn update_conversation_title(
    state: State<'_, AppState>,
    id: String,
    title: String,
) -> Result<(), String> {
    grove_core::orchestrator::update_conversation_title(state.workspace_root(), &id, &title)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn archive_conversation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    grove_core::orchestrator::archive_conversation(state.workspace_root(), &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_conversation(state: State<'_, AppState>, id: String) -> Result<(), String> {
    grove_core::orchestrator::delete_conversation(state.workspace_root(), &id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn merge_conversation(
    state: State<'_, AppState>,
    id: String,
) -> Result<grove_core::orchestrator::MergeConversationResult, String> {
    let result = grove_core::orchestrator::merge_conversation(state.workspace_root(), &id)
        .map_err(|e| e.to_string())?;
    if matches!(
        result.outcome.as_str(),
        "merged" | "pr_opened" | "pr_exists"
    ) {
        emit(
            &state.app_handle,
            "grove://conv-merged",
            serde_json::json!({
                "conversation_id": id,
                "outcome": result.outcome,
                "target_branch": result.target_branch,
                "source_branch": result.source_branch,
            }),
        );
    }
    Ok(result)
}

#[tauri::command]
pub async fn rebase_conversation_sync(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<String, String> {
    let project_root = state.workspace_root().to_path_buf();
    let app_handle = state.app_handle.clone();
    let conv_id_for_emit = conversation_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        grove_core::orchestrator::rebase_conversation(&project_root, &conversation_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    emit(
        &app_handle,
        "grove://conv-rebased",
        serde_json::json!({
            "conversation_id": conv_id_for_emit,
        }),
    );
    Ok(result)
}
