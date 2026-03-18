use tauri::State;

use super::emit;
use crate::state::AppState;

// ── Automations ──────────────────────────────────────────────────────────────

#[tauri::command]
pub fn create_automation(
    state: State<'_, AppState>,
    project_id: String,
    name: String,
    trigger_config_json: String,
    defaults_json: Option<String>,
    description: Option<String>,
    session_mode: Option<String>,
    dedicated_conversation_id: Option<String>,
) -> Result<grove_core::automation::AutomationDef, String> {
    let trigger: grove_core::automation::TriggerConfig =
        serde_json::from_str(&trigger_config_json).map_err(|e| e.to_string())?;
    let defaults: grove_core::automation::AutomationDefaults = match defaults_json {
        Some(ref json) => serde_json::from_str(json).map_err(|e| e.to_string())?,
        None => grove_core::automation::AutomationDefaults::default(),
    };

    let sm = match session_mode.as_deref() {
        Some("dedicated") => grove_core::automation::SessionMode::Dedicated,
        _ => grove_core::automation::SessionMode::New,
    };

    let id = format!("auto_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let now = chrono::Utc::now().to_rfc3339();

    let automation = grove_core::automation::AutomationDef {
        id,
        project_id,
        name,
        description,
        enabled: true,
        trigger,
        defaults,
        session_mode: sm,
        dedicated_conversation_id,
        source_path: None,
        last_triggered_at: None,
        created_at: now.clone(),
        updated_at: now,
        notifications: None,
    };

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::insert_automation(&conn, &automation)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "project_id": automation.project_id }),
    );

    Ok(automation)
}

#[tauri::command]
pub fn list_automations(
    state: State<'_, AppState>,
    project_id: String,
) -> Result<Vec<grove_core::automation::AutomationDef>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::list_automations(&conn, &project_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_automation(
    state: State<'_, AppState>,
    automation_id: String,
) -> Result<grove_core::automation::AutomationDef, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::get_automation(&conn, &automation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_automation(
    state: State<'_, AppState>,
    automation_id: String,
    name: Option<String>,
    description: Option<String>,
    enabled: Option<bool>,
    trigger_config_json: Option<String>,
    defaults_json: Option<String>,
    session_mode: Option<String>,
    dedicated_conversation_id: Option<String>,
) -> Result<grove_core::automation::AutomationDef, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let mut automation =
        grove_core::db::repositories::automations_repo::get_automation(&conn, &automation_id)
            .map_err(|e| e.to_string())?;

    if let Some(n) = name {
        automation.name = n;
    }
    if let Some(d) = description {
        automation.description = Some(d);
    }
    if let Some(e) = enabled {
        automation.enabled = e;
    }
    if let Some(ref json) = trigger_config_json {
        automation.trigger = serde_json::from_str(json).map_err(|e| e.to_string())?;
    }
    if let Some(ref json) = defaults_json {
        automation.defaults = serde_json::from_str(json).map_err(|e| e.to_string())?;
    }
    if let Some(ref sm) = session_mode {
        automation.session_mode = match sm.as_str() {
            "dedicated" => grove_core::automation::SessionMode::Dedicated,
            _ => grove_core::automation::SessionMode::New,
        };
    }
    if let Some(ref conv_id) = dedicated_conversation_id {
        automation.dedicated_conversation_id = if conv_id.is_empty() {
            None
        } else {
            Some(conv_id.clone())
        };
    }

    grove_core::db::repositories::automations_repo::update_automation(&conn, &automation)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "automation_id": automation_id }),
    );

    Ok(automation)
}

#[tauri::command]
pub fn delete_automation(state: State<'_, AppState>, automation_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::delete_automation(&conn, &automation_id)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "automation_id": automation_id }),
    );

    Ok(())
}

#[tauri::command]
pub fn toggle_automation(
    state: State<'_, AppState>,
    automation_id: String,
    enabled: bool,
) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let mut automation =
        grove_core::db::repositories::automations_repo::get_automation(&conn, &automation_id)
            .map_err(|e| e.to_string())?;
    automation.enabled = enabled;
    grove_core::db::repositories::automations_repo::update_automation(&conn, &automation)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "automation_id": automation_id, "enabled": enabled }),
    );

    Ok(())
}

// ── Automation Steps ─────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_automation_steps(
    state: State<'_, AppState>,
    automation_id: String,
) -> Result<Vec<grove_core::automation::AutomationStep>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::list_steps(&conn, &automation_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_automation_step(
    state: State<'_, AppState>,
    automation_id: String,
    step_key: String,
    objective: String,
    ordinal: i32,
    depends_on_json: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    budget_usd: Option<f64>,
    pipeline: Option<String>,
    permission_mode: Option<String>,
    condition: Option<String>,
) -> Result<grove_core::automation::AutomationStep, String> {
    let depends_on: Vec<String> = match depends_on_json {
        Some(ref json) => serde_json::from_str(json).map_err(|e| e.to_string())?,
        None => Vec::new(),
    };

    let id = format!("step_{}", &uuid::Uuid::new_v4().simple().to_string()[..12]);
    let now = chrono::Utc::now().to_rfc3339();

    let step = grove_core::automation::AutomationStep {
        id,
        automation_id: automation_id.clone(),
        step_key,
        ordinal,
        objective,
        depends_on,
        provider,
        model,
        budget_usd,
        pipeline,
        permission_mode,
        condition,
        created_at: now.clone(),
        updated_at: now,
    };

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::insert_step(&conn, &step)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "automation_id": automation_id }),
    );

    Ok(step)
}

#[tauri::command]
pub fn update_automation_step(
    state: State<'_, AppState>,
    step_id: String,
    step_key: Option<String>,
    objective: Option<String>,
    ordinal: Option<i32>,
    depends_on_json: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    budget_usd: Option<f64>,
    pipeline: Option<String>,
    permission_mode: Option<String>,
    condition: Option<String>,
) -> Result<grove_core::automation::AutomationStep, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let mut step = grove_core::db::repositories::automations_repo::get_step(&conn, &step_id)
        .map_err(|e| e.to_string())?;

    if let Some(k) = step_key {
        step.step_key = k;
    }
    if let Some(o) = objective {
        step.objective = o;
    }
    if let Some(ord) = ordinal {
        step.ordinal = ord;
    }
    if let Some(ref json) = depends_on_json {
        step.depends_on = serde_json::from_str(json).map_err(|e| e.to_string())?;
    }
    // Allow explicit null to clear optional fields
    step.provider = provider;
    step.model = model;
    step.budget_usd = budget_usd;
    step.pipeline = pipeline;
    step.permission_mode = permission_mode;
    step.condition = condition;

    grove_core::db::repositories::automations_repo::update_step(&conn, &step)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "automation_id": step.automation_id }),
    );

    Ok(step)
}

#[tauri::command]
pub fn delete_automation_step(state: State<'_, AppState>, step_id: String) -> Result<(), String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    // Load the step first to get the automation_id for the event
    let step = grove_core::db::repositories::automations_repo::get_step(&conn, &step_id)
        .map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::delete_step(&conn, &step_id)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "automation_id": step.automation_id }),
    );

    Ok(())
}

// ── Automation Execution ─────────────────────────────────────────────────────

#[tauri::command]
pub fn trigger_automation_manually(
    state: State<'_, AppState>,
    automation_id: String,
) -> Result<String, String> {
    let trigger_info = serde_json::json!({
        "type": "manual",
        "triggered_by": "gui",
        "triggered_at": chrono::Utc::now().to_rfc3339(),
    });
    let run_id = state
        .workflow_engine
        .start_run(&automation_id, trigger_info)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automation-runs-changed",
        serde_json::json!({ "automation_id": automation_id, "run_id": run_id }),
    );

    Ok(run_id)
}

#[tauri::command]
pub fn get_automation_run(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<grove_core::automation::AutomationRun, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::get_run(&conn, &run_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_automation_runs(
    state: State<'_, AppState>,
    automation_id: String,
    limit: Option<i64>,
) -> Result<Vec<grove_core::automation::AutomationRun>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::list_runs(
        &conn,
        &automation_id,
        limit.unwrap_or(50),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_automation_run_steps(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<Vec<grove_core::automation::AutomationRunStep>, String> {
    let conn = state.pool().get().map_err(|e| e.to_string())?;
    grove_core::db::repositories::automations_repo::list_run_steps(&conn, &run_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cancel_automation_run(state: State<'_, AppState>, run_id: String) -> Result<(), String> {
    state
        .workflow_engine
        .cancel_run(&run_id)
        .map_err(|e| e.to_string())?;

    emit(
        &state.app_handle,
        "grove://automation-runs-changed",
        serde_json::json!({ "run_id": run_id }),
    );

    Ok(())
}

// ── Automation Markdown Sync ─────────────────────────────────────────────────

#[tauri::command]
pub fn import_automations_from_files(
    state: State<'_, AppState>,
    project_id: String,
    project_root: String,
) -> Result<Vec<grove_core::automation::AutomationDef>, String> {
    let automations_dir = std::path::Path::new(&project_root)
        .join(".grove")
        .join("automations");

    if !automations_dir.is_dir() {
        return Ok(Vec::new());
    }

    let conn = state.pool().get().map_err(|e| e.to_string())?;
    let mut imported = Vec::new();

    let entries = std::fs::read_dir(&automations_dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let file_stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let (mut automation, steps) = grove_core::automation::markdown::parse_automation_markdown(
            &content,
            &file_stem,
            &project_id,
        )
        .map_err(|e| e.to_string())?;

        automation.source_path = Some(path.to_string_lossy().to_string());

        // Upsert: try insert, if it already exists update instead
        match grove_core::db::repositories::automations_repo::insert_automation(&conn, &automation)
        {
            Ok(()) => {}
            Err(_) => {
                grove_core::db::repositories::automations_repo::update_automation(
                    &conn,
                    &automation,
                )
                .map_err(|e| e.to_string())?;
            }
        }

        // Replace steps: delete existing, then insert new ones
        let existing_steps =
            grove_core::db::repositories::automations_repo::list_steps(&conn, &automation.id)
                .map_err(|e| e.to_string())?;
        for old_step in &existing_steps {
            let _ =
                grove_core::db::repositories::automations_repo::delete_step(&conn, &old_step.id);
        }
        for step in &steps {
            grove_core::db::repositories::automations_repo::insert_step(&conn, step)
                .map_err(|e| e.to_string())?;
        }

        imported.push(automation);
    }

    emit(
        &state.app_handle,
        "grove://automations-changed",
        serde_json::json!({ "project_id": project_id, "imported": imported.len() }),
    );

    Ok(imported)
}
