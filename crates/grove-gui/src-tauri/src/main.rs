// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod pty;
mod state;

use tauri::Manager;
use tauri::menu::{AboutMetadata, MenuBuilder, PredefinedMenuItem, SubmenuBuilder};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Bootstrap the centralized Grove app runtime (~/.grove/workspaces/<id>/).
    let grove_app = grove_core::app::GroveApp::init().expect("failed to initialize Grove");

    tracing::info!("starting Grove Desktop");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |tauri_app| {
            let handle = tauri_app.handle().clone();

            // ── Native menu with About metadata ──────────────────────────
            let about = AboutMetadata {
                name: Some("Grove".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                copyright: Some("Copyright (c) 2025 GroveHQ".into()),
                authors: None,
                license: Some("Apache-2.0".into()),
                website: Some("https://github.com/farooqarahim/grove".into()),
                website_label: Some("GitHub".into()),
                comments: Some("Local orchestration engine for coordinating coding agents in isolated git worktrees.".into()),
                ..Default::default()
            };

            let app_menu = SubmenuBuilder::new(&handle, "Grove")
                .item(&PredefinedMenuItem::about(&handle, Some("About Grove"), Some(about))?)
                .separator()
                .item(&PredefinedMenuItem::services(&handle, None)?)
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?;

            let edit_menu = SubmenuBuilder::new(&handle, "Edit")
                .undo()
                .redo()
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;

            let window_menu = SubmenuBuilder::new(&handle, "Window")
                .minimize()
                .separator()
                .close_window()
                .build()?;

            let menu = MenuBuilder::new(&handle)
                .item(&app_menu)
                .item(&edit_menu)
                .item(&window_menu)
                .build()?;

            tauri_app.set_menu(menu)?;

            // ── App state ────────────────────────────────────────────────
            let app_state = state::AppState::new(grove_app, handle);
            app_state.spawn_automation_services();
            tauri_app.manage(app_state);
            tauri_app.manage(pty::PtyManager::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap_data,
            commands::get_project,
            commands::get_workspace,
            commands::list_conversations,
            commands::get_conversation,
            commands::list_runs,
            commands::list_runs_for_conversation,
            commands::get_run,
            commands::list_sessions,
            commands::list_plan_steps,
            commands::run_events,
            commands::list_tasks,
            commands::list_tasks_for_conversation,
            commands::queue_task,
            commands::start_run,
            commands::create_conversation,
            commands::create_hive_loom_conversation,
            commands::cancel_task,
            commands::delete_task,
            commands::clear_queue,
            commands::retry_task,
            commands::refresh_queue,
            commands::abort_run,
            commands::list_messages,
            commands::list_run_messages,
            commands::read_session_log,
            commands::list_providers,
            commands::list_models,
            commands::set_api_key,
            commands::remove_api_key,
            commands::is_authenticated,
            commands::get_llm_selection,
            commands::set_llm_selection,
            commands::detect_editors,
            commands::update_workspace_name,
            commands::update_project_name,
            commands::get_project_settings,
            commands::update_project_settings,
            commands::credit_balance,
            commands::add_credits,
            commands::update_conversation_title,
            commands::archive_conversation,
            commands::delete_conversation,
            commands::merge_conversation,
            commands::rebase_conversation_sync,
            commands::resume_run,
            commands::retry_publish_run,
            commands::get_config,
            commands::get_workspace_root,
            commands::list_projects,
            commands::create_project,
            commands::create_project_from_source,
            commands::archive_project,
            commands::delete_project,
            commands::list_worktrees,
            commands::clean_worktrees,
            commands::clean_worktrees_scoped,
            commands::delete_worktree,
            commands::doctor_check,
            commands::list_subtasks,
            commands::list_ownership_locks,
            commands::list_merge_queue,
            commands::get_run_report,
            commands::get_run_report_markdown,
            commands::list_run_files,
            commands::get_file_diff,
            commands::get_right_panel_data,
            commands::get_project_panel_data,
            commands::get_all_file_diffs,
            commands::list_signals,
            commands::mark_signal_read,
            commands::list_checkpoints,
            commands::list_issues,
            commands::create_issue,
            commands::close_issue,
            commands::refresh_issues,
            commands::check_connections,
            commands::connect_provider,
            commands::disconnect_provider,
            commands::list_provider_statuses,
            commands::list_provider_issues,
            commands::search_issues,
            commands::fetch_ready_issues,
            commands::start_run_from_issue,
            commands::get_hooks_config,
            commands::detect_capabilities,
            // Git operations
            commands::git_status_detailed,
            commands::git_stage_files,
            commands::git_unstage_files,
            commands::git_stage_all,
            commands::git_revert_files,
            commands::git_revert_all,
            commands::git_commit,
            commands::git_push,
            commands::git_create_pr,
            commands::publish_changes,
            commands::fork_run_worktree,
            commands::git_merge_run_to_main,
            commands::git_pull,
            commands::git_branch_status,
            commands::git_get_log,
            commands::git_get_latest_commit,
            commands::git_soft_reset,
            commands::git_get_pr_status,
            commands::git_merge_pr,
            commands::git_generate_pr_content,
            // Project-root git operations (no run ID needed)
            commands::git_project_files,
            commands::git_project_status,
            commands::git_project_commit,
            commands::git_project_push,
            commands::git_project_pull,
            commands::git_project_branch_status,
            commands::git_project_diff,
            commands::git_project_is_repo,
            commands::git_project_init,
            commands::git_project_stage_files,
            commands::git_project_unstage_files,
            commands::git_project_stage_all,
            commands::git_project_revert_files,
            commands::git_project_revert_all,
            commands::git_project_get_pr_status,
            commands::git_project_create_pr,
            commands::git_project_soft_reset,
            commands::git_project_generate_pr_content,
            commands::git_project_merge_pr,
            // Issue board
            commands::issue_board,
            commands::issue_get,
            commands::issue_create_native,
            commands::issue_update,
            commands::issue_move,
            commands::issue_assign,
            commands::issue_comment_add,
            commands::issue_list_comments,
            commands::issue_list_activity,
            commands::issue_link_run,
            commands::issue_sync_all,
            commands::issue_sync_provider,
            commands::issue_reopen,
            commands::issue_delete,
            commands::issue_count_open,
            commands::issue_list_provider_projects,
            commands::push_issue_to_provider,
            commands::issue_create_on_provider,
            // PTY / real terminal (new tabbed architecture)
            pty::pty_open,
            pty::pty_write_new,
            pty::pty_resize_new,
            pty::pty_close_new,
            // Agent catalog & default provider
            commands::get_agent_catalog,
            commands::get_pipelines,
            commands::get_default_provider,
            commands::set_default_provider,
            commands::set_agent_enabled,
            commands::get_last_session_info,
            // Phase checkpoints (pipeline gates)
            commands::list_phase_checkpoints,
            commands::get_pending_checkpoint,
            commands::submit_gate_decision,
            // Agent Studio: config CRUD
            commands::list_agent_configs,
            commands::get_agent_config,
            commands::save_agent_config,
            commands::delete_agent_config,
            commands::list_pipeline_configs,
            commands::save_pipeline_config,
            commands::delete_pipeline_config,
            commands::list_skill_configs,
            commands::save_skill_config,
            commands::delete_skill_config,
            commands::preview_agent_prompt,
            // Streaming & Q&A
            commands::get_stream_events,
            commands::get_run_artifacts,
            commands::get_artifact_content,
            commands::send_agent_message,
            commands::list_qa_messages,
            commands::get_app_version,
            // Automations
            commands::create_automation,
            commands::list_automations,
            commands::get_automation,
            commands::update_automation,
            commands::delete_automation,
            commands::toggle_automation,
            commands::list_automation_steps,
            commands::add_automation_step,
            commands::update_automation_step,
            commands::delete_automation_step,
            commands::trigger_automation_manually,
            commands::get_automation_run,
            commands::list_automation_runs,
            commands::get_automation_run_steps,
            commands::cancel_automation_run,
            commands::import_automations_from_files,
            // Grove Graph
            commands::create_graph,
            commands::create_graph_from_spec,
            commands::create_graph_simple,
            commands::get_graph_document,
            commands::save_graph_document,
            commands::retry_document_generation,
            commands::get_graph,
            commands::get_graph_detail,
            commands::list_graphs,
            commands::update_graph_status,
            commands::delete_graph,
            commands::create_graph_phase,
            commands::list_graph_phases,
            commands::update_graph_phase_status,
            commands::delete_graph_phase,
            commands::create_graph_step,
            commands::list_graph_steps,
            commands::update_graph_step_status,
            commands::delete_graph_step,
            commands::populate_graph,
            commands::get_ready_graph_steps,
            commands::get_phases_pending_validation,
            commands::get_step_with_feedback,
            commands::set_graph_config,
            commands::get_graph_config,
            commands::set_active_graph,
            commands::get_active_graph,
            commands::set_graph_execution_mode,
            commands::report_graph_bug,
            commands::get_graph_git_status,
            commands::check_graph_readiness,
            commands::submit_clarification_answer,
            commands::list_graph_clarifications,
            commands::start_graph_loop,
            commands::pause_graph,
            commands::resume_graph,
            commands::abort_graph,
            commands::restart_graph,
            commands::rerun_step,
            commands::rerun_phase,
            // Token filter savings
            commands::get_run_token_savings,
            #[cfg(debug_assertions)]
            commands::detect_debug,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Grove Desktop");
}
