use tauri::State;

use grove_core::db::repositories::grove_graph_repo;

use super::emit;
use crate::state::AppState;

// ── Grove Graph ──────────────────────────────────────────────────────────────

/// Resolve the working directory for graph agents.
///
/// Graphs use the conversation's existing worktree (same as CLI runs).
/// Falls back to the project root if the worktree cannot be ensured
/// (e.g. the project is not a git repo).
fn resolve_graph_workdir(
    conn: &rusqlite::Connection,
    conversation_id: &str,
    workspace_root: &std::path::Path,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let conv = grove_core::db::repositories::conversations_repo::get(conn, conversation_id)
        .map_err(|e| e.to_string())?;

    let project_root =
        match grove_core::db::repositories::projects_repo::get(conn, &conv.project_id) {
            Ok(p) => std::path::PathBuf::from(&p.root_path),
            Err(_) => workspace_root.to_path_buf(),
        };

    // Load config to get the branch prefix.
    let cfg = grove_core::config::GroveConfig::load_or_create(&project_root)
        .map_err(|e| e.to_string())?;

    // Ensure the conversation worktree exists (idempotent, self-healing).
    let is_git = grove_core::worktree::git_ops::is_git_repo(&project_root)
        && grove_core::worktree::git_ops::has_commits(&project_root);

    let workdir = if is_git {
        match grove_core::worktree::conversation::ensure_conversation_worktree(
            &project_root,
            conversation_id,
            &cfg.worktree.branch_prefix,
        ) {
            Ok(wt) => wt,
            Err(e) => {
                tracing::warn!(
                    conversation_id,
                    error = %e,
                    "failed to ensure conversation worktree for graph — falling back to project root"
                );
                project_root.clone()
            }
        }
    } else {
        project_root.clone()
    };

    Ok((workdir, project_root))
}

/// Check if the conversation has a queued graph and start it.
///
/// Called after a graph loop finishes (completes, fails, or is aborted) to
/// automatically start the next graph in the queue. Only one graph per
/// conversation can run at a time.
fn dequeue_next_graph(
    pool: &grove_core::db::DbPool,
    workspace_root: &std::path::Path,
    app_handle: &tauri::AppHandle,
) {
    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "dequeue_next_graph: failed to get DB connection");
            return;
        }
    };

    // Find the next queued graph whose conversation has no running graph.
    // We scan all conversations because the caller may not track which one.
    let queued = match grove_graph_repo::get_next_queued_graph_any_conversation(&conn) {
        Ok(Some(g)) => g,
        Ok(None) => return, // Nothing queued.
        Err(e) => {
            tracing::error!(error = %e, "dequeue_next_graph: failed to query queued graphs");
            return;
        }
    };

    let graph_id = queued.id.clone();
    let conversation_id = queued.conversation_id.clone();

    tracing::info!(
        graph_id = graph_id.as_str(),
        conversation_id = conversation_id.as_str(),
        "dequeuing next graph"
    );

    // Resolve workdir for the queued graph's conversation.
    let (workdir, _project_root) =
        match resolve_graph_workdir(&conn, &conversation_id, workspace_root) {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!(
                    graph_id = graph_id.as_str(),
                    error = e.as_str(),
                    "dequeue: failed to resolve workdir"
                );
                let _ = grove_graph_repo::update_graph_status(&conn, &graph_id, "failed");
                let _ = grove_graph_repo::set_runtime_status(&conn, &graph_id, "idle");
                return;
            }
        };

    let db_path = grove_core::config::db_path(workspace_root);

    // Build provider.
    let cfg = match grove_core::config::GroveConfig::load_or_create(&workdir) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(graph_id = graph_id.as_str(), error = %e, "dequeue: failed to load config");
            let _ = grove_graph_repo::update_graph_status(&conn, &graph_id, "failed");
            let _ = grove_graph_repo::set_runtime_status(&conn, &graph_id, "idle");
            return;
        }
    };
    let provider = match grove_core::orchestrator::build_provider(
        &cfg,
        &workdir,
        queued.provider.as_deref(),
        None,
    ) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(graph_id = graph_id.as_str(), error = %e, "dequeue: failed to build provider");
            let _ = grove_graph_repo::update_graph_status(&conn, &graph_id, "failed");
            let _ = grove_graph_repo::set_runtime_status(&conn, &graph_id, "idle");
            return;
        }
    };

    // Drop the connection before spawning — the blocking thread gets its own.
    drop(conn);

    let pool = pool.clone();
    let app_handle = app_handle.clone();
    let gid = graph_id.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "dequeue: failed to get DB connection");
                return;
            }
        };

        let result = rt.block_on(grove_core::grove_graph::loop_orchestrator::run_graph_loop(
            &conn, &gid, &workdir, &db_path, &provider,
        ));

        match result {
            Ok(outcome) => {
                tracing::info!(graph_id = gid.as_str(), outcome = ?outcome, "dequeued graph loop finished");
            }
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "dequeued graph loop error");
                if let Ok(c) = pool.get() {
                    let _ = grove_graph_repo::update_graph_status(&c, &gid, "failed");
                    let _ = grove_graph_repo::set_runtime_status(&c, &gid, "idle");
                }
            }
        }

        emit(
            &app_handle,
            "grove://graphs-changed",
            serde_json::json!({ "graph_id": &gid }),
        );

        // Recursively dequeue: if this graph also finished, check for the next one.
        dequeue_next_graph(&pool, &workdir, &app_handle);
    });
}

// -- Graph CRUD --

#[tauri::command]
pub fn create_graph(
    state: State<'_, AppState>,
    conversation_id: String,
    title: String,
    description: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let desc = description.as_deref().unwrap_or("");
    let graph_id = grove_graph_repo::insert_graph(&conn, &conversation_id, &title, desc, None)
        .map_err(|e| e.to_string())?;
    let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    serde_json::to_value(&graph).map_err(|e| e.to_string())
}

/// Create a graph from a spec: insert graph, set config, set source doc path,
/// activate it, then spawn pre-planning + graph creation in the background.
/// Returns immediately with the initial graph detail so the frontend can show
/// the graph in a "planning" state while the pipeline runs asynchronously.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_graph_from_spec(
    state: State<'_, AppState>,
    conversation_id: String,
    title: String,
    config_json: String,
    spec_path: Option<String>,
    spec_text: Option<String>,
    provider: Option<String>,
    _model: Option<String>,
) -> Result<serde_json::Value, String> {
    let config: grove_core::grove_graph::GraphConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("invalid config_json: {e}"))?;

    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    // ── 1. Create graph, set config, set source path, activate ───────────────
    let graph_id = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let desc = spec_text.as_deref().unwrap_or("");
        let effective_provider = provider.as_deref().filter(|s| !s.is_empty());
        let gid = grove_graph_repo::insert_graph(
            &conn,
            &conversation_id,
            &title,
            desc,
            effective_provider,
        )
        .map_err(|e| e.to_string())?;

        if let Some(ref path) = spec_path {
            grove_graph_repo::set_source_document_path(&conn, &gid, path)
                .map_err(|e| e.to_string())?;
        }

        grove_graph_repo::set_graph_config(&conn, &gid, &config).map_err(|e| e.to_string())?;

        grove_graph_repo::set_active_graph(&conn, &gid).map_err(|e| e.to_string())?;

        gid
    };

    // ── 2. Resolve workdir from conversation worktree ──────────────────────
    let (_workdir, project_root) = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        resolve_graph_workdir(&conn, &conversation_id, &workspace_root)?
    };

    // ── 3. Snapshot the graph detail for immediate return ─────────────────────
    let detail = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::get_graph_detail(&conn, &graph_id).map_err(|e| e.to_string())?
    };
    let return_value = serde_json::to_value(&detail).map_err(|e| e.to_string())?;

    // ── 4. Spawn pre-planning + graph creation in background ─────────────────
    let gid = graph_id.clone();
    let pool_bg = pool.clone();
    let db_path = grove_core::config::db_path(&workspace_root);
    let provider_override = provider;

    tauri::async_runtime::spawn_blocking(move || {
        let result: Result<(), String> = (|| {
            let cfg = grove_core::config::GroveConfig::load_or_create(&project_root)
                .map_err(|e| e.to_string())?;

            // Re-resolve workdir inside the background thread (avoids moving
            // the original workdir which is not Send-safe across await).
            let bg_conn = pool_bg.get().map_err(|e| e.to_string())?;
            let graph = grove_graph_repo::get_graph(&bg_conn, &gid).map_err(|e| e.to_string())?;
            let (workdir, _) =
                resolve_graph_workdir(&bg_conn, &graph.conversation_id, &workspace_root)?;
            drop(bg_conn);

            let built_provider = grove_core::orchestrator::build_provider(
                &cfg,
                &workdir,
                provider_override.as_deref().filter(|s| !s.is_empty()),
                None,
            )
            .map_err(|e| e.to_string())?;

            let rt = tokio::runtime::Handle::current();
            let conn = pool_bg.get().map_err(|e| e.to_string())?;

            // Pre-planning loop.
            rt.block_on(grove_core::grove_graph::planning::run_pre_planning_loop(
                &conn,
                &gid,
                &workdir,
                &db_path,
                &built_provider,
            ))
            .map_err(|e| e.to_string())?;

            // Graph creation.
            rt.block_on(grove_core::grove_graph::planning::run_graph_creation(
                &conn,
                &gid,
                &workdir,
                &db_path,
                &built_provider,
            ))
            .map_err(|e| e.to_string())?;

            Ok(())
        })();

        if let Err(ref e) = result {
            tracing::warn!(graph_id = gid.as_str(), error = %e, "graph spec pipeline failed");
            if let Ok(conn) = pool_bg.get() {
                let _ = grove_graph_repo::set_graph_pipeline_error(&conn, &gid, Some(e));
                let _ = grove_graph_repo::set_graph_parsing_status(&conn, &gid, "error");
            }
        }

        emit(
            &app_handle,
            "grove://graph-pipeline-complete",
            serde_json::json!({ "graph_id": &gid }),
        );
        emit(
            &app_handle,
            "grove://graphs-changed",
            serde_json::json!({ "graph_id": &gid }),
        );
    });

    Ok(return_value)
}

/// Create a graph from a plain objective string.
///
/// If `has_docs` is true the caller already has a spec document — store `doc_paths`
/// and return immediately.  Otherwise spawn a background agent to *generate* the
/// document, emitting `grove://graph-document-ready` when finished.
#[tauri::command]
pub async fn create_graph_simple(
    state: State<'_, AppState>,
    conversation_id: String,
    objective: String,
    has_docs: bool,
    doc_paths: Option<String>,
    provider: Option<String>,
    model: Option<String>,
) -> Result<serde_json::Value, String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    // ── 1. Derive a short title from the objective ───────────────────────────
    let title = if objective.len() > 60 {
        format!("{}...", &objective[..60])
    } else {
        objective.clone()
    };

    // ── 2. Create graph, set objective, config, activate ─────────────────────
    let graph_id = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let effective_provider = provider.as_deref().filter(|s| !s.is_empty());
        let gid =
            grove_graph_repo::insert_graph(&conn, &conversation_id, &title, "", effective_provider)
                .map_err(|e| e.to_string())?;

        grove_graph_repo::set_graph_objective(&conn, &gid, &objective)
            .map_err(|e| e.to_string())?;

        grove_graph_repo::set_graph_config(
            &conn,
            &gid,
            &grove_core::grove_graph::GraphConfig::default(),
        )
        .map_err(|e| e.to_string())?;

        grove_graph_repo::set_active_graph(&conn, &gid).map_err(|e| e.to_string())?;

        gid
    };

    // ── 3. If user already has docs, store the path and return ───────────────
    if has_docs {
        if let Some(ref dp) = doc_paths {
            let conn = pool.get().map_err(|e| e.to_string())?;
            grove_graph_repo::set_source_document_path(&conn, &graph_id, dp)
                .map_err(|e| e.to_string())?;
        }
        let conn = pool.get().map_err(|e| e.to_string())?;
        let detail =
            grove_graph_repo::get_graph_detail(&conn, &graph_id).map_err(|e| e.to_string())?;
        return serde_json::to_value(&detail).map_err(|e| e.to_string());
    }

    // ── 4. No docs — mark as generating, resolve workdir, return early ───────
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::set_graph_parsing_status(&conn, &graph_id, "generating")
            .map_err(|e| e.to_string())?;
    }

    let (workdir, project_root) = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        resolve_graph_workdir(&conn, &conversation_id, &workspace_root)?
    };

    // Snapshot graph detail *before* spawning so the frontend gets it immediately.
    let detail = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::get_graph_detail(&conn, &graph_id).map_err(|e| e.to_string())?
    };
    let return_value = serde_json::to_value(&detail).map_err(|e| e.to_string())?;

    // ── 5. Spawn blocking thread for document generation ─────────────────────
    let gid = graph_id.clone();
    let obj = objective.clone();
    let provider_override = provider;
    let model_override = model;
    let pool_bg = pool.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let result: Result<(), String> = (|| {
            let cfg = grove_core::config::GroveConfig::load_or_create(&project_root)
                .map_err(|e| e.to_string())?;

            let prov = grove_core::orchestrator::build_provider(
                &cfg,
                &workdir,
                provider_override.as_deref(),
                None,
            )
            .map_err(|e| e.to_string())?;

            let skill_text = grove_core::grove_graph::skill_loader::load_skill(
                &project_root,
                "document-generation",
            );

            // Create docs directory.
            let doc_dir = workdir.join(".grove").join("docs");
            std::fs::create_dir_all(&doc_dir)
                .map_err(|e| format!("failed to create docs dir {}: {e}", doc_dir.display()))?;

            let doc_path = doc_dir.join(format!("{}_spec.md", &gid));
            let doc_path_str = doc_path.to_string_lossy().to_string();

            let instructions = format!(
                "{skill_text}\n\n## Objective\n\n{obj}\n\n## Output\n\nWrite the full spec document to: {doc_path_str}"
            );

            let request = grove_core::providers::ProviderRequest {
                objective: obj.clone(),
                role: "document_generator".to_string(),
                worktree_path: workdir.to_string_lossy().to_string(),
                instructions,
                model: model_override,
                allowed_tools: None,
                timeout_override: None,
                provider_session_id: None,
                log_dir: None,
                grove_session_id: None,
                input_handle_callback: None,
                mcp_config_path: None,
            };

            let _response = prov
                .execute(&request)
                .map_err(|e| format!("document generation agent failed: {e}"))?;

            // Verify the document was created.
            if !doc_path.exists() {
                return Err(format!(
                    "agent completed but spec file was not created at {}",
                    doc_path.display()
                ));
            }

            // Store the document path.
            let conn = pool_bg.get().map_err(|e| e.to_string())?;
            grove_graph_repo::set_source_document_path(&conn, &gid, &doc_path_str)
                .map_err(|e| e.to_string())?;

            // Derive title from first markdown heading.
            if let Ok(content) = std::fs::read_to_string(&doc_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(heading) = trimmed.strip_prefix("# ") {
                        let heading = heading.trim();
                        if !heading.is_empty() {
                            let _ = grove_graph_repo::set_graph_title(&conn, &gid, heading);
                            break;
                        }
                    }
                }
            }

            grove_graph_repo::set_graph_parsing_status(&conn, &gid, "draft_ready")
                .map_err(|e| e.to_string())?;

            Ok(())
        })();

        if let Err(ref e) = result {
            tracing::warn!(graph_id = gid.as_str(), error = %e, "document generation failed");
            if let Ok(conn) = pool_bg.get() {
                let _ = grove_graph_repo::set_graph_pipeline_error(&conn, &gid, Some(e));
                let _ = grove_graph_repo::set_graph_parsing_status(&conn, &gid, "error");
            }
        }

        emit(
            &app_handle,
            "grove://graph-document-ready",
            serde_json::json!({ "graph_id": &gid }),
        );
    });

    Ok(return_value)
}

/// Read the generated/attached spec document for a graph.
#[tauri::command]
pub async fn get_graph_document(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let pool = state.pool().clone();
    let workspace_root = state.workspace_root().to_path_buf();

    let conn = pool.get().map_err(|e| e.to_string())?;
    let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;

    let doc_rel = graph
        .source_document_path
        .as_deref()
        .ok_or_else(|| "graph has no source_document_path".to_string())?;

    // Resolve path — could be absolute or relative to the workdir.
    let doc_path = {
        let p = std::path::Path::new(doc_rel);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            let (workdir, _project_root) =
                resolve_graph_workdir(&conn, &graph.conversation_id, &workspace_root)?;
            workdir.join(p)
        }
    };

    let content = std::fs::read_to_string(&doc_path)
        .map_err(|e| format!("failed to read document at {}: {e}", doc_path.display()))?;

    Ok(serde_json::json!({
        "title": graph.title,
        "content": content,
        "path": doc_path.to_string_lossy(),
    }))
}

/// Save edits to a graph's spec document, then kick off the pre-planning +
/// graph-creation pipeline in the background.
#[tauri::command]
pub async fn save_graph_document(
    state: State<'_, AppState>,
    graph_id: String,
    title: String,
    content: String,
) -> Result<serde_json::Value, String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    // ── 1. Resolve the document path and write ───────────────────────────────
    let (workdir, _project_root) = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
        resolve_graph_workdir(&conn, &graph.conversation_id, &workspace_root)?
    };

    let doc_path = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
        let doc_rel = graph
            .source_document_path
            .as_deref()
            .ok_or_else(|| "graph has no source_document_path".to_string())?;
        let p = std::path::Path::new(doc_rel);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            workdir.join(p)
        }
    };

    std::fs::write(&doc_path, &content)
        .map_err(|e| format!("failed to write document at {}: {e}", doc_path.display()))?;

    // ── 2. Update DB: title, parsing_status, clear error ─────────────────────
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::set_graph_title(&conn, &graph_id, &title).map_err(|e| e.to_string())?;
        grove_graph_repo::set_graph_parsing_status(&conn, &graph_id, "pending")
            .map_err(|e| e.to_string())?;
        grove_graph_repo::set_graph_pipeline_error(&conn, &graph_id, None)
            .map_err(|e| e.to_string())?;
    }

    // ── 3. Snapshot detail for immediate return ──────────────────────────────
    let detail = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::get_graph_detail(&conn, &graph_id).map_err(|e| e.to_string())?
    };
    let return_value = serde_json::to_value(&detail).map_err(|e| e.to_string())?;

    // ── 4. Spawn pre-planning + graph creation ───────────────────────────────
    let gid = graph_id.clone();
    let pool_bg = pool.clone();
    let db_path = grove_core::config::db_path(&workspace_root);

    tauri::async_runtime::spawn_blocking(move || {
        let result: Result<(), String> = (|| {
            let cfg = grove_core::config::GroveConfig::load_or_create(&workdir)
                .map_err(|e| e.to_string())?;
            let provider = grove_core::orchestrator::build_provider(&cfg, &workdir, None, None)
                .map_err(|e| e.to_string())?;

            let rt = tokio::runtime::Handle::current();
            let conn = pool_bg.get().map_err(|e| e.to_string())?;

            rt.block_on(grove_core::grove_graph::planning::run_pre_planning_loop(
                &conn, &gid, &workdir, &db_path, &provider,
            ))
            .map_err(|e| e.to_string())?;

            rt.block_on(grove_core::grove_graph::planning::run_graph_creation(
                &conn, &gid, &workdir, &db_path, &provider,
            ))
            .map_err(|e| e.to_string())?;

            Ok(())
        })();

        if let Err(ref e) = result {
            tracing::warn!(graph_id = gid.as_str(), error = %e, "save_graph_document pipeline failed");
            if let Ok(conn) = pool_bg.get() {
                let _ = grove_graph_repo::set_graph_pipeline_error(&conn, &gid, Some(e));
                let _ = grove_graph_repo::set_graph_parsing_status(&conn, &gid, "error");
            }
        }

        emit(
            &app_handle,
            "grove://graph-pipeline-complete",
            serde_json::json!({ "graph_id": &gid }),
        );
    });

    Ok(return_value)
}

/// Retry document generation for a graph that previously failed.
#[tauri::command]
pub async fn retry_document_generation(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    // ── 1. Verify the graph is in error state ────────────────────────────────
    let (objective, conversation_id) = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;

        if graph.parsing_status != "error" {
            return Err(format!(
                "cannot retry: graph parsing_status is '{}', expected 'error'",
                graph.parsing_status
            ));
        }

        let obj = graph.objective.or(graph.description).unwrap_or_default();

        (obj, graph.conversation_id.clone())
    };

    // ── 2. Clear error, set generating ───────────────────────────────────────
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::set_graph_pipeline_error(&conn, &graph_id, None)
            .map_err(|e| e.to_string())?;
        grove_graph_repo::set_graph_parsing_status(&conn, &graph_id, "generating")
            .map_err(|e| e.to_string())?;
    }

    // ── 3. Resolve workdir ───────────────────────────────────────────────────
    let (workdir, project_root) = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        resolve_graph_workdir(&conn, &conversation_id, &workspace_root)?
    };

    // ── 4. Return detail immediately ─────────────────────────────────────────
    let detail = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::get_graph_detail(&conn, &graph_id).map_err(|e| e.to_string())?
    };
    let return_value = serde_json::to_value(&detail).map_err(|e| e.to_string())?;

    // ── 5. Spawn same agent logic as create_graph_simple ─────────────────────
    let gid = graph_id.clone();
    let obj = objective.clone();
    let pool_bg = pool.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let result: Result<(), String> = (|| {
            let cfg = grove_core::config::GroveConfig::load_or_create(&project_root)
                .map_err(|e| e.to_string())?;

            let prov = grove_core::orchestrator::build_provider(&cfg, &workdir, None, None)
                .map_err(|e| e.to_string())?;

            let skill_text = grove_core::grove_graph::skill_loader::load_skill(
                &project_root,
                "document-generation",
            );

            let doc_dir = workdir.join(".grove").join("docs");
            std::fs::create_dir_all(&doc_dir)
                .map_err(|e| format!("failed to create docs dir {}: {e}", doc_dir.display()))?;

            let doc_path = doc_dir.join(format!("{}_spec.md", &gid));
            let doc_path_str = doc_path.to_string_lossy().to_string();

            let instructions = format!(
                "{skill_text}\n\n## Objective\n\n{obj}\n\n## Output\n\nWrite the full spec document to: {doc_path_str}"
            );

            let request = grove_core::providers::ProviderRequest {
                objective: obj.clone(),
                role: "document_generator".to_string(),
                worktree_path: workdir.to_string_lossy().to_string(),
                instructions,
                model: None,
                allowed_tools: None,
                timeout_override: None,
                provider_session_id: None,
                log_dir: None,
                grove_session_id: None,
                input_handle_callback: None,
                mcp_config_path: None,
            };

            let _response = prov
                .execute(&request)
                .map_err(|e| format!("document generation agent failed: {e}"))?;

            if !doc_path.exists() {
                return Err(format!(
                    "agent completed but spec file was not created at {}",
                    doc_path.display()
                ));
            }

            let conn = pool_bg.get().map_err(|e| e.to_string())?;
            grove_graph_repo::set_source_document_path(&conn, &gid, &doc_path_str)
                .map_err(|e| e.to_string())?;

            if let Ok(content) = std::fs::read_to_string(&doc_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(heading) = trimmed.strip_prefix("# ") {
                        let heading = heading.trim();
                        if !heading.is_empty() {
                            let _ = grove_graph_repo::set_graph_title(&conn, &gid, heading);
                            break;
                        }
                    }
                }
            }

            grove_graph_repo::set_graph_parsing_status(&conn, &gid, "draft_ready")
                .map_err(|e| e.to_string())?;

            Ok(())
        })();

        if let Err(ref e) = result {
            tracing::warn!(graph_id = gid.as_str(), error = %e, "retry document generation failed");
            if let Ok(conn) = pool_bg.get() {
                let _ = grove_graph_repo::set_graph_pipeline_error(&conn, &gid, Some(e));
                let _ = grove_graph_repo::set_graph_parsing_status(&conn, &gid, "error");
            }
        }

        emit(
            &app_handle,
            "grove://graph-document-ready",
            serde_json::json!({ "graph_id": &gid }),
        );
    });

    Ok(return_value)
}

#[tauri::command]
pub fn get_graph(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&graph).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_graph_detail(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let detail = grove_graph_repo::get_graph_detail(&conn, &graph_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&detail).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_graphs(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let graphs = grove_graph_repo::list_graphs_for_conversation(&conn, &conversation_id)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&graphs).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_graph_status(
    state: State<'_, AppState>,
    graph_id: String,
    status: String,
) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::update_graph_status(&conn, &graph_id, &status).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn delete_graph(state: State<'_, AppState>, graph_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::delete_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

// -- Phase CRUD --

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_graph_phase(
    state: State<'_, AppState>,
    graph_id: String,
    task_name: String,
    task_objective: String,
    ordinal: i64,
    depends_on_json: Option<String>,
    ref_required: Option<bool>,
    reference_doc_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let deps = depends_on_json.as_deref().unwrap_or("[]");
    let ref_req = ref_required.unwrap_or(false);
    let phase_id = grove_graph_repo::insert_phase(
        &conn,
        &graph_id,
        &task_name,
        &task_objective,
        ordinal,
        deps,
        ref_req,
        reference_doc_path.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(serde_json::json!({ "phase_id": phase_id }))
}

#[tauri::command]
pub fn list_graph_phases(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let phases = grove_graph_repo::list_phases(&conn, &graph_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&phases).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_graph_phase_status(
    state: State<'_, AppState>,
    phase_id: String,
    status: String,
) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::update_phase_status(&conn, &phase_id, &status).map_err(|e| e.to_string())?;
    // Look up graph_id for the event payload.
    let phase = grove_graph_repo::get_phase(&conn, &phase_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &phase.graph_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn delete_graph_phase(state: State<'_, AppState>, phase_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    // Look up graph_id before deleting.
    let phase = grove_graph_repo::get_phase(&conn, &phase_id).map_err(|e| e.to_string())?;
    let graph_id = phase.graph_id.clone();
    grove_graph_repo::delete_phase(&conn, &phase_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

// -- Step CRUD --

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn create_graph_step(
    state: State<'_, AppState>,
    phase_id: String,
    graph_id: String,
    task_name: String,
    task_objective: String,
    ordinal: i64,
    step_type: Option<String>,
    execution_mode: Option<String>,
    depends_on_json: Option<String>,
    ref_required: Option<bool>,
    reference_doc_path: Option<String>,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let st = step_type.as_deref().unwrap_or("build");
    let em = execution_mode.as_deref().unwrap_or("sequential");
    let deps = depends_on_json.as_deref().unwrap_or("[]");
    let ref_req = ref_required.unwrap_or(false);
    let step_id = grove_graph_repo::insert_step(
        &conn,
        &phase_id,
        &graph_id,
        &task_name,
        &task_objective,
        ordinal,
        st,
        em,
        deps,
        ref_req,
        reference_doc_path.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(serde_json::json!({ "step_id": step_id }))
}

#[tauri::command]
pub fn list_graph_steps(
    state: State<'_, AppState>,
    phase_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let steps = grove_graph_repo::list_steps(&conn, &phase_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&steps).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_graph_step_status(
    state: State<'_, AppState>,
    step_id: String,
    status: String,
) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::update_step_status(&conn, &step_id, &status).map_err(|e| e.to_string())?;
    // Look up graph_id for the event payload.
    let step = grove_graph_repo::get_step(&conn, &step_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &step.graph_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn delete_graph_step(state: State<'_, AppState>, step_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    // Look up graph_id before deleting.
    let step = grove_graph_repo::get_step(&conn, &step_id).map_err(|e| e.to_string())?;
    let graph_id = step.graph_id.clone();
    grove_graph_repo::delete_step(&conn, &step_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

// -- Batch --

/// JSON structure for `populate_graph`:
/// ```json
/// [
///   {
///     "task_name": "Phase 1",
///     "task_objective": "...",
///     "ordinal": 0,
///     "depends_on_json": "[]",
///     "ref_required": false,
///     "reference_doc_path": null,
///     "steps": [
///       {
///         "task_name": "Step 1.1",
///         "task_objective": "...",
///         "ordinal": 0,
///         "step_type": "build",
///         "execution_mode": "sequential",
///         "depends_on_json": "[]",
///         "ref_required": false,
///         "reference_doc_path": null
///       }
///     ]
///   }
/// ]
/// ```
#[tauri::command]
pub fn populate_graph(
    state: State<'_, AppState>,
    graph_id: String,
    phases_json: String,
) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct StepInput {
        task_name: String,
        task_objective: String,
        ordinal: i64,
        #[serde(default = "default_step_type")]
        step_type: String,
        #[serde(default = "default_execution_mode")]
        execution_mode: String,
        #[serde(default = "default_empty_json_array")]
        depends_on_json: String,
        #[serde(default)]
        ref_required: bool,
        #[serde(default)]
        reference_doc_path: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct PhaseInput {
        task_name: String,
        task_objective: String,
        ordinal: i64,
        #[serde(default = "default_empty_json_array")]
        depends_on_json: String,
        #[serde(default)]
        ref_required: bool,
        #[serde(default)]
        reference_doc_path: Option<String>,
        #[serde(default)]
        steps: Vec<StepInput>,
    }

    fn default_step_type() -> String {
        "build".to_string()
    }
    fn default_execution_mode() -> String {
        "sequential".to_string()
    }
    fn default_empty_json_array() -> String {
        "[]".to_string()
    }

    let phases_input: Vec<PhaseInput> =
        serde_json::from_str(&phases_json).map_err(|e| format!("invalid phases_json: {e}"))?;

    let phases_data: Vec<grove_graph_repo::PopulatePhaseData> = phases_input
        .into_iter()
        .map(|p| grove_graph_repo::PopulatePhaseData {
            task_name: p.task_name,
            task_objective: p.task_objective,
            ordinal: p.ordinal,
            depends_on_json: p.depends_on_json,
            ref_required: p.ref_required,
            reference_doc_path: p.reference_doc_path,
            steps: p
                .steps
                .into_iter()
                .map(|s| grove_graph_repo::PopulateStepData {
                    task_name: s.task_name,
                    task_objective: s.task_objective,
                    ordinal: s.ordinal,
                    step_type: s.step_type,
                    execution_mode: s.execution_mode,
                    depends_on_json: s.depends_on_json,
                    ref_required: s.ref_required,
                    reference_doc_path: s.reference_doc_path,
                })
                .collect(),
        })
        .collect();

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::populate_graph(&conn, &graph_id, &phases_data).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

// -- DAG + Pipeline Queries --

#[tauri::command]
pub fn get_ready_graph_steps(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let steps =
        grove_graph_repo::get_ready_steps_for_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&steps).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_phases_pending_validation(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let phases = grove_graph_repo::get_phases_pending_validation(&conn, &graph_id)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&phases).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_step_with_feedback(
    state: State<'_, AppState>,
    step_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let (step, feedback) =
        grove_graph_repo::get_step_with_feedback(&conn, &step_id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "step": serde_json::to_value(&step).map_err(|e| e.to_string())?,
        "feedback": feedback,
    }))
}

// -- Config --

#[tauri::command]
pub fn set_graph_config(
    state: State<'_, AppState>,
    graph_id: String,
    config_json: String,
) -> Result<(), String> {
    let config: grove_core::grove_graph::GraphConfig =
        serde_json::from_str(&config_json).map_err(|e| format!("invalid config_json: {e}"))?;
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::set_graph_config(&conn, &graph_id, &config).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn get_graph_config(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let config = grove_graph_repo::get_graph_config(&conn, &graph_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&config).map_err(|e| e.to_string())
}

// -- Active + Runtime --

#[tauri::command]
pub fn set_active_graph(state: State<'_, AppState>, graph_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::set_active_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn get_active_graph(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let graph =
        grove_graph_repo::get_active_graph(&conn, &conversation_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&graph).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_graph_execution_mode(
    state: State<'_, AppState>,
    graph_id: String,
    mode: String,
) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::set_graph_execution_mode(&conn, &graph_id, &mode)
        .map_err(|e| e.to_string())?;
    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

// -- Bug Report --

/// Report a bug against a graph (and optionally a specific step/phase).
/// Sets runtime_status to 'paused'. If step_id is provided, appends a
/// `[BUG REPORT]` entry to that step's judge_feedback_json.
#[tauri::command]
pub fn report_graph_bug(
    state: State<'_, AppState>,
    graph_id: String,
    step_id: Option<String>,
    phase_id: Option<String>,
    description: String,
) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;

    // Pause the graph runtime.
    grove_graph_repo::set_runtime_status(&conn, &graph_id, "paused").map_err(|e| e.to_string())?;

    // If a step_id is provided, inject the bug report into its feedback.
    if let Some(ref sid) = step_id {
        let feedback_msg = format!(
            "[BUG REPORT] phase={} step={} — {}",
            phase_id.as_deref().unwrap_or("n/a"),
            sid,
            description,
        );
        grove_graph_repo::append_judge_feedback(&conn, sid, &feedback_msg)
            .map_err(|e| e.to_string())?;
    }

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

// -- Git Status --

/// Extract git-related fields from a graph row and return them.
#[tauri::command]
pub fn get_graph_git_status(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "branch": graph.git_branch,
        "commit_sha": graph.git_commit_sha,
        "pr_url": graph.git_pr_url,
        "merge_status": graph.git_merge_status,
    }))
}

// -- Loop Control Commands --

// ── Readiness Check & Clarifications ─────────────────────────────────────────

/// Run a readiness check for a graph. Returns either `Ready` or
/// `NeedsClarification` with missing doc names and questions.
#[tauri::command]
pub fn check_graph_readiness(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let pool = state.pool().clone();
    let workspace_root = state.workspace_root().to_path_buf();

    let conn = pool.get().map_err(|e| e.to_string())?;

    // Resolve project_root.
    let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    let conv = grove_core::db::repositories::conversations_repo::get(&conn, &graph.conversation_id)
        .map_err(|e| e.to_string())?;
    let project_root =
        match grove_core::db::repositories::projects_repo::get(&conn, &conv.project_id) {
            Ok(p) => std::path::PathBuf::from(&p.root_path),
            Err(_) => workspace_root,
        };

    let result =
        grove_core::grove_graph::planning::check_readiness(&conn, &graph_id, &project_root)
            .map_err(|e| e.to_string())?;

    serde_json::to_value(&result).map_err(|e| e.to_string())
}

/// Submit an answer for a clarification question.
#[tauri::command]
pub fn submit_clarification_answer(
    state: State<'_, AppState>,
    clarification_id: String,
    answer: String,
) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_graph_repo::answer_clarification(&conn, &clarification_id, &answer)
        .map_err(|e| e.to_string())
}

/// List all clarification questions for a graph.
#[tauri::command]
pub fn list_graph_clarifications(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let clarifications =
        grove_graph_repo::list_clarifications(&conn, &graph_id).map_err(|e| e.to_string())?;
    serde_json::to_value(&clarifications).map_err(|e| e.to_string())
}

/// Sets runtime_status to 'running' (or 'queued' if another graph in the same
/// conversation is already running), and spawns `run_graph_loop` as a background
/// tokio task. Returns immediately with `{ started: true }` or `{ queued: true }`.
#[tauri::command]
pub async fn start_graph_loop(
    state: State<'_, AppState>,
    graph_id: String,
) -> Result<serde_json::Value, String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    // Validate parsing status and check for queue.
    let conversation_id = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
        if graph.parsing_status != "complete" {
            return Err(format!(
                "cannot start graph loop: parsing_status is '{}', expected 'complete'",
                graph.parsing_status
            ));
        }

        // If another graph in this conversation is already running, queue this one.
        if grove_graph_repo::has_running_graph_in_conversation(&conn, &graph.conversation_id)
            .map_err(|e| e.to_string())?
        {
            grove_graph_repo::set_runtime_status(&conn, &graph_id, "queued")
                .map_err(|e| e.to_string())?;

            emit(
                &state.app_handle,
                "grove://graphs-changed",
                serde_json::json!({ "graph_id": &graph_id, "conversation_id": &graph.conversation_id }),
            );
            return Ok(serde_json::json!({ "queued": true }));
        }

        graph.conversation_id.clone()
    };

    // Resolve workdir from the conversation's worktree.
    let workdir = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let (wd, _proj) = resolve_graph_workdir(&conn, &conversation_id, &workspace_root)?;
        wd
    };

    let db_path = grove_core::config::db_path(&workspace_root);
    let gid = graph_id.clone();

    // Build the provider for agent execution.
    let cfg =
        grove_core::config::GroveConfig::load_or_create(&workdir).map_err(|e| e.to_string())?;
    let provider = grove_core::orchestrator::build_provider(&cfg, &workdir, None, None)
        .map_err(|e| e.to_string())?;

    // Set runtime_status to "running" BEFORE spawning the background thread.
    // This prevents the double-click bug: without this, the frontend gets
    // { started: true } while runtime_status is still "idle" in the DB, so
    // the Start button re-enables before the loop thread has a chance to
    // update the status.
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::set_runtime_status(&conn, &graph_id, "running")
            .map_err(|e| e.to_string())?;
    }

    // Spawn the loop on a blocking thread. `run_graph_loop` is async but uses
    // `&Connection` (not Send), so we run it via `block_on` on a dedicated thread.
    let pool_for_dequeue = pool.clone();
    let app_for_dequeue = app_handle.clone();
    let ws_for_dequeue = workspace_root.clone();
    let conv_id_for_spawn = conversation_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "failed to get DB connection for graph loop");
                return;
            }
        };

        let result = rt.block_on(grove_core::grove_graph::loop_orchestrator::run_graph_loop(
            &conn, &gid, &workdir, &db_path, &provider,
        ));

        match result {
            Ok(outcome) => {
                tracing::info!(graph_id = gid.as_str(), outcome = ?outcome, "graph loop finished");
            }
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "graph loop error");
                if let Ok(c) = pool.get() {
                    let _ = grove_graph_repo::update_graph_status(&c, &gid, "failed");
                    let _ = grove_graph_repo::set_runtime_status(&c, &gid, "idle");
                }
            }
        }

        emit(
            &app_handle,
            "grove://graphs-changed",
            serde_json::json!({ "graph_id": &gid, "conversation_id": &conv_id_for_spawn }),
        );

        // When this graph finishes, dequeue the next one in the conversation.
        dequeue_next_graph(&pool_for_dequeue, &ws_for_dequeue, &app_for_dequeue);
    });

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id, "conversation_id": &conversation_id }),
    );

    Ok(serde_json::json!({ "started": true }))
}

/// Pause the graph loop. Sets runtime_status to 'paused'. The loop will pause
/// at the next stage transition — currently-running agents complete their work
/// but no new agents are spawned.
#[tauri::command]
pub fn pause_graph(state: State<'_, AppState>, graph_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    grove_graph_repo::set_runtime_status(&conn, &graph_id, "paused").map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id, "conversation_id": &graph.conversation_id }),
    );
    Ok(())
}

/// Resume a paused graph. Sets runtime_status to 'running' and re-spawns
/// `run_graph_loop`. The loop resumes from DAG state — whatever steps are
/// ready get picked up.
#[tauri::command]
pub async fn resume_graph(state: State<'_, AppState>, graph_id: String) -> Result<(), String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    // Resolve conversation_id and workdir, and guard against concurrent loops.
    let (conversation_id, workdir) = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;

        // Guard: if another graph in this conversation is already running, queue instead.
        if grove_graph_repo::has_running_graph_in_conversation(&conn, &graph.conversation_id)
            .map_err(|e| e.to_string())?
        {
            grove_graph_repo::set_runtime_status(&conn, &graph_id, "queued")
                .map_err(|e| e.to_string())?;

            emit(
                &state.app_handle,
                "grove://graphs-changed",
                serde_json::json!({ "graph_id": &graph_id, "conversation_id": &graph.conversation_id }),
            );
            return Ok(());
        }

        let (wd, _) = resolve_graph_workdir(&conn, &graph.conversation_id, &workspace_root)?;
        (graph.conversation_id.clone(), wd)
    };

    // Set running.
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        grove_graph_repo::set_runtime_status(&conn, &graph_id, "running")
            .map_err(|e| e.to_string())?;
    }

    let db_path = grove_core::config::db_path(&workspace_root);
    let gid = graph_id.clone();

    // Build the provider for agent execution.
    let cfg =
        grove_core::config::GroveConfig::load_or_create(&workdir).map_err(|e| e.to_string())?;
    let provider = grove_core::orchestrator::build_provider(&cfg, &workdir, None, None)
        .map_err(|e| e.to_string())?;

    // Re-spawn the loop on a blocking thread.
    let pool_for_dequeue = pool.clone();
    let app_for_dequeue = app_handle.clone();
    let ws_for_dequeue = workspace_root.clone();
    let conv_id_for_spawn = conversation_id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "failed to get DB connection for resume");
                return;
            }
        };

        let result = rt.block_on(grove_core::grove_graph::loop_orchestrator::run_graph_loop(
            &conn, &gid, &workdir, &db_path, &provider,
        ));

        match result {
            Ok(outcome) => {
                tracing::info!(graph_id = gid.as_str(), outcome = ?outcome, "resumed graph loop finished");
            }
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "resumed graph loop error");
                if let Ok(c) = pool.get() {
                    let _ = grove_graph_repo::update_graph_status(&c, &gid, "failed");
                    let _ = grove_graph_repo::set_runtime_status(&c, &gid, "idle");
                }
            }
        }

        emit(
            &app_handle,
            "grove://graphs-changed",
            serde_json::json!({ "graph_id": &gid, "conversation_id": &conv_id_for_spawn }),
        );

        // Dequeue next graph in the conversation.
        dequeue_next_graph(&pool_for_dequeue, &ws_for_dequeue, &app_for_dequeue);
    });

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id, "conversation_id": &conversation_id }),
    );
    Ok(())
}

/// Abort the graph loop. Sets runtime_status to 'aborted' and status to 'failed'.
/// The loop exits at the next runtime check. Currently-running agents may
/// complete but their results are discarded.
///
/// Also dequeues the next graph in the conversation (if any), since the
/// running graph's background loop may have already exited before the abort
/// status was set.
#[tauri::command]
pub fn abort_graph(state: State<'_, AppState>, graph_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
    grove_graph_repo::set_runtime_status(&conn, &graph_id, "aborted").map_err(|e| e.to_string())?;
    grove_graph_repo::update_graph_status(&conn, &graph_id, "failed").map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id, "conversation_id": &graph.conversation_id }),
    );

    // Dequeue next graph in the conversation. The running loop will also
    // call dequeue when it exits, but `dequeue_next_graph` is safe to call
    // multiple times — the SQL guard prevents double-starting.
    let pool = state.pool().clone();
    let workspace_root = state.workspace_root().to_path_buf();
    let app_handle = state.app_handle.clone();
    dequeue_next_graph(&pool, &workspace_root, &app_handle);

    Ok(())
}

/// Restart a failed graph. Only available when status == 'failed' and
/// rerun_count < max_reruns. Increments rerun_count, re-opens all failed steps
/// (preserving accumulated feedback), resets runtime_status to 'running' and
/// status to 'inprogress', then re-spawns `run_graph_loop`.
#[tauri::command]
pub async fn restart_graph(
    state: State<'_, AppState>,
    graph_id: String,
    full_restart: Option<bool>,
) -> Result<(), String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();
    let do_full_restart = full_restart.unwrap_or(false);

    // Validate and prepare restart.
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;

        if graph.status != "failed" {
            return Err(format!(
                "cannot restart graph: status is '{}', expected 'failed'",
                graph.status
            ));
        }
        if graph.rerun_count >= graph.max_reruns {
            return Err(format!(
                "cannot restart graph: rerun_count ({}) >= max_reruns ({})",
                graph.rerun_count, graph.max_reruns
            ));
        }

        // Increment rerun_count.
        grove_graph_repo::increment_rerun_count(&conn, &graph_id).map_err(|e| e.to_string())?;

        // Re-open all failed steps (preserving their judge_feedback_json).
        let all_steps =
            grove_graph_repo::list_steps_for_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
        for step in &all_steps {
            if step.status == "failed" {
                grove_graph_repo::reopen_step(&conn, &step.id).map_err(|e| e.to_string())?;
            }
        }

        if do_full_restart {
            // Full restart: reset parsing_status to 'pending' so the
            // pre-planning loop runs again before execution.
            grove_graph_repo::set_graph_parsing_status(&conn, &graph_id, "pending")
                .map_err(|e| e.to_string())?;
        }

        // Reset graph state.
        grove_graph_repo::update_graph_status(&conn, &graph_id, "inprogress")
            .map_err(|e| e.to_string())?;
        grove_graph_repo::set_runtime_status(&conn, &graph_id, "running")
            .map_err(|e| e.to_string())?;
    }

    // Resolve workdir from conversation worktree.
    let workdir = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
        let (wd, _) = resolve_graph_workdir(&conn, &graph.conversation_id, &workspace_root)?;
        wd
    };

    let db_path = grove_core::config::db_path(&workspace_root);
    let gid = graph_id.clone();

    // Build the provider for agent execution.
    let cfg =
        grove_core::config::GroveConfig::load_or_create(&workdir).map_err(|e| e.to_string())?;
    let provider = grove_core::orchestrator::build_provider(&cfg, &workdir, None, None)
        .map_err(|e| e.to_string())?;

    // Re-spawn the loop on a blocking thread.
    let pool_for_spawn = pool.clone();
    let pool_for_dequeue = pool.clone();
    let app_for_dequeue = app_handle.clone();
    let ws_for_dequeue = workspace_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let conn = match pool_for_spawn.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "failed to get DB connection for restart");
                return;
            }
        };

        // Full restart: re-run the pre-planning + graph creation pipeline.
        if do_full_restart {
            tracing::info!(
                graph_id = gid.as_str(),
                "full restart — running pre-planning loop"
            );
            if let Err(e) = rt.block_on(grove_core::grove_graph::planning::run_pre_planning_loop(
                &conn, &gid, &workdir, &db_path, &provider,
            )) {
                tracing::error!(graph_id = gid.as_str(), error = %e, "pre-planning loop failed on full restart");
                let _ = grove_graph_repo::update_graph_status(&conn, &gid, "failed");
                let _ = grove_graph_repo::set_runtime_status(&conn, &gid, "idle");
                emit(
                    &app_handle,
                    "grove://graphs-changed",
                    serde_json::json!({ "graph_id": &gid }),
                );
                dequeue_next_graph(&pool_for_dequeue, &ws_for_dequeue, &app_for_dequeue);
                return;
            }

            // Re-create the graph structure (phases/steps) from the updated plan.
            tracing::info!(
                graph_id = gid.as_str(),
                "full restart — running graph creation"
            );
            if let Err(e) = rt.block_on(grove_core::grove_graph::planning::run_graph_creation(
                &conn, &gid, &workdir, &db_path, &provider,
            )) {
                tracing::error!(graph_id = gid.as_str(), error = %e, "graph creation failed on full restart");
                let _ = grove_graph_repo::update_graph_status(&conn, &gid, "failed");
                let _ = grove_graph_repo::set_runtime_status(&conn, &gid, "idle");
                emit(
                    &app_handle,
                    "grove://graphs-changed",
                    serde_json::json!({ "graph_id": &gid }),
                );
                dequeue_next_graph(&pool_for_dequeue, &ws_for_dequeue, &app_for_dequeue);
                return;
            }
        }

        let result = rt.block_on(grove_core::grove_graph::loop_orchestrator::run_graph_loop(
            &conn, &gid, &workdir, &db_path, &provider,
        ));

        match result {
            Ok(outcome) => {
                tracing::info!(graph_id = gid.as_str(), outcome = ?outcome, "restarted graph loop finished");
            }
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "restarted graph loop error");
                if let Ok(c) = pool_for_spawn.get() {
                    let _ = grove_graph_repo::update_graph_status(&c, &gid, "failed");
                    let _ = grove_graph_repo::set_runtime_status(&c, &gid, "idle");
                }
            }
        }

        emit(
            &app_handle,
            "grove://graphs-changed",
            serde_json::json!({ "graph_id": &gid }),
        );

        // Dequeue next graph in the conversation.
        dequeue_next_graph(&pool_for_dequeue, &ws_for_dequeue, &app_for_dequeue);
    });

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

// ── Rerun Step / Phase ─────────────────────────────────────────────────────

/// Rerun a single step: reopens it (preserving feedback), resets its parent
/// phase validation to 'pending', then resumes the graph loop.
///
/// Valid for steps in `closed` or `failed` status. The graph must be in
/// `idle` or `paused` runtime status (not actively running).
#[tauri::command]
pub async fn rerun_step(state: State<'_, AppState>, step_id: String) -> Result<(), String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    let graph_id = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let step = grove_graph_repo::get_step(&conn, &step_id).map_err(|e| e.to_string())?;

        if step.status != "closed" && step.status != "failed" {
            return Err(format!(
                "cannot rerun step: status is '{}', expected 'closed' or 'failed'",
                step.status
            ));
        }

        // Check runtime status — cannot rerun while graph is actively running.
        let graph =
            grove_graph_repo::get_graph(&conn, &step.graph_id).map_err(|e| e.to_string())?;
        if graph.runtime_status == "running" {
            return Err("cannot rerun step while graph is running — pause first".to_string());
        }

        // Reopen the step and reset phase validation.
        grove_graph_repo::reopen_step(&conn, &step_id).map_err(|e| e.to_string())?;
        grove_graph_repo::set_phase_validation_status(&conn, &step.phase_id, "pending")
            .map_err(|e| e.to_string())?;

        // Ensure graph is inprogress/running so the loop picks it up.
        grove_graph_repo::update_graph_status(&conn, &step.graph_id, "inprogress")
            .map_err(|e| e.to_string())?;
        grove_graph_repo::set_runtime_status(&conn, &step.graph_id, "running")
            .map_err(|e| e.to_string())?;

        step.graph_id.clone()
    };

    // Resolve workdir from conversation worktree (consistent with other graph commands).
    let workdir = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
        let (wd, _) = resolve_graph_workdir(&conn, &graph.conversation_id, &workspace_root)?;
        wd
    };

    let db_path = grove_core::config::db_path(&workspace_root);
    let gid = graph_id.clone();

    let cfg =
        grove_core::config::GroveConfig::load_or_create(&workdir).map_err(|e| e.to_string())?;
    let provider = grove_core::orchestrator::build_provider(&cfg, &workdir, None, None)
        .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "rerun_step: DB connection error");
                return;
            }
        };

        let result = rt.block_on(grove_core::grove_graph::loop_orchestrator::run_graph_loop(
            &conn, &gid, &workdir, &db_path, &provider,
        ));

        match result {
            Ok(outcome) => {
                tracing::info!(graph_id = gid.as_str(), outcome = ?outcome, "rerun_step loop finished");
            }
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "rerun_step loop error");
                if let Ok(c) = pool.get() {
                    let _ = grove_graph_repo::update_graph_status(&c, &gid, "failed");
                    let _ = grove_graph_repo::set_runtime_status(&c, &gid, "idle");
                }
            }
        }

        emit(
            &app_handle,
            "grove://graphs-changed",
            serde_json::json!({ "graph_id": &gid }),
        );
    });

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}

/// Rerun an entire phase: reopens all steps in the phase, resets validation
/// to 'pending', then resumes the graph loop.
///
/// The graph must be in `idle` or `paused` runtime status.
#[tauri::command]
pub async fn rerun_phase(state: State<'_, AppState>, phase_id: String) -> Result<(), String> {
    let pool = state.pool().clone();
    let app_handle = state.app_handle.clone();
    let workspace_root = state.workspace_root().to_path_buf();

    let graph_id = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let phase = grove_graph_repo::get_phase(&conn, &phase_id).map_err(|e| e.to_string())?;

        // Check runtime status.
        let graph =
            grove_graph_repo::get_graph(&conn, &phase.graph_id).map_err(|e| e.to_string())?;
        if graph.runtime_status == "running" {
            return Err("cannot rerun phase while graph is running — pause first".to_string());
        }

        // Reopen all steps in the phase.
        let steps = grove_graph_repo::list_steps(&conn, &phase_id).map_err(|e| e.to_string())?;
        for step in &steps {
            grove_graph_repo::reopen_step(&conn, &step.id).map_err(|e| e.to_string())?;
        }

        // Reset phase validation and status.
        grove_graph_repo::set_phase_validation_status(&conn, &phase_id, "pending")
            .map_err(|e| e.to_string())?;

        // Ensure graph is inprogress/running.
        grove_graph_repo::update_graph_status(&conn, &phase.graph_id, "inprogress")
            .map_err(|e| e.to_string())?;
        grove_graph_repo::set_runtime_status(&conn, &phase.graph_id, "running")
            .map_err(|e| e.to_string())?;

        phase.graph_id.clone()
    };

    // Resolve project_root and spawn the loop.
    let project_root = {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let graph = grove_graph_repo::get_graph(&conn, &graph_id).map_err(|e| e.to_string())?;
        let conv =
            grove_core::db::repositories::conversations_repo::get(&conn, &graph.conversation_id)
                .map_err(|e| e.to_string())?;
        match grove_core::db::repositories::projects_repo::get(&conn, &conv.project_id) {
            Ok(p) => std::path::PathBuf::from(&p.root_path),
            Err(_) => workspace_root.clone(),
        }
    };

    let db_path = grove_core::config::db_path(&workspace_root);
    let gid = graph_id.clone();

    let cfg = grove_core::config::GroveConfig::load_or_create(&project_root)
        .map_err(|e| e.to_string())?;
    let provider = grove_core::orchestrator::build_provider(&cfg, &project_root, None, None)
        .map_err(|e| e.to_string())?;

    tauri::async_runtime::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let conn = match pool.get() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "rerun_phase: DB connection error");
                return;
            }
        };

        let result = rt.block_on(grove_core::grove_graph::loop_orchestrator::run_graph_loop(
            &conn,
            &gid,
            &project_root,
            &db_path,
            &provider,
        ));

        match result {
            Ok(outcome) => {
                tracing::info!(graph_id = gid.as_str(), outcome = ?outcome, "rerun_phase loop finished");
            }
            Err(e) => {
                tracing::error!(graph_id = gid.as_str(), error = %e, "rerun_phase loop error");
                if let Ok(c) = pool.get() {
                    let _ = grove_graph_repo::update_graph_status(&c, &gid, "failed");
                    let _ = grove_graph_repo::set_runtime_status(&c, &gid, "idle");
                }
            }
        }

        emit(
            &app_handle,
            "grove://graphs-changed",
            serde_json::json!({ "graph_id": &gid }),
        );
    });

    emit(
        &state.app_handle,
        "grove://graphs-changed",
        serde_json::json!({ "graph_id": &graph_id }),
    );
    Ok(())
}
