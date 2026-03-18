use rusqlite::{Connection, OptionalExtension, params};

use crate::automation::{
    AutomationDef, AutomationDefaults, AutomationRun, AutomationRunState, AutomationRunStep,
    AutomationStep, NotificationConfig, SessionMode, StepState, TriggerConfig,
};
use crate::errors::{GroveError, GroveResult};

// ---------------------------------------------------------------------------
// AutomationEventRow — lightweight struct for the automation_events table
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AutomationEventRow {
    pub id: i64,
    pub event_type: String,
    pub payload: String,
    pub source: Option<String>,
    pub automation_id: Option<String>,
    pub automation_run_id: Option<String>,
    pub created_at: String,
}

// ===========================================================================
//  Automations (definitions)
// ===========================================================================

pub fn insert_automation(conn: &Connection, a: &AutomationDef) -> GroveResult<()> {
    let trigger_type = a.trigger.type_str();
    let trigger_config =
        serde_json::to_string(&a.trigger).map_err(|e| GroveError::Runtime(e.to_string()))?;
    let session_mode = match a.session_mode {
        SessionMode::New => "new",
        SessionMode::Dedicated => "dedicated",
    };
    let notifications_json: Option<String> = a
        .notifications
        .as_ref()
        .map(|n| serde_json::to_string(n))
        .transpose()
        .map_err(|e| GroveError::Runtime(e.to_string()))?;
    conn.execute(
        "INSERT INTO automations (
            id, project_id, name, description, enabled,
            trigger_type, trigger_config,
            default_provider, default_model, default_budget_usd,
            default_pipeline, default_permission_mode,
            session_mode, dedicated_conversation_id, source_path,
            last_triggered_at, created_at, updated_at,
            notifications_json
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5,
            ?6, ?7,
            ?8, ?9, ?10,
            ?11, ?12,
            ?13, ?14, ?15,
            ?16, ?17, ?18,
            ?19
        )",
        params![
            a.id,
            a.project_id,
            a.name,
            a.description,
            a.enabled as i32,
            trigger_type,
            trigger_config,
            a.defaults.provider,
            a.defaults.model,
            a.defaults.budget_usd,
            a.defaults.pipeline,
            a.defaults.permission_mode,
            session_mode,
            a.dedicated_conversation_id,
            a.source_path,
            a.last_triggered_at,
            a.created_at,
            a.updated_at,
            notifications_json,
        ],
    )?;
    Ok(())
}

pub fn get_automation(conn: &Connection, id: &str) -> GroveResult<AutomationDef> {
    let row = conn
        .query_row(
            "SELECT id, project_id, name, description, enabled,
                    trigger_type, trigger_config,
                    default_provider, default_model, default_budget_usd,
                    default_pipeline, default_permission_mode,
                    session_mode, dedicated_conversation_id, source_path,
                    last_triggered_at, created_at, updated_at,
                    notifications_json
             FROM automations WHERE id = ?1",
            [id],
            map_automation_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("automation {id}")))
}

pub fn list_automations(conn: &Connection, project_id: &str) -> GroveResult<Vec<AutomationDef>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, description, enabled,
                trigger_type, trigger_config,
                default_provider, default_model, default_budget_usd,
                default_pipeline, default_permission_mode,
                session_mode, dedicated_conversation_id, source_path,
                last_triggered_at, created_at, updated_at,
                notifications_json
         FROM automations WHERE project_id = ?1 ORDER BY name",
    )?;
    let rows = stmt
        .query_map([project_id], map_automation_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_automation(conn: &Connection, a: &AutomationDef) -> GroveResult<()> {
    let trigger_type = a.trigger.type_str();
    let trigger_config =
        serde_json::to_string(&a.trigger).map_err(|e| GroveError::Runtime(e.to_string()))?;
    let session_mode = match a.session_mode {
        SessionMode::New => "new",
        SessionMode::Dedicated => "dedicated",
    };
    let notifications_json: Option<String> = a
        .notifications
        .as_ref()
        .map(|n| serde_json::to_string(n))
        .transpose()
        .map_err(|e| GroveError::Runtime(e.to_string()))?;
    let n = conn.execute(
        "UPDATE automations SET
            name = ?1, description = ?2, enabled = ?3,
            trigger_type = ?4, trigger_config = ?5,
            default_provider = ?6, default_model = ?7, default_budget_usd = ?8,
            default_pipeline = ?9, default_permission_mode = ?10,
            session_mode = ?11, dedicated_conversation_id = ?12, source_path = ?13,
            notifications_json = ?14,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?15",
        params![
            a.name,
            a.description,
            a.enabled as i32,
            trigger_type,
            trigger_config,
            a.defaults.provider,
            a.defaults.model,
            a.defaults.budget_usd,
            a.defaults.pipeline,
            a.defaults.permission_mode,
            session_mode,
            a.dedicated_conversation_id,
            a.source_path,
            notifications_json,
            a.id,
        ],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("automation {}", a.id)));
    }
    Ok(())
}

pub fn delete_automation(conn: &Connection, id: &str) -> GroveResult<()> {
    let n = conn.execute("DELETE FROM automations WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("automation {id}")));
    }
    Ok(())
}

pub fn list_enabled_cron_automations(conn: &Connection) -> GroveResult<Vec<AutomationDef>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, description, enabled,
                trigger_type, trigger_config,
                default_provider, default_model, default_budget_usd,
                default_pipeline, default_permission_mode,
                session_mode, dedicated_conversation_id, source_path,
                last_triggered_at, created_at, updated_at,
                notifications_json
         FROM automations WHERE enabled = 1 AND trigger_type = 'cron'",
    )?;
    let rows = stmt
        .query_map([], map_automation_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_enabled_issue_automations(conn: &Connection) -> GroveResult<Vec<AutomationDef>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, description, enabled,
                trigger_type, trigger_config,
                default_provider, default_model, default_budget_usd,
                default_pipeline, default_permission_mode,
                session_mode, dedicated_conversation_id, source_path,
                last_triggered_at, created_at, updated_at,
                notifications_json
         FROM automations WHERE enabled = 1 AND trigger_type = 'issue'",
    )?;
    let rows = stmt
        .query_map([], map_automation_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_last_triggered(conn: &Connection, id: &str) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE automations
         SET last_triggered_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
             updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?1",
        [id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("automation {id}")));
    }
    Ok(())
}

fn map_automation_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationDef> {
    let enabled_int: i32 = r.get(4)?;
    let _trigger_type: String = r.get(5)?;
    let trigger_config_json: String = r.get(6)?;
    let session_mode_str: String = r.get(12)?;

    let trigger: TriggerConfig = serde_json::from_str(&trigger_config_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let session_mode = match session_mode_str.as_str() {
        "dedicated" => SessionMode::Dedicated,
        _ => SessionMode::New,
    };

    let notifications_json: Option<String> = r.get(18)?;
    let notifications: Option<NotificationConfig> = notifications_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .unwrap_or(None);

    Ok(AutomationDef {
        id: r.get(0)?,
        project_id: r.get(1)?,
        name: r.get(2)?,
        description: r.get(3)?,
        enabled: enabled_int != 0,
        trigger,
        defaults: AutomationDefaults {
            provider: r.get(7)?,
            model: r.get(8)?,
            budget_usd: r.get(9)?,
            pipeline: r.get(10)?,
            permission_mode: r.get(11)?,
        },
        session_mode,
        dedicated_conversation_id: r.get(13)?,
        source_path: r.get(14)?,
        last_triggered_at: r.get(15)?,
        created_at: r.get(16)?,
        updated_at: r.get(17)?,
        notifications,
    })
}

// ===========================================================================
//  Steps
// ===========================================================================

pub fn insert_step(conn: &Connection, s: &AutomationStep) -> GroveResult<()> {
    let depends_on_json =
        serde_json::to_string(&s.depends_on).map_err(|e| GroveError::Runtime(e.to_string()))?;
    conn.execute(
        "INSERT INTO automation_steps (
            id, automation_id, step_key, ordinal, objective,
            depends_on, provider, model, budget_usd,
            pipeline, permission_mode, condition,
            created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5,
            ?6, ?7, ?8, ?9,
            ?10, ?11, ?12,
            ?13, ?14
        )",
        params![
            s.id,
            s.automation_id,
            s.step_key,
            s.ordinal,
            s.objective,
            depends_on_json,
            s.provider,
            s.model,
            s.budget_usd,
            s.pipeline,
            s.permission_mode,
            s.condition,
            s.created_at,
            s.updated_at,
        ],
    )?;
    Ok(())
}

pub fn list_steps(conn: &Connection, automation_id: &str) -> GroveResult<Vec<AutomationStep>> {
    let mut stmt = conn.prepare(
        "SELECT id, automation_id, step_key, ordinal, objective,
                depends_on, provider, model, budget_usd,
                pipeline, permission_mode, condition,
                created_at, updated_at
         FROM automation_steps WHERE automation_id = ?1 ORDER BY ordinal",
    )?;
    let rows = stmt
        .query_map([automation_id], map_step_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_step(conn: &Connection, step_id: &str) -> GroveResult<AutomationStep> {
    let row = conn
        .query_row(
            "SELECT id, automation_id, step_key, ordinal, objective,
                    depends_on, provider, model, budget_usd,
                    pipeline, permission_mode, condition,
                    created_at, updated_at
             FROM automation_steps WHERE id = ?1",
            [step_id],
            map_step_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("automation step {step_id}")))
}

pub fn update_step(conn: &Connection, s: &AutomationStep) -> GroveResult<()> {
    let depends_on_json =
        serde_json::to_string(&s.depends_on).map_err(|e| GroveError::Runtime(e.to_string()))?;
    let n = conn.execute(
        "UPDATE automation_steps SET
            step_key = ?1, ordinal = ?2, objective = ?3,
            depends_on = ?4, provider = ?5, model = ?6,
            budget_usd = ?7, pipeline = ?8, permission_mode = ?9,
            condition = ?10,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?11",
        params![
            s.step_key,
            s.ordinal,
            s.objective,
            depends_on_json,
            s.provider,
            s.model,
            s.budget_usd,
            s.pipeline,
            s.permission_mode,
            s.condition,
            s.id,
        ],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("automation step {}", s.id)));
    }
    Ok(())
}

pub fn delete_step(conn: &Connection, step_id: &str) -> GroveResult<()> {
    let n = conn.execute("DELETE FROM automation_steps WHERE id = ?1", [step_id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("automation step {step_id}")));
    }
    Ok(())
}

fn map_step_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationStep> {
    let depends_on_json: Option<String> = r.get(5)?;
    let depends_on: Vec<String> = match depends_on_json {
        Some(ref json) => serde_json::from_str(json).unwrap_or_default(),
        None => Vec::new(),
    };

    Ok(AutomationStep {
        id: r.get(0)?,
        automation_id: r.get(1)?,
        step_key: r.get(2)?,
        ordinal: r.get(3)?,
        objective: r.get(4)?,
        depends_on,
        provider: r.get(6)?,
        model: r.get(7)?,
        budget_usd: r.get(8)?,
        pipeline: r.get(9)?,
        permission_mode: r.get(10)?,
        condition: r.get(11)?,
        created_at: r.get(12)?,
        updated_at: r.get(13)?,
    })
}

// ===========================================================================
//  Automation Runs
// ===========================================================================

pub fn insert_run(conn: &Connection, r: &AutomationRun) -> GroveResult<()> {
    let state = r.state.as_str();
    let trigger_info_json: Option<String> = r
        .trigger_info
        .as_ref()
        .map(|v| serde_json::to_string(v))
        .transpose()
        .map_err(|e| GroveError::Runtime(e.to_string()))?;
    conn.execute(
        "INSERT INTO automation_runs (
            id, automation_id, state, trigger_info,
            conversation_id, started_at, completed_at,
            created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4,
            ?5, ?6, ?7,
            ?8, ?9
        )",
        params![
            r.id,
            r.automation_id,
            state,
            trigger_info_json,
            r.conversation_id,
            r.started_at,
            r.completed_at,
            r.created_at,
            r.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_run(conn: &Connection, run_id: &str) -> GroveResult<AutomationRun> {
    let row = conn
        .query_row(
            "SELECT id, automation_id, state, trigger_info,
                    conversation_id, started_at, completed_at,
                    created_at, updated_at
             FROM automation_runs WHERE id = ?1",
            [run_id],
            map_run_row,
        )
        .optional()?;
    row.ok_or_else(|| GroveError::NotFound(format!("automation run {run_id}")))
}

pub fn list_runs(
    conn: &Connection,
    automation_id: &str,
    limit: i64,
) -> GroveResult<Vec<AutomationRun>> {
    let mut stmt = conn.prepare(
        "SELECT id, automation_id, state, trigger_info,
                conversation_id, started_at, completed_at,
                created_at, updated_at
         FROM automation_runs WHERE automation_id = ?1
         ORDER BY created_at DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![automation_id, limit], map_run_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_run_state(
    conn: &Connection,
    run_id: &str,
    state: &str,
    completed_at: Option<&str>,
) -> GroveResult<()> {
    let n = conn.execute(
        "UPDATE automation_runs SET
            state = ?1, completed_at = ?2,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?3",
        params![state, completed_at, run_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("automation run {run_id}")));
    }
    Ok(())
}

fn map_run_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRun> {
    let state_str: String = r.get(2)?;
    let trigger_info_json: Option<String> = r.get(3)?;

    let state = AutomationRunState::from_str(&state_str).unwrap_or(AutomationRunState::Pending);
    let trigger_info: Option<serde_json::Value> = trigger_info_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .unwrap_or(None);

    Ok(AutomationRun {
        id: r.get(0)?,
        automation_id: r.get(1)?,
        state,
        trigger_info,
        conversation_id: r.get(4)?,
        started_at: r.get(5)?,
        completed_at: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

// ===========================================================================
//  Automation Run Steps
// ===========================================================================

pub fn insert_run_step(conn: &Connection, rs: &AutomationRunStep) -> GroveResult<()> {
    let state = rs.state.as_str();
    let condition_result: Option<i32> = rs.condition_result.map(|b| b as i32);
    conn.execute(
        "INSERT INTO automation_run_steps (
            id, automation_run_id, step_id, step_key, state,
            task_id, run_id, condition_result, error,
            started_at, completed_at, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5,
            ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13
        )",
        params![
            rs.id,
            rs.automation_run_id,
            rs.step_id,
            rs.step_key,
            state,
            rs.task_id,
            rs.run_id,
            condition_result,
            rs.error,
            rs.started_at,
            rs.completed_at,
            rs.created_at,
            rs.updated_at,
        ],
    )?;
    Ok(())
}

pub fn list_run_steps(
    conn: &Connection,
    automation_run_id: &str,
) -> GroveResult<Vec<AutomationRunStep>> {
    let mut stmt = conn.prepare(
        "SELECT id, automation_run_id, step_id, step_key, state,
                task_id, run_id, condition_result, error,
                started_at, completed_at, created_at, updated_at
         FROM automation_run_steps WHERE automation_run_id = ?1
         ORDER BY step_key",
    )?;
    let rows = stmt
        .query_map([automation_run_id], map_run_step_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_run_step_by_task(
    conn: &Connection,
    task_id: &str,
) -> GroveResult<Option<AutomationRunStep>> {
    let row = conn
        .query_row(
            "SELECT id, automation_run_id, step_id, step_key, state,
                    task_id, run_id, condition_result, error,
                    started_at, completed_at, created_at, updated_at
             FROM automation_run_steps WHERE task_id = ?1",
            [task_id],
            map_run_step_row,
        )
        .optional()?;
    Ok(row)
}

pub fn update_run_step(conn: &Connection, rs: &AutomationRunStep) -> GroveResult<()> {
    let state = rs.state.as_str();
    let condition_result: Option<i32> = rs.condition_result.map(|b| b as i32);
    let n = conn.execute(
        "UPDATE automation_run_steps SET
            state = ?1, task_id = ?2, run_id = ?3,
            condition_result = ?4, error = ?5,
            started_at = ?6, completed_at = ?7,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE id = ?8",
        params![
            state,
            rs.task_id,
            rs.run_id,
            condition_result,
            rs.error,
            rs.started_at,
            rs.completed_at,
            rs.id,
        ],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!(
            "automation run step {}",
            rs.id
        )));
    }
    Ok(())
}

fn map_run_step_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationRunStep> {
    let state_str: String = r.get(4)?;
    let condition_result_int: Option<i32> = r.get(7)?;

    let state = StepState::from_str(&state_str).unwrap_or(StepState::Pending);
    let condition_result = condition_result_int.map(|v| v != 0);

    Ok(AutomationRunStep {
        id: r.get(0)?,
        automation_run_id: r.get(1)?,
        step_id: r.get(2)?,
        step_key: r.get(3)?,
        state,
        task_id: r.get(5)?,
        run_id: r.get(6)?,
        condition_result,
        error: r.get(8)?,
        started_at: r.get(9)?,
        completed_at: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

// ===========================================================================
//  Automation Events
// ===========================================================================

pub fn insert_event(
    conn: &Connection,
    event_type: &str,
    payload: &str,
    source: Option<&str>,
    automation_id: Option<&str>,
    automation_run_id: Option<&str>,
) -> GroveResult<()> {
    conn.execute(
        "INSERT INTO automation_events (
            event_type, payload, source,
            automation_id, automation_run_id
        ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            event_type,
            payload,
            source,
            automation_id,
            automation_run_id
        ],
    )?;
    Ok(())
}

pub fn list_events_for_run(
    conn: &Connection,
    automation_run_id: &str,
    limit: i64,
) -> GroveResult<Vec<AutomationEventRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, event_type, payload, source,
                automation_id, automation_run_id, created_at
         FROM automation_events WHERE automation_run_id = ?1
         ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![automation_run_id, limit], |r| {
            Ok(AutomationEventRow {
                id: r.get(0)?,
                event_type: r.get(1)?,
                payload: r.get(2)?,
                source: r.get(3)?,
                automation_id: r.get(4)?,
                automation_run_id: r.get(5)?,
                created_at: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
