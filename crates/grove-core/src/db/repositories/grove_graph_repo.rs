use rusqlite::{Connection, OptionalExtension, params};

use crate::errors::{GroveError, GroveResult};
use crate::grove_graph::GraphConfig;

// ── Row Structs ─────────────────────────────────────────────────────────────

/// All 25 columns from `grove_graphs`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphRow {
    pub id: String,
    pub conversation_id: String,
    pub title: String,
    pub description: Option<String>,
    pub objective: Option<String>,
    pub status: String,
    pub runtime_status: String,
    pub parsing_status: String,
    pub execution_mode: String,
    pub active: bool,
    pub rerun_count: i64,
    pub max_reruns: i64,
    pub phases_created_count: i64,
    pub steps_created_count: i64,
    pub steps_closed_count: i64,
    pub current_phase: Option<String>,
    pub next_step: Option<String>,
    pub progress_summary: Option<String>,
    pub source_document_path: Option<String>,
    pub git_branch: Option<String>,
    pub git_commit_sha: Option<String>,
    pub git_pr_url: Option<String>,
    pub git_merge_status: Option<String>,
    pub pipeline_error: Option<String>,
    pub provider: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// All 22 columns from `graph_phases`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphPhaseRow {
    pub id: String,
    pub graph_id: String,
    pub task_name: String,
    pub task_objective: String,
    pub outcome: Option<String>,
    pub ai_comments: Option<String>,
    pub grade: Option<i64>,
    pub reference_doc_path: Option<String>,
    pub ref_required: bool,
    pub status: String,
    pub validation_status: String,
    pub ordinal: i64,
    pub depends_on_json: String,
    pub git_commit_sha: Option<String>,
    pub conversation_id: Option<String>,
    pub created_run_id: Option<String>,
    pub executed_run_id: Option<String>,
    pub validator_run_id: Option<String>,
    pub judge_run_id: Option<String>,
    pub execution_agent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// All 27 columns from `graph_steps`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphStepRow {
    pub id: String,
    pub phase_id: String,
    pub graph_id: String,
    pub task_name: String,
    pub task_objective: String,
    pub step_type: String,
    pub outcome: Option<String>,
    pub ai_comments: Option<String>,
    pub grade: Option<i64>,
    pub reference_doc_path: Option<String>,
    pub ref_required: bool,
    pub status: String,
    pub ordinal: i64,
    pub execution_mode: String,
    pub depends_on_json: String,
    pub run_iteration: i64,
    pub max_iterations: i64,
    pub judge_feedback_json: String,
    pub builder_run_id: Option<String>,
    pub verdict_run_id: Option<String>,
    pub judge_run_id: Option<String>,
    pub conversation_id: Option<String>,
    pub created_run_id: Option<String>,
    pub executed_run_id: Option<String>,
    pub execution_agent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// All 6 columns from `graph_config`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphConfigRow {
    pub id: String,
    pub graph_id: String,
    pub config_key: String,
    pub config_value: String,
    pub created_at: String,
    pub updated_at: String,
}

// ── Composite Structs ───────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PhaseDetail {
    pub phase: GraphPhaseRow,
    pub steps: Vec<GraphStepRow>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphDetail {
    pub graph: GraphRow,
    pub config: GraphConfig,
    pub phases: Vec<PhaseDetail>,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn now_iso() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

fn gen_id(prefix: &str) -> String {
    format!(
        "{}_{}",
        prefix,
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    )
}

// ── Row Mappers ─────────────────────────────────────────────────────────────

fn map_graph_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GraphRow> {
    let active_int: i64 = r.get(9)?;
    Ok(GraphRow {
        id: r.get(0)?,
        conversation_id: r.get(1)?,
        title: r.get(2)?,
        description: r.get(3)?,
        objective: r.get(4)?,
        status: r.get(5)?,
        runtime_status: r.get(6)?,
        parsing_status: r.get(7)?,
        execution_mode: r.get(8)?,
        active: active_int != 0,
        rerun_count: r.get(10)?,
        max_reruns: r.get(11)?,
        phases_created_count: r.get(12)?,
        steps_created_count: r.get(13)?,
        steps_closed_count: r.get(14)?,
        current_phase: r.get(15)?,
        next_step: r.get(16)?,
        progress_summary: r.get(17)?,
        source_document_path: r.get(18)?,
        git_branch: r.get(19)?,
        git_commit_sha: r.get(20)?,
        git_pr_url: r.get(21)?,
        git_merge_status: r.get(22)?,
        pipeline_error: r.get(23)?,
        provider: r.get(24)?,
        created_at: r.get(25)?,
        updated_at: r.get(26)?,
    })
}

fn map_phase_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GraphPhaseRow> {
    let ref_req_int: i64 = r.get(8)?;
    Ok(GraphPhaseRow {
        id: r.get(0)?,
        graph_id: r.get(1)?,
        task_name: r.get(2)?,
        task_objective: r.get(3)?,
        outcome: r.get(4)?,
        ai_comments: r.get(5)?,
        grade: r.get(6)?,
        reference_doc_path: r.get(7)?,
        ref_required: ref_req_int != 0,
        status: r.get(9)?,
        validation_status: r.get(10)?,
        ordinal: r.get(11)?,
        depends_on_json: r.get(12)?,
        git_commit_sha: r.get(13)?,
        conversation_id: r.get(14)?,
        created_run_id: r.get(15)?,
        executed_run_id: r.get(16)?,
        validator_run_id: r.get(17)?,
        judge_run_id: r.get(18)?,
        execution_agent: r.get(19)?,
        created_at: r.get(20)?,
        updated_at: r.get(21)?,
    })
}

fn map_step_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GraphStepRow> {
    let ref_req_int: i64 = r.get(10)?;
    Ok(GraphStepRow {
        id: r.get(0)?,
        phase_id: r.get(1)?,
        graph_id: r.get(2)?,
        task_name: r.get(3)?,
        task_objective: r.get(4)?,
        step_type: r.get(5)?,
        outcome: r.get(6)?,
        ai_comments: r.get(7)?,
        grade: r.get(8)?,
        reference_doc_path: r.get(9)?,
        ref_required: ref_req_int != 0,
        status: r.get(11)?,
        ordinal: r.get(12)?,
        execution_mode: r.get(13)?,
        depends_on_json: r.get(14)?,
        run_iteration: r.get(15)?,
        max_iterations: r.get(16)?,
        judge_feedback_json: r.get(17)?,
        builder_run_id: r.get(18)?,
        verdict_run_id: r.get(19)?,
        judge_run_id: r.get(20)?,
        conversation_id: r.get(21)?,
        created_run_id: r.get(22)?,
        executed_run_id: r.get(23)?,
        execution_agent: r.get(24)?,
        created_at: r.get(25)?,
        updated_at: r.get(26)?,
    })
}

fn map_config_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<GraphConfigRow> {
    Ok(GraphConfigRow {
        id: r.get(0)?,
        graph_id: r.get(1)?,
        config_key: r.get(2)?,
        config_value: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

// ── Column Lists ────────────────────────────────────────────────────────────

const GRAPH_COLS: &str = "\
    id, conversation_id, title, description, objective, status, runtime_status, \
    parsing_status, execution_mode, active, rerun_count, max_reruns, \
    phases_created_count, steps_created_count, \
    (SELECT COUNT(*) FROM graph_steps WHERE graph_id = grove_graphs.id AND status = 'closed') AS steps_closed_count, \
    current_phase, next_step, \
    progress_summary, source_document_path, git_branch, git_commit_sha, \
    git_pr_url, git_merge_status, pipeline_error, provider, created_at, updated_at";

const PHASE_COLS: &str = "\
    id, graph_id, task_name, task_objective, outcome, ai_comments, grade, \
    reference_doc_path, ref_required, status, validation_status, ordinal, \
    depends_on_json, git_commit_sha, conversation_id, created_run_id, \
    executed_run_id, validator_run_id, judge_run_id, execution_agent, \
    created_at, updated_at";

const STEP_COLS: &str = "\
    id, phase_id, graph_id, task_name, task_objective, step_type, outcome, \
    ai_comments, grade, reference_doc_path, ref_required, status, ordinal, \
    execution_mode, depends_on_json, run_iteration, max_iterations, \
    judge_feedback_json, builder_run_id, verdict_run_id, judge_run_id, \
    conversation_id, created_run_id, executed_run_id, execution_agent, \
    created_at, updated_at";

const CONFIG_COLS: &str = "\
    id, graph_id, config_key, config_value, created_at, updated_at";

// ── Graph CRUD ──────────────────────────────────────────────────────────────

pub fn insert_graph(
    conn: &Connection,
    conversation_id: &str,
    title: &str,
    description: &str,
    provider: Option<&str>,
) -> GroveResult<String> {
    let id = gen_id("gg");
    let now = now_iso();
    conn.execute(
        "INSERT INTO grove_graphs \
            (id, conversation_id, title, description, provider, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, conversation_id, title, description, provider, now, now],
    )?;
    Ok(id)
}

pub fn get_graph(conn: &Connection, graph_id: &str) -> GroveResult<GraphRow> {
    conn.query_row(
        &format!("SELECT {GRAPH_COLS} FROM grove_graphs WHERE id=?1"),
        [graph_id],
        map_graph_row,
    )
    .optional()?
    .ok_or_else(|| GroveError::NotFound(format!("grove_graph {graph_id}")))
}

pub fn list_graphs_for_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<Vec<GraphRow>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {GRAPH_COLS} FROM grove_graphs WHERE conversation_id=?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt
        .query_map([conversation_id], map_graph_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn update_graph_status(conn: &Connection, graph_id: &str, status: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET status=?1, updated_at=?2 WHERE id=?3",
        params![status, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

pub fn delete_graph(conn: &Connection, graph_id: &str) -> GroveResult<()> {
    let n = conn.execute("DELETE FROM grove_graphs WHERE id=?1", [graph_id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

// ── Phase CRUD ──────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn insert_phase(
    conn: &Connection,
    graph_id: &str,
    task_name: &str,
    task_objective: &str,
    ordinal: i64,
    depends_on_json: &str,
    ref_required: bool,
    reference_doc_path: Option<&str>,
) -> GroveResult<String> {
    // Duplicate detection: if graph_id+task_name already exists, return existing id.
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM graph_phases WHERE graph_id=?1 AND task_name=?2",
            params![graph_id, task_name],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(existing_id) = existing {
        return Ok(existing_id);
    }

    let id = gen_id("gp");
    let now = now_iso();
    let ref_req_int: i64 = if ref_required { 1 } else { 0 };
    conn.execute(
        "INSERT INTO graph_phases \
            (id, graph_id, task_name, task_objective, ordinal, depends_on_json, \
             ref_required, reference_doc_path, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            graph_id,
            task_name,
            task_objective,
            ordinal,
            depends_on_json,
            ref_req_int,
            reference_doc_path,
            now,
            now,
        ],
    )?;
    Ok(id)
}

pub fn get_phase(conn: &Connection, phase_id: &str) -> GroveResult<GraphPhaseRow> {
    conn.query_row(
        &format!("SELECT {PHASE_COLS} FROM graph_phases WHERE id=?1"),
        [phase_id],
        map_phase_row,
    )
    .optional()?
    .ok_or_else(|| GroveError::NotFound(format!("graph_phase {phase_id}")))
}

pub fn list_phases(conn: &Connection, graph_id: &str) -> GroveResult<Vec<GraphPhaseRow>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {PHASE_COLS} FROM graph_phases WHERE graph_id=?1 ORDER BY ordinal"
    ))?;
    let rows = stmt
        .query_map([graph_id], map_phase_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn update_phase_status(conn: &Connection, phase_id: &str, status: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_phases SET status=?1, updated_at=?2 WHERE id=?3",
        params![status, now, phase_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

pub fn delete_phase(conn: &Connection, phase_id: &str) -> GroveResult<()> {
    let n = conn.execute("DELETE FROM graph_phases WHERE id=?1", [phase_id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

/// Delete all phases (and their steps via CASCADE) for a graph, then reset
/// the graph's phase/step counters to 0.  Used before a graph creation retry
/// so stale data from a failed attempt doesn't pollute the new plan.
pub fn clear_graph_plan(conn: &Connection, graph_id: &str) -> GroveResult<()> {
    // graph_steps are deleted automatically via ON DELETE CASCADE on graph_phases.
    conn.execute("DELETE FROM graph_phases WHERE graph_id=?1", [graph_id])?;
    let now = now_iso();
    conn.execute(
        "UPDATE grove_graphs \
         SET phases_created_count=0, steps_created_count=0, updated_at=?1 \
         WHERE id=?2",
        params![now, graph_id],
    )?;
    Ok(())
}

// ── Step CRUD ───────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub fn insert_step(
    conn: &Connection,
    phase_id: &str,
    graph_id: &str,
    task_name: &str,
    task_objective: &str,
    ordinal: i64,
    step_type: &str,
    execution_mode: &str,
    depends_on_json: &str,
    ref_required: bool,
    reference_doc_path: Option<&str>,
) -> GroveResult<String> {
    // Duplicate detection: if phase_id+task_name already exists, return existing id.
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM graph_steps WHERE phase_id=?1 AND task_name=?2",
            params![phase_id, task_name],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(existing_id) = existing {
        return Ok(existing_id);
    }

    let id = gen_id("gs");
    let now = now_iso();
    let ref_req_int: i64 = if ref_required { 1 } else { 0 };
    conn.execute(
        "INSERT INTO graph_steps \
            (id, phase_id, graph_id, task_name, task_objective, ordinal, step_type, \
             execution_mode, depends_on_json, ref_required, reference_doc_path, \
             created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            phase_id,
            graph_id,
            task_name,
            task_objective,
            ordinal,
            step_type,
            execution_mode,
            depends_on_json,
            ref_req_int,
            reference_doc_path,
            now,
            now,
        ],
    )?;
    Ok(id)
}

pub fn get_step(conn: &Connection, step_id: &str) -> GroveResult<GraphStepRow> {
    conn.query_row(
        &format!("SELECT {STEP_COLS} FROM graph_steps WHERE id=?1"),
        [step_id],
        map_step_row,
    )
    .optional()?
    .ok_or_else(|| GroveError::NotFound(format!("graph_step {step_id}")))
}

pub fn list_steps(conn: &Connection, phase_id: &str) -> GroveResult<Vec<GraphStepRow>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {STEP_COLS} FROM graph_steps WHERE phase_id=?1 ORDER BY ordinal"
    ))?;
    let rows = stmt
        .query_map([phase_id], map_step_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn list_steps_for_graph(conn: &Connection, graph_id: &str) -> GroveResult<Vec<GraphStepRow>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {STEP_COLS} FROM graph_steps WHERE graph_id=?1 ORDER BY ordinal"
    ))?;
    let rows = stmt
        .query_map([graph_id], map_step_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

pub fn update_step_status(conn: &Connection, step_id: &str, status: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET status=?1, updated_at=?2 WHERE id=?3",
        params![status, now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

pub fn delete_step(conn: &Connection, step_id: &str) -> GroveResult<()> {
    let n = conn.execute("DELETE FROM graph_steps WHERE id=?1", [step_id])?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

// ── Composite & Batch Functions ─────────────────────────────────────────────

/// Load a graph with its config and all phases (each with their steps).
pub fn get_graph_detail(conn: &Connection, graph_id: &str) -> GroveResult<GraphDetail> {
    let graph = get_graph(conn, graph_id)?;
    let config = get_graph_config(conn, graph_id)?;
    let phases = list_phases(conn, graph_id)?;
    let mut phase_details = Vec::with_capacity(phases.len());
    for phase in phases {
        let steps = list_steps(conn, &phase.id)?;
        phase_details.push(PhaseDetail { phase, steps });
    }
    Ok(GraphDetail {
        graph,
        config,
        phases: phase_details,
    })
}

/// Input data for a single step within `populate_graph`.
pub struct PopulateStepData {
    pub task_name: String,
    pub task_objective: String,
    pub ordinal: i64,
    pub step_type: String,
    pub execution_mode: String,
    pub depends_on_json: String,
    pub ref_required: bool,
    pub reference_doc_path: Option<String>,
}

/// Input data for a single phase within `populate_graph`.
pub struct PopulatePhaseData {
    pub task_name: String,
    pub task_objective: String,
    pub ordinal: i64,
    pub depends_on_json: String,
    pub ref_required: bool,
    pub reference_doc_path: Option<String>,
    pub steps: Vec<PopulateStepData>,
}

/// Batch insert phases and their steps for a graph, then update counts.
pub fn populate_graph(
    conn: &Connection,
    graph_id: &str,
    phases_data: &[PopulatePhaseData],
) -> GroveResult<()> {
    for pd in phases_data {
        let phase_id = insert_phase(
            conn,
            graph_id,
            &pd.task_name,
            &pd.task_objective,
            pd.ordinal,
            &pd.depends_on_json,
            pd.ref_required,
            pd.reference_doc_path.as_deref(),
        )?;

        for sd in &pd.steps {
            insert_step(
                conn,
                &phase_id,
                graph_id,
                &sd.task_name,
                &sd.task_objective,
                sd.ordinal,
                &sd.step_type,
                &sd.execution_mode,
                &sd.depends_on_json,
                sd.ref_required,
                sd.reference_doc_path.as_deref(),
            )?;
        }
    }

    update_graph_counts(conn, graph_id)?;
    Ok(())
}

/// Update the current_phase, next_step, and progress_summary fields of a graph.
pub fn update_graph_progress(
    conn: &Connection,
    graph_id: &str,
    current_phase: Option<&str>,
    next_step: Option<&str>,
    progress_summary: Option<&str>,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs \
         SET current_phase=?1, next_step=?2, progress_summary=?3, updated_at=?4 \
         WHERE id=?5",
        params![current_phase, next_step, progress_summary, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

/// Recalculate and update phases_created_count and steps_created_count on a graph.
pub fn update_graph_counts(conn: &Connection, graph_id: &str) -> GroveResult<()> {
    let phase_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM graph_phases WHERE graph_id=?1",
        [graph_id],
        |r| r.get(0),
    )?;
    let step_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM graph_steps WHERE graph_id=?1",
        [graph_id],
        |r| r.get(0),
    )?;
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs \
         SET phases_created_count=?1, steps_created_count=?2, updated_at=?3 \
         WHERE id=?4",
        params![phase_count, step_count, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

// ── Config Functions ────────────────────────────────────────────────────────

/// Upsert all keys from `GraphConfig` into `graph_config`.
pub fn set_graph_config(
    conn: &Connection,
    graph_id: &str,
    config: &GraphConfig,
) -> GroveResult<()> {
    let pairs = config.to_config_pairs();
    let now = now_iso();
    let mut stmt = conn.prepare_cached(
        "INSERT INTO graph_config (id, graph_id, config_key, config_value, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
         ON CONFLICT(graph_id, config_key) DO UPDATE SET \
             config_value=excluded.config_value, \
             updated_at=excluded.updated_at",
    )?;
    for (key, value) in &pairs {
        let row_id = gen_id("gc");
        stmt.execute(params![row_id, graph_id, key, value, now, now])?;
    }
    Ok(())
}

/// Read all key-value pairs for a graph and build a `GraphConfig`.
pub fn get_graph_config(conn: &Connection, graph_id: &str) -> GroveResult<GraphConfig> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT {CONFIG_COLS} FROM graph_config WHERE graph_id=?1"
    ))?;
    let rows: Vec<GraphConfigRow> = stmt
        .query_map([graph_id], map_config_row)?
        .collect::<Result<_, _>>()?;
    let pairs: Vec<(String, String)> = rows
        .into_iter()
        .map(|r| (r.config_key, r.config_value))
        .collect();
    Ok(GraphConfig::from_pairs(&pairs))
}

// ── DAG Queries ─────────────────────────────────────────────────────────────

/// Return steps that are ready to execute: status='open' and every dependency
/// (from `depends_on_json`) is already 'closed'. Steps with no deps are
/// always ready.
pub fn get_ready_steps_for_graph(
    conn: &Connection,
    graph_id: &str,
) -> GroveResult<Vec<GraphStepRow>> {
    let sql = format!(
        "SELECT {STEP_COLS} FROM graph_steps s \
         WHERE s.graph_id = ?1 AND s.status = 'open' \
           AND NOT EXISTS ( \
               SELECT 1 FROM json_each(s.depends_on_json) AS dep \
               WHERE NOT EXISTS ( \
                   SELECT 1 FROM graph_steps d WHERE d.id = dep.value AND d.status = 'closed' \
               ) \
           ) \
         ORDER BY s.ordinal"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt
        .query_map([graph_id], map_step_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Return phases where ALL steps have status='closed' AND validation_status is
/// 'pending' or 'fixing' (after re-opened steps have been reworked and closed
/// again). The phase must have at least one step.
pub fn get_phases_pending_validation(
    conn: &Connection,
    graph_id: &str,
) -> GroveResult<Vec<GraphPhaseRow>> {
    let sql = format!(
        "SELECT {PHASE_COLS} FROM graph_phases p \
         WHERE p.graph_id = ?1 AND p.validation_status IN ('pending', 'fixing') \
           AND EXISTS ( \
               SELECT 1 FROM graph_steps s2 WHERE s2.phase_id = p.id \
           ) \
           AND NOT EXISTS ( \
               SELECT 1 FROM graph_steps s3 WHERE s3.phase_id = p.id AND s3.status != 'closed' \
           ) \
         ORDER BY p.ordinal"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt
        .query_map([graph_id], map_phase_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// True if every phase in the graph has status='closed'.
pub fn all_phases_closed(conn: &Connection, graph_id: &str) -> GroveResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM graph_phases WHERE graph_id=?1 AND status != 'closed'",
        [graph_id],
        |r| r.get(0),
    )?;
    Ok(count == 0)
}

/// True if every phase in the graph has validation_status='passed'.
pub fn all_validations_passed(conn: &Connection, graph_id: &str) -> GroveResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM graph_phases WHERE graph_id=?1 AND validation_status != 'passed'",
        [graph_id],
        |r| r.get(0),
    )?;
    Ok(count == 0)
}

/// True if any step in the graph has status='open' or 'inprogress'.
/// Including 'inprogress' prevents an infinite loop when a worker dies
/// leaving steps stuck — the deadlock handler can then recover them.
pub fn has_any_open_steps(conn: &Connection, graph_id: &str) -> GroveResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM graph_steps WHERE graph_id=?1 AND status IN ('open', 'inprogress')",
        [graph_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

// ── Step Pipeline Tracking ──────────────────────────────────────────────────

pub fn set_step_builder_run(conn: &Connection, step_id: &str, run_id: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET builder_run_id=?1, updated_at=?2 WHERE id=?3",
        params![run_id, now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

pub fn set_step_verdict_run(conn: &Connection, step_id: &str, run_id: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET verdict_run_id=?1, updated_at=?2 WHERE id=?3",
        params![run_id, now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

pub fn set_step_judge_run(
    conn: &Connection,
    step_id: &str,
    run_id: &str,
    grade: Option<i64>,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET judge_run_id=?1, grade=?2, updated_at=?3 WHERE id=?4",
        params![run_id, grade, now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

pub fn set_step_outcome(
    conn: &Connection,
    step_id: &str,
    outcome: &str,
    ai_comments: &str,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET outcome=?1, ai_comments=?2, updated_at=?3 WHERE id=?4",
        params![outcome, ai_comments, now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

pub fn set_step_closed(
    conn: &Connection,
    step_id: &str,
    outcome: &str,
    ai_comments: &str,
    grade: i64,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET status='closed', outcome=?1, ai_comments=?2, grade=?3, updated_at=?4 WHERE id=?5",
        params![outcome, ai_comments, grade, now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

pub fn set_step_failed(conn: &Connection, step_id: &str, ai_comments: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET status='failed', ai_comments=?1, updated_at=?2 WHERE id=?3",
        params![ai_comments, now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

/// Reset a step to 'open', clearing builder/verdict/judge run IDs.
/// Preserves `judge_feedback_json` and `run_iteration`.
pub fn reopen_step(conn: &Connection, step_id: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET status='open', \
         builder_run_id=NULL, verdict_run_id=NULL, judge_run_id=NULL, \
         outcome=NULL, ai_comments=NULL, grade=NULL, \
         updated_at=?1 WHERE id=?2",
        params![now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    Ok(())
}

/// Increment `run_iteration` by 1 and return the new value.
pub fn increment_step_run_iteration(conn: &Connection, step_id: &str) -> GroveResult<i64> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_steps SET run_iteration = run_iteration + 1, updated_at=?1 WHERE id=?2",
        params![now, step_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_step {step_id}")));
    }
    let new_val: i64 = conn.query_row(
        "SELECT run_iteration FROM graph_steps WHERE id=?1",
        [step_id],
        |r| r.get(0),
    )?;
    Ok(new_val)
}

/// Read `judge_feedback_json`, parse as `Vec<String>`, append `feedback`,
/// and write it back.
pub fn append_judge_feedback(conn: &Connection, step_id: &str, feedback: &str) -> GroveResult<()> {
    let current: String = conn
        .query_row(
            "SELECT judge_feedback_json FROM graph_steps WHERE id=?1",
            [step_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| GroveError::NotFound(format!("graph_step {step_id}")))?;

    let mut arr: Vec<String> = serde_json::from_str(&current).unwrap_or_default();
    arr.push(feedback.to_string());
    let updated = serde_json::to_string(&arr)?;

    let now = now_iso();
    conn.execute(
        "UPDATE graph_steps SET judge_feedback_json=?1, updated_at=?2 WHERE id=?3",
        params![updated, now, step_id],
    )?;
    Ok(())
}

/// Get a step along with its parsed feedback array.
pub fn get_step_with_feedback(
    conn: &Connection,
    step_id: &str,
) -> GroveResult<(GraphStepRow, Vec<String>)> {
    let step = get_step(conn, step_id)?;
    let feedback: Vec<String> = serde_json::from_str(&step.judge_feedback_json).unwrap_or_default();
    Ok((step, feedback))
}

// ── Phase Pipeline Tracking ─────────────────────────────────────────────────

pub fn set_phase_validation_status(
    conn: &Connection,
    phase_id: &str,
    status: &str,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_phases SET validation_status=?1, updated_at=?2 WHERE id=?3",
        params![status, now, phase_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

pub fn set_phase_validator_run(conn: &Connection, phase_id: &str, run_id: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_phases SET validator_run_id=?1, updated_at=?2 WHERE id=?3",
        params![run_id, now, phase_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

pub fn set_phase_judge_run(
    conn: &Connection,
    phase_id: &str,
    run_id: &str,
    grade: Option<i64>,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_phases SET judge_run_id=?1, grade=?2, updated_at=?3 WHERE id=?4",
        params![run_id, grade, now, phase_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

pub fn set_phase_outcome(
    conn: &Connection,
    phase_id: &str,
    outcome: &str,
    ai_comments: &str,
    grade: i64,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_phases SET outcome=?1, ai_comments=?2, grade=?3, updated_at=?4 WHERE id=?5",
        params![outcome, ai_comments, grade, now, phase_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

pub fn set_phase_closed(
    conn: &Connection,
    phase_id: &str,
    outcome: &str,
    grade: i64,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_phases SET status='closed', validation_status='passed', \
         outcome=?1, grade=?2, updated_at=?3 WHERE id=?4",
        params![outcome, grade, now, phase_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

/// Reopen the given steps and reset the phase's validation_status to 'pending'.
pub fn reopen_steps_for_phase(
    conn: &Connection,
    phase_id: &str,
    step_ids: &[String],
) -> GroveResult<()> {
    for sid in step_ids {
        reopen_step(conn, sid)?;
    }
    set_phase_validation_status(conn, phase_id, "pending")?;
    Ok(())
}

// ── Git Tracking ────────────────────────────────────────────────────────────

pub fn set_graph_git_branch(conn: &Connection, graph_id: &str, branch: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET git_branch=?1, updated_at=?2 WHERE id=?3",
        params![branch, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

pub fn set_phase_git_commit(
    conn: &Connection,
    phase_id: &str,
    commit_sha: &str,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_phases SET git_commit_sha=?1, updated_at=?2 WHERE id=?3",
        params![commit_sha, now, phase_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("graph_phase {phase_id}")));
    }
    Ok(())
}

pub fn set_graph_git_final(
    conn: &Connection,
    graph_id: &str,
    commit_sha: &str,
    pr_url: Option<&str>,
    merge_status: &str,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET git_commit_sha=?1, git_pr_url=?2, git_merge_status=?3, \
         updated_at=?4 WHERE id=?5",
        params![commit_sha, pr_url, merge_status, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

// ── Runtime + Active Graph ──────────────────────────────────────────────────

pub fn set_runtime_status(conn: &Connection, graph_id: &str, status: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET runtime_status=?1, updated_at=?2 WHERE id=?3",
        params![status, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

/// Return the active graph for a conversation (WHERE active=1 LIMIT 1).
pub fn get_active_graph(conn: &Connection, conversation_id: &str) -> GroveResult<Option<GraphRow>> {
    let row = conn
        .query_row(
            &format!(
                "SELECT {GRAPH_COLS} FROM grove_graphs \
                 WHERE conversation_id=?1 AND active=1 LIMIT 1"
            ),
            [conversation_id],
            map_graph_row,
        )
        .optional()?;
    Ok(row)
}

/// Deactivate all graphs in the same conversation, then activate this one.
pub fn set_active_graph(conn: &Connection, graph_id: &str) -> GroveResult<()> {
    let now = now_iso();
    // First get the conversation_id for this graph.
    let conv_id: String = conn
        .query_row(
            "SELECT conversation_id FROM grove_graphs WHERE id=?1",
            [graph_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| GroveError::NotFound(format!("grove_graph {graph_id}")))?;

    conn.execute(
        "UPDATE grove_graphs SET active=0, updated_at=?1 WHERE conversation_id=?2",
        params![now, conv_id],
    )?;
    conn.execute(
        "UPDATE grove_graphs SET active=1, updated_at=?1 WHERE id=?2",
        params![now, graph_id],
    )?;
    Ok(())
}

/// Check whether any graph in the given conversation currently has
/// `runtime_status = 'running'`.
pub fn has_running_graph_in_conversation(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM grove_graphs \
         WHERE conversation_id = ?1 AND runtime_status = 'running'",
        [conversation_id],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Return the next queued graph for a conversation, ordered by creation time.
/// Returns `None` if no graph is queued.
pub fn get_next_queued_graph(
    conn: &Connection,
    conversation_id: &str,
) -> GroveResult<Option<GraphRow>> {
    let row = conn
        .query_row(
            &format!(
                "SELECT {GRAPH_COLS} FROM grove_graphs \
                 WHERE conversation_id = ?1 AND runtime_status = 'queued' \
                 ORDER BY created_at ASC LIMIT 1"
            ),
            [conversation_id],
            map_graph_row,
        )
        .optional()?;
    Ok(row)
}

/// Return the next queued graph across ALL conversations, but only if that
/// conversation has no graph currently running.
pub fn get_next_queued_graph_any_conversation(conn: &Connection) -> GroveResult<Option<GraphRow>> {
    let row = conn
        .query_row(
            &format!(
                "SELECT {GRAPH_COLS} FROM grove_graphs \
                 WHERE grove_graphs.runtime_status = 'queued' \
                 AND NOT EXISTS ( \
                     SELECT 1 FROM grove_graphs g2 \
                     WHERE g2.conversation_id = grove_graphs.conversation_id \
                     AND g2.runtime_status = 'running' \
                 ) \
                 ORDER BY grove_graphs.created_at ASC LIMIT 1"
            ),
            [],
            map_graph_row,
        )
        .optional()?;
    Ok(row)
}

/// Increment `rerun_count` by 1 and return the new value.
pub fn increment_rerun_count(conn: &Connection, graph_id: &str) -> GroveResult<i64> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET rerun_count = rerun_count + 1, updated_at=?1 WHERE id=?2",
        params![now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    let new_val: i64 = conn.query_row(
        "SELECT rerun_count FROM grove_graphs WHERE id=?1",
        [graph_id],
        |r| r.get(0),
    )?;
    Ok(new_val)
}

/// True if rerun_count < max_reruns.
pub fn can_rerun(conn: &Connection, graph_id: &str) -> GroveResult<bool> {
    let (rerun_count, max_reruns): (i64, i64) = conn
        .query_row(
            "SELECT rerun_count, max_reruns FROM grove_graphs WHERE id=?1",
            [graph_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| GroveError::NotFound(format!("grove_graph {graph_id}")))?;
    Ok(rerun_count < max_reruns)
}

// ── Additional ──────────────────────────────────────────────────────────────

pub fn set_graph_execution_mode(conn: &Connection, graph_id: &str, mode: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET execution_mode=?1, updated_at=?2 WHERE id=?3",
        params![mode, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

pub fn set_graph_parsing_status(
    conn: &Connection,
    graph_id: &str,
    status: &str,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET parsing_status=?1, updated_at=?2 WHERE id=?3",
        params![status, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

/// Set the `source_document_path` field on a graph.
pub fn set_source_document_path(conn: &Connection, graph_id: &str, path: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET source_document_path=?1, updated_at=?2 WHERE id=?3",
        params![path, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

pub fn set_graph_objective(conn: &Connection, graph_id: &str, objective: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET objective=?1, updated_at=?2 WHERE id=?3",
        params![objective, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

pub fn set_graph_pipeline_error(
    conn: &Connection,
    graph_id: &str,
    error: Option<&str>,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET pipeline_error=?1, updated_at=?2 WHERE id=?3",
        params![error, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

pub fn set_graph_title(conn: &Connection, graph_id: &str, title: &str) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE grove_graphs SET title=?1, updated_at=?2 WHERE id=?3",
        params![title, now, graph_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("grove_graph {graph_id}")));
    }
    Ok(())
}

/// Return steps where ref_required=1 AND reference_doc_path IS NULL.
pub fn get_steps_missing_refs(conn: &Connection, graph_id: &str) -> GroveResult<Vec<GraphStepRow>> {
    let sql = format!(
        "SELECT {STEP_COLS} FROM graph_steps \
         WHERE graph_id=?1 AND ref_required=1 AND reference_doc_path IS NULL \
         ORDER BY ordinal"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt
        .query_map([graph_id], map_step_row)?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

// ── Graph Clarifications ────────────────────────────────────────────────────

/// A clarification question generated during the readiness check.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GraphClarification {
    pub id: String,
    pub graph_id: String,
    pub question: String,
    pub answer: Option<String>,
    pub answered: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Insert a clarification question for a graph.
pub fn insert_clarification(
    conn: &Connection,
    graph_id: &str,
    question: &str,
) -> GroveResult<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_iso();
    conn.execute(
        "INSERT INTO graph_clarifications (id, graph_id, question, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, graph_id, question, now, now],
    )?;
    Ok(id)
}

/// List all clarification questions for a graph.
pub fn list_clarifications(
    conn: &Connection,
    graph_id: &str,
) -> GroveResult<Vec<GraphClarification>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, graph_id, question, answer, answered, created_at, updated_at \
         FROM graph_clarifications WHERE graph_id=?1 ORDER BY created_at",
    )?;
    let rows = stmt
        .query_map([graph_id], |r| {
            let answered_int: i64 = r.get(4)?;
            Ok(GraphClarification {
                id: r.get(0)?,
                graph_id: r.get(1)?,
                question: r.get(2)?,
                answer: r.get(3)?,
                answered: answered_int != 0,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// List unanswered clarification questions for a graph.
pub fn list_unanswered_clarifications(
    conn: &Connection,
    graph_id: &str,
) -> GroveResult<Vec<GraphClarification>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, graph_id, question, answer, answered, created_at, updated_at \
         FROM graph_clarifications WHERE graph_id=?1 AND answered=0 ORDER BY created_at",
    )?;
    let rows = stmt
        .query_map([graph_id], |r| {
            Ok(GraphClarification {
                id: r.get(0)?,
                graph_id: r.get(1)?,
                question: r.get(2)?,
                answer: r.get(3)?,
                answered: false,
                created_at: r.get(5)?,
                updated_at: r.get(6)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Submit an answer for a clarification question.
pub fn answer_clarification(
    conn: &Connection,
    clarification_id: &str,
    answer: &str,
) -> GroveResult<()> {
    let now = now_iso();
    let n = conn.execute(
        "UPDATE graph_clarifications SET answer=?1, answered=1, updated_at=?2 WHERE id=?3",
        params![answer, now, clarification_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!(
            "graph_clarification {clarification_id}"
        )));
    }
    Ok(())
}

/// Delete all clarification questions for a graph (used on restart/reset).
pub fn clear_clarifications(conn: &Connection, graph_id: &str) -> GroveResult<()> {
    conn.execute(
        "DELETE FROM graph_clarifications WHERE graph_id=?1",
        [graph_id],
    )?;
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let dir = tempfile::TempDir::new().unwrap();
        crate::db::initialize(dir.path()).unwrap();
        crate::db::DbHandle::new(dir.path()).connect().unwrap()
    }

    /// Insert a minimal conversation row to satisfy FKs.
    fn seed_conversation(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO conversations (id, project_id, state, conversation_kind, \
             remote_registration_state, created_at, updated_at) \
             VALUES (?1, 'proj1', 'active', 'run', 'none', \
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [id],
        )
        .unwrap();
    }

    // ── Graph tests ─────────────────────────────────────────────────────────

    #[test]
    fn insert_and_get_graph() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let id = insert_graph(&conn, "conv1", "My Graph", "A test graph", None).unwrap();
        assert!(id.starts_with("gg_"));

        let row = get_graph(&conn, &id).unwrap();
        assert_eq!(row.title, "My Graph");
        assert_eq!(row.description.as_deref(), Some("A test graph"));
        assert_eq!(row.status, "open");
        assert_eq!(row.runtime_status, "idle");
        assert_eq!(row.parsing_status, "pending");
        assert_eq!(row.execution_mode, "sequential");
        assert!(row.active);
        assert_eq!(row.rerun_count, 0);
        assert_eq!(row.max_reruns, 3);
        assert_eq!(row.phases_created_count, 0);
        assert_eq!(row.steps_created_count, 0);
    }

    #[test]
    fn get_graph_not_found() {
        let conn = test_db();
        let result = get_graph(&conn, "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("grove_graph"));
    }

    #[test]
    fn list_graphs_for_conversation_ordered() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let id1 = insert_graph(&conn, "conv1", "First", "desc1", None).unwrap();
        let id2 = insert_graph(&conn, "conv1", "Second", "desc2", None).unwrap();

        let list = list_graphs_for_conversation(&conn, "conv1").unwrap();
        assert_eq!(list.len(), 2);
        // DESC order: most recent first
        assert_eq!(list[0].id, id2);
        assert_eq!(list[1].id, id1);
    }

    #[test]
    fn update_graph_status_works() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let id = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        update_graph_status(&conn, &id, "inprogress").unwrap();
        let row = get_graph(&conn, &id).unwrap();
        assert_eq!(row.status, "inprogress");
    }

    #[test]
    fn update_graph_status_not_found() {
        let conn = test_db();
        assert!(update_graph_status(&conn, "nope", "open").is_err());
    }

    #[test]
    fn delete_graph_cascades() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "phase1", "obj", 0, "[]", false, None).unwrap();
        insert_step(
            &conn, &pid, &gid, "step1", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();

        delete_graph(&conn, &gid).unwrap();
        assert!(get_graph(&conn, &gid).is_err());
        // CASCADE should have removed phases and steps
        assert!(get_phase(&conn, &pid).is_err());
    }

    // ── Phase tests ─────────────────────────────────────────────────────────

    #[test]
    fn insert_and_get_phase() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        let pid = insert_phase(
            &conn,
            &gid,
            "Setup Phase",
            "Set things up",
            0,
            "[]",
            true,
            Some("/docs/ref.md"),
        )
        .unwrap();
        assert!(pid.starts_with("gp_"));

        let row = get_phase(&conn, &pid).unwrap();
        assert_eq!(row.task_name, "Setup Phase");
        assert_eq!(row.task_objective, "Set things up");
        assert_eq!(row.ordinal, 0);
        assert!(row.ref_required);
        assert_eq!(row.reference_doc_path.as_deref(), Some("/docs/ref.md"));
        assert_eq!(row.status, "open");
        assert_eq!(row.validation_status, "pending");
        assert_eq!(row.depends_on_json, "[]");
    }

    #[test]
    fn insert_phase_duplicate_returns_existing() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        let id1 = insert_phase(&conn, &gid, "dup", "obj", 0, "[]", false, None).unwrap();
        let id2 = insert_phase(&conn, &gid, "dup", "obj2", 1, "[]", false, None).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn list_phases_ordered_by_ordinal() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        insert_phase(&conn, &gid, "B", "obj", 2, "[]", false, None).unwrap();
        insert_phase(&conn, &gid, "A", "obj", 0, "[]", false, None).unwrap();
        insert_phase(&conn, &gid, "C", "obj", 1, "[]", false, None).unwrap();

        let list = list_phases(&conn, &gid).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].task_name, "A");
        assert_eq!(list[1].task_name, "C");
        assert_eq!(list[2].task_name, "B");
    }

    #[test]
    fn update_phase_status_works() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();

        update_phase_status(&conn, &pid, "inprogress").unwrap();
        let row = get_phase(&conn, &pid).unwrap();
        assert_eq!(row.status, "inprogress");
    }

    #[test]
    fn delete_phase_works() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();

        delete_phase(&conn, &pid).unwrap();
        assert!(get_phase(&conn, &pid).is_err());
    }

    // ── Step tests ──────────────────────────────────────────────────────────

    #[test]
    fn insert_and_get_step() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();

        let sid = insert_step(
            &conn,
            &pid,
            &gid,
            "Implement API",
            "Build the endpoint",
            0,
            "code",
            "auto",
            "[\"dep1\"]",
            true,
            Some("/docs/api.md"),
        )
        .unwrap();
        assert!(sid.starts_with("gs_"));

        let row = get_step(&conn, &sid).unwrap();
        assert_eq!(row.task_name, "Implement API");
        assert_eq!(row.task_objective, "Build the endpoint");
        assert_eq!(row.step_type, "code");
        assert_eq!(row.execution_mode, "auto");
        assert!(row.ref_required);
        assert_eq!(row.reference_doc_path.as_deref(), Some("/docs/api.md"));
        assert_eq!(row.status, "open");
        assert_eq!(row.run_iteration, 0);
        assert_eq!(row.max_iterations, 3);
        assert_eq!(row.judge_feedback_json, "[]");
        assert_eq!(row.depends_on_json, "[\"dep1\"]");
    }

    #[test]
    fn insert_step_duplicate_returns_existing() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();

        let id1 = insert_step(
            &conn, &pid, &gid, "dup", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();
        let id2 = insert_step(
            &conn, &pid, &gid, "dup", "obj2", 1, "test", "manual", "[]", true, None,
        )
        .unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn list_steps_ordered() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();

        insert_step(
            &conn, &pid, &gid, "B", "obj", 2, "code", "auto", "[]", false, None,
        )
        .unwrap();
        insert_step(
            &conn, &pid, &gid, "A", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();
        insert_step(
            &conn, &pid, &gid, "C", "obj", 1, "code", "auto", "[]", false, None,
        )
        .unwrap();

        let list = list_steps(&conn, &pid).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].task_name, "A");
        assert_eq!(list[1].task_name, "C");
        assert_eq!(list[2].task_name, "B");
    }

    #[test]
    fn list_steps_for_graph_works() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let p1 = insert_phase(&conn, &gid, "p1", "obj", 0, "[]", false, None).unwrap();
        let p2 = insert_phase(&conn, &gid, "p2", "obj", 1, "[]", false, None).unwrap();

        insert_step(
            &conn, &p1, &gid, "s1", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();
        insert_step(
            &conn, &p2, &gid, "s2", "obj", 1, "code", "auto", "[]", false, None,
        )
        .unwrap();

        let list = list_steps_for_graph(&conn, &gid).unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn update_step_status_works() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();
        let sid = insert_step(
            &conn, &pid, &gid, "s", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();

        update_step_status(&conn, &sid, "inprogress").unwrap();
        let row = get_step(&conn, &sid).unwrap();
        assert_eq!(row.status, "inprogress");
    }

    #[test]
    fn delete_step_works() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();
        let sid = insert_step(
            &conn, &pid, &gid, "s", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();

        delete_step(&conn, &sid).unwrap();
        assert!(get_step(&conn, &sid).is_err());
    }

    // ── Composite tests ─────────────────────────────────────────────────────

    #[test]
    fn get_graph_detail_assembles_correctly() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();
        insert_step(
            &conn, &pid, &gid, "s1", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();
        insert_step(
            &conn, &pid, &gid, "s2", "obj", 1, "test", "auto", "[]", false, None,
        )
        .unwrap();

        let detail = get_graph_detail(&conn, &gid).unwrap();
        assert_eq!(detail.graph.id, gid);
        assert_eq!(detail.phases.len(), 1);
        assert_eq!(detail.phases[0].steps.len(), 2);
    }

    #[test]
    fn populate_graph_batch_inserts() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        let phases = vec![
            PopulatePhaseData {
                task_name: "Phase A".to_string(),
                task_objective: "Obj A".to_string(),
                ordinal: 0,
                depends_on_json: "[]".to_string(),
                ref_required: false,
                reference_doc_path: None,
                steps: vec![
                    PopulateStepData {
                        task_name: "Step A1".to_string(),
                        task_objective: "Obj A1".to_string(),
                        ordinal: 0,
                        step_type: "code".to_string(),
                        execution_mode: "auto".to_string(),
                        depends_on_json: "[]".to_string(),
                        ref_required: false,
                        reference_doc_path: None,
                    },
                    PopulateStepData {
                        task_name: "Step A2".to_string(),
                        task_objective: "Obj A2".to_string(),
                        ordinal: 1,
                        step_type: "test".to_string(),
                        execution_mode: "manual".to_string(),
                        depends_on_json: "[]".to_string(),
                        ref_required: true,
                        reference_doc_path: Some("/docs/test.md".to_string()),
                    },
                ],
            },
            PopulatePhaseData {
                task_name: "Phase B".to_string(),
                task_objective: "Obj B".to_string(),
                ordinal: 1,
                depends_on_json: "[\"Phase A\"]".to_string(),
                ref_required: false,
                reference_doc_path: None,
                steps: vec![],
            },
        ];

        populate_graph(&conn, &gid, &phases).unwrap();

        let row = get_graph(&conn, &gid).unwrap();
        assert_eq!(row.phases_created_count, 2);
        assert_eq!(row.steps_created_count, 2);

        let phase_list = list_phases(&conn, &gid).unwrap();
        assert_eq!(phase_list.len(), 2);
        let step_list = list_steps(&conn, &phase_list[0].id).unwrap();
        assert_eq!(step_list.len(), 2);
    }

    #[test]
    fn update_graph_progress_works() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        update_graph_progress(
            &conn,
            &gid,
            Some("phase_1"),
            Some("step_2"),
            Some("50% done"),
        )
        .unwrap();

        let row = get_graph(&conn, &gid).unwrap();
        assert_eq!(row.current_phase.as_deref(), Some("phase_1"));
        assert_eq!(row.next_step.as_deref(), Some("step_2"));
        assert_eq!(row.progress_summary.as_deref(), Some("50% done"));
    }

    #[test]
    fn update_graph_counts_recalculates() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();
        let pid = insert_phase(&conn, &gid, "p", "obj", 0, "[]", false, None).unwrap();
        insert_step(
            &conn, &pid, &gid, "s1", "obj", 0, "code", "auto", "[]", false, None,
        )
        .unwrap();
        insert_step(
            &conn, &pid, &gid, "s2", "obj", 1, "code", "auto", "[]", false, None,
        )
        .unwrap();

        update_graph_counts(&conn, &gid).unwrap();
        let row = get_graph(&conn, &gid).unwrap();
        assert_eq!(row.phases_created_count, 1);
        assert_eq!(row.steps_created_count, 2);
    }

    // ── Config tests ────────────────────────────────────────────────────────

    #[test]
    fn set_and_get_graph_config() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        let cfg = GraphConfig {
            doc_prd: true,
            platform_frontend: true,
            arch_saas: true,
            ..Default::default()
        };

        set_graph_config(&conn, &gid, &cfg).unwrap();
        let loaded = get_graph_config(&conn, &gid).unwrap();
        assert!(loaded.doc_prd);
        assert!(loaded.platform_frontend);
        assert!(loaded.arch_saas);
        assert!(!loaded.doc_system_design);
        assert!(!loaded.arch_dlib);
    }

    #[test]
    fn set_graph_config_upserts() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        let cfg = GraphConfig {
            doc_prd: true,
            ..Default::default()
        };
        set_graph_config(&conn, &gid, &cfg).unwrap();

        // Now flip doc_prd off and enable another
        let cfg = GraphConfig {
            arch_dlib: true,
            ..Default::default()
        };
        set_graph_config(&conn, &gid, &cfg).unwrap();

        let loaded = get_graph_config(&conn, &gid).unwrap();
        assert!(!loaded.doc_prd);
        assert!(loaded.arch_dlib);
    }

    #[test]
    fn get_graph_config_returns_default_when_empty() {
        let conn = test_db();
        seed_conversation(&conn, "conv1");
        let gid = insert_graph(&conn, "conv1", "G", "d", None).unwrap();

        let loaded = get_graph_config(&conn, &gid).unwrap();
        assert!(!loaded.doc_prd);
        assert!(!loaded.platform_frontend);
    }
}
