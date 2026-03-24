use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior};

use crate::config::grove_dir;
use crate::errors::GroveResult;

pub mod connection;
pub mod integrity;
pub mod pool;
pub mod pragma;
pub mod repositories;
pub mod test_helpers;

pub use pool::DbPool;

const MIGRATION_0001: &str = include_str!("../../../../migrations/0001_init.sql");

/// Upgrade migration for databases created with older Grove versions (schema < 27).
/// Adds new columns to conversations and rewrites merge_queue for conversation-level merging.
/// All statements are idempotent (ADD COLUMN is skipped if column exists via skip_existing_add_columns).
const MIGRATION_0027_UPGRADE: &str = "\
ALTER TABLE conversations ADD COLUMN branch_name TEXT;\n\
ALTER TABLE conversations ADD COLUMN worktree_path TEXT;\n\
DROP TABLE IF EXISTS worktree_pool;\n\
UPDATE meta SET value = '27' WHERE key = 'schema_version';\n\
";

/// Upgrade migration for databases created before project source metadata existed.
const MIGRATION_0028_UPGRADE: &str = "\
ALTER TABLE projects ADD COLUMN source_kind TEXT NOT NULL DEFAULT 'local';\n\
ALTER TABLE projects ADD COLUMN source_details_json TEXT;\n\
UPDATE meta SET value = '28' WHERE key = 'schema_version';\n\
";

/// Upgrade migration for publish lifecycle metadata and the `publishing` run state.
const MIGRATION_0029_UPGRADE: &str = "\
ALTER TABLE runs ADD COLUMN publish_status TEXT NOT NULL DEFAULT 'pending_retry';\n\
ALTER TABLE runs ADD COLUMN publish_error TEXT;\n\
ALTER TABLE runs ADD COLUMN final_commit_sha TEXT;\n\
ALTER TABLE runs ADD COLUMN pr_url TEXT;\n\
ALTER TABLE runs ADD COLUMN published_at TEXT;\n\
ALTER TABLE tasks ADD COLUMN publish_status TEXT;\n\
ALTER TABLE tasks ADD COLUMN publish_error TEXT;\n\
ALTER TABLE tasks ADD COLUMN final_commit_sha TEXT;\n\
ALTER TABLE tasks ADD COLUMN pr_url TEXT;\n\
UPDATE runs SET publish_status = CASE \
    WHEN state = 'completed' THEN 'published' \
    WHEN state = 'failed' THEN 'failed' \
    ELSE 'pending_retry' \
END WHERE publish_status = 'pending_retry';\n\
UPDATE tasks SET publish_status = CASE \
    WHEN state = 'completed' THEN 'published' \
    WHEN state = 'failed' THEN 'failed' \
    ELSE NULL \
END WHERE publish_status IS NULL;\n\
DROP INDEX IF EXISTS idx_active_run_per_conv;\n\
CREATE UNIQUE INDEX IF NOT EXISTS idx_active_run_per_conv ON runs(conversation_id) WHERE state IN ('executing','planning','verifying','publishing','merging');\n\
UPDATE meta SET value = '29' WHERE key = 'schema_version';\n\
";

/// Upgrade migration for persisted conversation branch registration state.
const MIGRATION_0030_UPGRADE: &str = "\
ALTER TABLE conversations ADD COLUMN remote_branch_name TEXT;\n\
ALTER TABLE conversations ADD COLUMN remote_registration_state TEXT NOT NULL DEFAULT 'local_only';\n\
ALTER TABLE conversations ADD COLUMN remote_registration_error TEXT;\n\
ALTER TABLE conversations ADD COLUMN remote_registered_at TEXT;\n\
UPDATE meta SET value = '30' WHERE key = 'schema_version';\n\
";

// Migration 0031 is applied via `fix_runs_check_constraint` — it rewrites
// the runs table schema in sqlite_master to include 'publishing' in the
// CHECK constraint without dropping/recreating the table (which would
// CASCADE-delete sessions, events, etc.).

/// Migration 0035: add `resume_provider_session_id` to the tasks table.
///
/// When a task is queued with a prior run's provider_session_id (e.g. codex
/// thread_id), this column carries it to the drain thread so the next run can
/// resume the same provider session instead of starting fresh.
const MIGRATION_0035_RESUME_SESSION: &str = "\
ALTER TABLE tasks ADD COLUMN resume_provider_session_id TEXT;\n\
UPDATE meta SET value = '35' WHERE key = 'schema_version';\n\
";

/// Migration 0036: add performance indexes for hot query paths.
const MIGRATION_0036_PERF_INDEXES: &str = "\
CREATE INDEX IF NOT EXISTS idx_projects_root_path ON projects(root_path);\n\
CREATE INDEX IF NOT EXISTS idx_issues_project_updated ON issues(project_id, updated_at DESC);\n\
CREATE INDEX IF NOT EXISTS idx_issues_provider_ext_project ON issues(provider, external_id, project_id);\n\
CREATE INDEX IF NOT EXISTS idx_runs_created ON runs(created_at DESC);\n\
CREATE INDEX IF NOT EXISTS idx_runs_conversation ON runs(conversation_id, created_at DESC);\n\
UPDATE meta SET value = '36' WHERE key = 'schema_version';\n\
";

/// Migration 0037: phase-based execution support.
///
/// Adds pipeline/agent tracking to runs and a phase_checkpoints table
/// for gate decisions (approve/reject/skip).
const MIGRATION_0037_PHASE_EXECUTION: &str = "\
ALTER TABLE runs ADD COLUMN pipeline TEXT;\n\
ALTER TABLE runs ADD COLUMN current_agent TEXT;\n\
CREATE TABLE IF NOT EXISTS phase_checkpoints (\n\
    id           INTEGER PRIMARY KEY AUTOINCREMENT,\n\
    run_id       TEXT NOT NULL,\n\
    agent        TEXT NOT NULL,\n\
    status       TEXT NOT NULL DEFAULT 'pending',\n\
    decision     TEXT,\n\
    decided_at   TEXT,\n\
    artifact_path TEXT,\n\
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_phase_checkpoints_run ON phase_checkpoints(run_id);\n\
UPDATE meta SET value = '37' WHERE key = 'schema_version';\n\
";

/// Migration 0038: stream_events table for real-time agent output persistence.
const MIGRATION_0038_STREAM_EVENTS: &str = "\
CREATE TABLE IF NOT EXISTS stream_events (\n\
    id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
    run_id TEXT NOT NULL,\n\
    session_id TEXT,\n\
    kind TEXT NOT NULL,\n\
    content_json TEXT NOT NULL,\n\
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_stream_events_run ON stream_events(run_id, id);\n\
UPDATE meta SET value = '38' WHERE key = 'schema_version';\n\
";

/// Migration 0039: run_artifacts table for tracking files produced by agents.
const MIGRATION_0039_RUN_ARTIFACTS: &str = "\
CREATE TABLE IF NOT EXISTS run_artifacts (\n\
    id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
    run_id TEXT NOT NULL,\n\
    agent TEXT NOT NULL,\n\
    filename TEXT NOT NULL,\n\
    content_hash TEXT NOT NULL,\n\
    size_bytes INTEGER NOT NULL,\n\
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_run_artifacts_run ON run_artifacts(run_id, agent);\n\
UPDATE meta SET value = '39' WHERE key = 'schema_version';\n\
";

/// Migration 0040: qa_messages table for bidirectional agent-user Q&A.
const MIGRATION_0040_QA_MESSAGES: &str = "\
CREATE TABLE IF NOT EXISTS qa_messages (\n\
    id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
    run_id TEXT NOT NULL,\n\
    session_id TEXT,\n\
    direction TEXT NOT NULL,\n\
    content TEXT NOT NULL,\n\
    options_json TEXT,\n\
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_qa_messages_run ON qa_messages(run_id, id);\n\
UPDATE meta SET value = '40' WHERE key = 'schema_version';\n\
";

/// Migration 0041: add pipeline and permission_mode columns to tasks table.
const MIGRATION_0041_TASK_PIPELINE: &str = "\
ALTER TABLE tasks ADD COLUMN pipeline TEXT;\n\
ALTER TABLE tasks ADD COLUMN permission_mode TEXT;\n\
UPDATE meta SET value = '41' WHERE key = 'schema_version';\n\
";

/// Recreate merge_queue with conversation-level schema.
/// Older databases had a merge_queue table without conversation_id.
/// `CREATE TABLE IF NOT EXISTS` in the consolidated init SQL won't alter existing tables,
/// so we drop and recreate here.  Merge queue entries are transient (queued/running),
/// so data loss is acceptable.
const MIGRATION_0042_MERGE_QUEUE: &str = "\
DROP TABLE IF EXISTS merge_queue;\n\
CREATE TABLE IF NOT EXISTS merge_queue (\n\
    id              INTEGER PRIMARY KEY AUTOINCREMENT,\n\
    conversation_id TEXT NOT NULL,\n\
    branch_name     TEXT NOT NULL,\n\
    target_branch   TEXT NOT NULL,\n\
    status          TEXT NOT NULL CHECK(status IN ('queued','running','completed','failed','conflict')),\n\
    strategy        TEXT NOT NULL DEFAULT 'direct',\n\
    pr_url          TEXT,\n\
    error           TEXT,\n\
    created_at      TEXT NOT NULL,\n\
    updated_at      TEXT NOT NULL\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_merge_queue_status ON merge_queue(status);\n\
UPDATE meta SET value = '42' WHERE key = 'schema_version';\n\
";

/// Migration 0043: persist provider-native thread identity at run scope.
const MIGRATION_0043_RUN_PROVIDER_THREAD: &str = "\
ALTER TABLE runs ADD COLUMN provider_thread_id TEXT;\n\
UPDATE meta SET value = '43' WHERE key = 'schema_version';\n\
";

/// Migration 0044: automation tables — definitions, steps, runs, run-steps, and events.
const MIGRATION_0044_AUTOMATIONS: &str = "\
CREATE TABLE IF NOT EXISTS automations (\n\
    id TEXT PRIMARY KEY,\n\
    project_id TEXT NOT NULL REFERENCES projects(id),\n\
    name TEXT NOT NULL,\n\
    description TEXT,\n\
    enabled INTEGER NOT NULL DEFAULT 1,\n\
    trigger_type TEXT NOT NULL,\n\
    trigger_config TEXT NOT NULL,\n\
    default_provider TEXT,\n\
    default_model TEXT,\n\
    default_budget_usd REAL,\n\
    default_pipeline TEXT,\n\
    default_permission_mode TEXT,\n\
    session_mode TEXT NOT NULL DEFAULT 'new',\n\
    dedicated_conversation_id TEXT REFERENCES conversations(id),\n\
    source_path TEXT,\n\
    last_triggered_at TEXT,\n\
    created_at TEXT NOT NULL DEFAULT (datetime('now')),\n\
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_automations_project ON automations(project_id);\n\
CREATE INDEX IF NOT EXISTS idx_automations_enabled_trigger ON automations(enabled, trigger_type);\n\
CREATE TABLE IF NOT EXISTS automation_steps (\n\
    id TEXT PRIMARY KEY,\n\
    automation_id TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,\n\
    step_key TEXT NOT NULL,\n\
    ordinal INTEGER NOT NULL,\n\
    objective TEXT NOT NULL,\n\
    depends_on TEXT,\n\
    provider TEXT,\n\
    model TEXT,\n\
    budget_usd REAL,\n\
    pipeline TEXT,\n\
    permission_mode TEXT,\n\
    condition TEXT,\n\
    created_at TEXT NOT NULL DEFAULT (datetime('now')),\n\
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),\n\
    UNIQUE(automation_id, step_key)\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_automation_steps_automation ON automation_steps(automation_id);\n\
CREATE TABLE IF NOT EXISTS automation_runs (\n\
    id TEXT PRIMARY KEY,\n\
    automation_id TEXT NOT NULL REFERENCES automations(id),\n\
    state TEXT NOT NULL DEFAULT 'pending',\n\
    trigger_info TEXT,\n\
    conversation_id TEXT REFERENCES conversations(id),\n\
    started_at TEXT,\n\
    completed_at TEXT,\n\
    created_at TEXT NOT NULL DEFAULT (datetime('now')),\n\
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_automation_runs_automation ON automation_runs(automation_id);\n\
CREATE INDEX IF NOT EXISTS idx_automation_runs_state ON automation_runs(state);\n\
CREATE TABLE IF NOT EXISTS automation_run_steps (\n\
    id TEXT PRIMARY KEY,\n\
    automation_run_id TEXT NOT NULL REFERENCES automation_runs(id) ON DELETE CASCADE,\n\
    step_id TEXT NOT NULL REFERENCES automation_steps(id),\n\
    step_key TEXT NOT NULL,\n\
    state TEXT NOT NULL DEFAULT 'pending',\n\
    task_id TEXT REFERENCES tasks(id),\n\
    run_id TEXT REFERENCES runs(id),\n\
    condition_result INTEGER,\n\
    error TEXT,\n\
    started_at TEXT,\n\
    completed_at TEXT,\n\
    created_at TEXT NOT NULL DEFAULT (datetime('now')),\n\
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_run_steps_run ON automation_run_steps(automation_run_id);\n\
CREATE INDEX IF NOT EXISTS idx_run_steps_task ON automation_run_steps(task_id);\n\
CREATE TABLE IF NOT EXISTS automation_events (\n\
    id INTEGER PRIMARY KEY AUTOINCREMENT,\n\
    event_type TEXT NOT NULL,\n\
    payload TEXT NOT NULL,\n\
    source TEXT,\n\
    automation_id TEXT REFERENCES automations(id),\n\
    automation_run_id TEXT REFERENCES automation_runs(id),\n\
    created_at TEXT NOT NULL DEFAULT (datetime('now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_automation_events_run ON automation_events(automation_run_id);\n\
UPDATE meta SET value = '44' WHERE key = 'schema_version';\n\
";

/// Migration 0045: Add a `notifications_json` column to `automations` so
/// notification configuration survives a round-trip through the database.
const MIGRATION_0045_AUTOMATION_NOTIFICATIONS: &str = "\
ALTER TABLE automations ADD COLUMN notifications_json TEXT;\n\
UPDATE meta SET value = '45' WHERE key = 'schema_version';\n\
";

/// Migration 0047: persist a run/task-level override for phase gates.
const MIGRATION_0047_DISABLE_PHASE_GATES: &str = "\
ALTER TABLE runs ADD COLUMN disable_phase_gates INTEGER NOT NULL DEFAULT 0;\n\
ALTER TABLE tasks ADD COLUMN disable_phase_gates INTEGER NOT NULL DEFAULT 0;\n\
UPDATE meta SET value = '47' WHERE key = 'schema_version';\n\
";

/// Migration 0048: immutable conversation kind metadata and CLI launch settings.
const MIGRATION_0048_CONVERSATION_KIND: &str = "\
ALTER TABLE conversations ADD COLUMN conversation_kind TEXT NOT NULL DEFAULT 'run' CHECK(conversation_kind IN ('run','cli'));\n\
ALTER TABLE conversations ADD COLUMN cli_provider TEXT;\n\
ALTER TABLE conversations ADD COLUMN cli_model TEXT;\n\
UPDATE meta SET value = '48' WHERE key = 'schema_version';\n\
";

/// Migration 0049: chatter_threads table for chat conversation kind.
const MIGRATION_0049_CHAT_KIND: &str = "\
CREATE TABLE IF NOT EXISTS chatter_threads (\n\
    id TEXT PRIMARY KEY,\n\
    conversation_id TEXT NOT NULL REFERENCES conversations(id),\n\
    coding_agent TEXT NOT NULL,\n\
    ordinal INTEGER NOT NULL,\n\
    state TEXT NOT NULL DEFAULT 'active',\n\
    started_at TEXT NOT NULL,\n\
    ended_at TEXT\n\
);\n\
CREATE UNIQUE INDEX IF NOT EXISTS idx_chatter_thread_conv_ord ON chatter_threads(conversation_id, ordinal);\n\
CREATE INDEX IF NOT EXISTS idx_chatter_threads_conv ON chatter_threads(conversation_id);\n\
UPDATE meta SET value = '49' WHERE key = 'schema_version';\n\
";

/// Migration 0050: persist provider session ID on chatter_threads for resume-after-restart.
const MIGRATION_0050_CHATTER_SESSION_ID: &str = "\
ALTER TABLE chatter_threads ADD COLUMN provider_session_id TEXT;\n\
UPDATE meta SET value = '50' WHERE key = 'schema_version';\n\
";

/// Migration 0051: persist conversation-level chat settings as JSON.
const MIGRATION_0051_CHAT_SETTINGS_JSON: &str = "\
ALTER TABLE conversations ADD COLUMN chat_settings_json TEXT;\n\
UPDATE meta SET value = '51' WHERE key = 'schema_version';\n\
";

/// Migration 0055: Pipeline stages table for single-CLI pipeline execution.
///
/// Stores pre-built instructions for each pipeline stage so a single worker
/// agent can retrieve them via MCP tools and execute all stages in sequence.
const MIGRATION_0055_PIPELINE_STAGES: &str = "\
CREATE TABLE IF NOT EXISTS pipeline_stages (\n\
    id              TEXT PRIMARY KEY,\n\
    run_id          TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,\n\
    stage_name      TEXT NOT NULL,\n\
    ordinal         INTEGER NOT NULL,\n\
    instructions    TEXT NOT NULL,\n\
    status          TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','inprogress','completed','gate_pending','skipped','failed')),\n\
    gate_required   INTEGER NOT NULL DEFAULT 0,\n\
    gate_decision   TEXT CHECK(gate_decision IN ('pending','approved','approved_with_note','rejected','retry','auto_approved')),\n\
    gate_context    TEXT,\n\
    summary         TEXT,\n\
    artifacts_json  TEXT NOT NULL DEFAULT '[]',\n\
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    completed_at    TEXT\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_pipeline_stages_run ON pipeline_stages(run_id, ordinal);\n\
UPDATE meta SET value = '55' WHERE key = 'schema_version';\n\
";

/// Migration 0052: Grove Graph tables for the DAG-based agentic loop orchestrator.
///
/// Creates four tables:
/// - `grove_graphs`: main graph record with status, runtime controls, git metadata
/// - `graph_phases`: phase records with validation lifecycle, ordinals, dependencies
/// - `graph_steps`: step records with pipeline tracking, judge feedback, iteration control
/// - `graph_config`: key-value configuration per graph
const MIGRATION_0052_GROVE_GRAPH_TABLES: &str = "\
CREATE TABLE IF NOT EXISTS grove_graphs (\n\
    id                    TEXT PRIMARY KEY,\n\
    conversation_id       TEXT NOT NULL REFERENCES conversations(id),\n\
    title                 TEXT NOT NULL,\n\
    description           TEXT,\n\
    status                TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','inprogress','closed','failed')),\n\
    runtime_status        TEXT NOT NULL DEFAULT 'idle' CHECK(runtime_status IN ('idle','running','paused','aborted')),\n\
    parsing_status        TEXT NOT NULL DEFAULT 'pending' CHECK(parsing_status IN ('pending','planning','parsing','complete','error')),\n\
    execution_mode        TEXT NOT NULL DEFAULT 'sequential' CHECK(execution_mode IN ('sequential','parallel')),\n\
    active                INTEGER NOT NULL DEFAULT 1,\n\
    rerun_count           INTEGER NOT NULL DEFAULT 0,\n\
    max_reruns            INTEGER NOT NULL DEFAULT 3,\n\
    phases_created_count  INTEGER NOT NULL DEFAULT 0,\n\
    steps_created_count   INTEGER NOT NULL DEFAULT 0,\n\
    current_phase         TEXT,\n\
    next_step             TEXT,\n\
    progress_summary      TEXT,\n\
    source_document_path  TEXT,\n\
    git_branch            TEXT,\n\
    git_commit_sha        TEXT,\n\
    git_pr_url            TEXT,\n\
    git_merge_status      TEXT CHECK(git_merge_status IN ('pending','merged','failed')),\n\
    created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_grove_graphs_conversation ON grove_graphs(conversation_id);\n\
CREATE INDEX IF NOT EXISTS idx_grove_graphs_active ON grove_graphs(conversation_id, active);\n\
\n\
CREATE TABLE IF NOT EXISTS graph_phases (\n\
    id                TEXT PRIMARY KEY,\n\
    graph_id          TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,\n\
    task_name         TEXT NOT NULL,\n\
    task_objective    TEXT NOT NULL,\n\
    outcome           TEXT,\n\
    ai_comments       TEXT,\n\
    grade             INTEGER,\n\
    reference_doc_path TEXT,\n\
    ref_required      INTEGER NOT NULL DEFAULT 0,\n\
    status            TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','inprogress','closed','failed')),\n\
    validation_status TEXT NOT NULL DEFAULT 'pending' CHECK(validation_status IN ('pending','validating','passed','failed','fixing')),\n\
    ordinal           INTEGER NOT NULL,\n\
    depends_on_json   TEXT NOT NULL DEFAULT '[]',\n\
    git_commit_sha    TEXT,\n\
    conversation_id   TEXT REFERENCES conversations(id),\n\
    created_run_id    TEXT,\n\
    executed_run_id   TEXT,\n\
    validator_run_id  TEXT,\n\
    judge_run_id      TEXT,\n\
    execution_agent   TEXT,\n\
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    UNIQUE(graph_id, task_name)\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_graph_phases_graph ON graph_phases(graph_id);\n\
\n\
CREATE TABLE IF NOT EXISTS graph_steps (\n\
    id                  TEXT PRIMARY KEY,\n\
    phase_id            TEXT NOT NULL REFERENCES graph_phases(id) ON DELETE CASCADE,\n\
    graph_id            TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,\n\
    task_name           TEXT NOT NULL,\n\
    task_objective      TEXT NOT NULL,\n\
    step_type           TEXT NOT NULL DEFAULT 'code' CHECK(step_type IN ('code','config','docs','infra','test')),\n\
    outcome             TEXT,\n\
    ai_comments         TEXT,\n\
    grade               INTEGER,\n\
    reference_doc_path  TEXT,\n\
    ref_required        INTEGER NOT NULL DEFAULT 0,\n\
    status              TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','inprogress','closed','failed')),\n\
    ordinal             INTEGER NOT NULL,\n\
    execution_mode      TEXT NOT NULL DEFAULT 'auto' CHECK(execution_mode IN ('auto','manual')),\n\
    depends_on_json     TEXT NOT NULL DEFAULT '[]',\n\
    run_iteration       INTEGER NOT NULL DEFAULT 0,\n\
    max_iterations      INTEGER NOT NULL DEFAULT 3,\n\
    judge_feedback_json TEXT NOT NULL DEFAULT '[]',\n\
    builder_run_id      TEXT,\n\
    verdict_run_id      TEXT,\n\
    judge_run_id        TEXT,\n\
    conversation_id     TEXT REFERENCES conversations(id),\n\
    created_run_id      TEXT,\n\
    executed_run_id     TEXT,\n\
    execution_agent     TEXT,\n\
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    UNIQUE(phase_id, task_name)\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_graph_steps_phase ON graph_steps(phase_id);\n\
CREATE INDEX IF NOT EXISTS idx_graph_steps_graph ON graph_steps(graph_id);\n\
CREATE INDEX IF NOT EXISTS idx_graph_steps_status ON graph_steps(graph_id, status);\n\
\n\
CREATE TABLE IF NOT EXISTS graph_config (\n\
    id           TEXT PRIMARY KEY,\n\
    graph_id     TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,\n\
    config_key   TEXT NOT NULL,\n\
    config_value TEXT NOT NULL DEFAULT 'false',\n\
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    UNIQUE(graph_id, config_key)\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_graph_config_graph ON graph_config(graph_id);\n\
\n\
CREATE TABLE IF NOT EXISTS graph_clarifications (\n\
    id           TEXT PRIMARY KEY,\n\
    graph_id     TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,\n\
    question     TEXT NOT NULL,\n\
    answer       TEXT,\n\
    answered     INTEGER NOT NULL DEFAULT 0,\n\
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_graph_clarifications_graph ON graph_clarifications(graph_id);\n\
UPDATE meta SET value = '53' WHERE key = 'schema_version';\n\
";

// Migration 0046 is applied via `fix_runs_waiting_for_gate_constraint` — it
// rewrites the runs table CHECK constraint in sqlite_master to include the
// `waiting_for_gate` run state without rebuilding the table.

/// Migration 0034 (repair): add `provider` and `model` tracking to runs and tasks.
///
/// Originally migration 0032, but it was applied after 0033 — causing it to be
/// skipped on existing databases (schema_version 33 >= 32).  Re-numbered to 34
/// so it runs unconditionally after 0033.  `skip_existing_add_columns` makes the
/// ALTER TABLEs idempotent for databases that already have the columns.
const MIGRATION_0034_REPAIR_PROVIDER: &str = "\
ALTER TABLE runs ADD COLUMN provider TEXT;\n\
ALTER TABLE runs ADD COLUMN model     TEXT;\n\
ALTER TABLE tasks ADD COLUMN provider TEXT;\n\
UPDATE meta SET value = '34' WHERE key = 'schema_version';\n\
";

/// Migration 0053: Add the `graph_clarifications` table for interactive clarification
/// questions during the readiness check phase.
const MIGRATION_0053_GRAPH_CLARIFICATIONS: &str = "\
CREATE TABLE IF NOT EXISTS graph_clarifications (\n\
    id           TEXT PRIMARY KEY,\n\
    graph_id     TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,\n\
    question     TEXT NOT NULL,\n\
    answer       TEXT,\n\
    answered     INTEGER NOT NULL DEFAULT 0,\n\
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
CREATE INDEX IF NOT EXISTS idx_graph_clarifications_graph ON graph_clarifications(graph_id);\n\
UPDATE meta SET value = '53' WHERE key = 'schema_version';\n\
";

/// Migration 0054: Hive Creation Redesign — add `objective` and `pipeline_error` columns
/// to `grove_graphs`, and expand CHECK constraints on `runtime_status` and `parsing_status`.
///
/// SQLite does not support ALTER COLUMN or DROP CONSTRAINT, so the table must be
/// fully rebuilt. The migration:
///   1. Creates `grove_graphs_new` with the updated schema.
///   2. Copies all existing rows (NULL for new columns).
///   3. Drops the original table.
///   4. Renames the new table.
///   5. Recreates the two indexes.
///
/// New values enabled:
///   - `runtime_status = 'queued'`: queue system was already emitting this value.
///   - `parsing_status = 'generating'`: document generation agent is running.
///   - `parsing_status = 'draft_ready'`: document ready for user review.
const MIGRATION_0054_HIVE_CREATION_REDESIGN: &str = "\
CREATE TABLE grove_graphs_new (\n\
    id                    TEXT PRIMARY KEY,\n\
    conversation_id       TEXT NOT NULL REFERENCES conversations(id),\n\
    title                 TEXT NOT NULL,\n\
    description           TEXT,\n\
    objective             TEXT,\n\
    status                TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','inprogress','closed','failed')),\n\
    runtime_status        TEXT NOT NULL DEFAULT 'idle' CHECK(runtime_status IN ('idle','queued','running','paused','aborted')),\n\
    parsing_status        TEXT NOT NULL DEFAULT 'pending' CHECK(parsing_status IN ('pending','planning','parsing','generating','draft_ready','complete','error')),\n\
    execution_mode        TEXT NOT NULL DEFAULT 'sequential' CHECK(execution_mode IN ('sequential','parallel')),\n\
    active                INTEGER NOT NULL DEFAULT 1,\n\
    rerun_count           INTEGER NOT NULL DEFAULT 0,\n\
    max_reruns            INTEGER NOT NULL DEFAULT 3,\n\
    phases_created_count  INTEGER NOT NULL DEFAULT 0,\n\
    steps_created_count   INTEGER NOT NULL DEFAULT 0,\n\
    current_phase         TEXT,\n\
    next_step             TEXT,\n\
    progress_summary      TEXT,\n\
    source_document_path  TEXT,\n\
    git_branch            TEXT,\n\
    git_commit_sha        TEXT,\n\
    git_pr_url            TEXT,\n\
    git_merge_status      TEXT CHECK(git_merge_status IN ('pending','merged','failed')),\n\
    pipeline_error        TEXT,\n\
    created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),\n\
    updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))\n\
);\n\
INSERT INTO grove_graphs_new (\n\
    id, conversation_id, title, description, objective,\n\
    status, runtime_status, parsing_status, execution_mode,\n\
    active, rerun_count, max_reruns,\n\
    phases_created_count, steps_created_count,\n\
    current_phase, next_step, progress_summary,\n\
    source_document_path,\n\
    git_branch, git_commit_sha, git_pr_url, git_merge_status,\n\
    pipeline_error,\n\
    created_at, updated_at\n\
)\n\
SELECT\n\
    id, conversation_id, title, description, NULL,\n\
    status, runtime_status, parsing_status, execution_mode,\n\
    active, rerun_count, max_reruns,\n\
    phases_created_count, steps_created_count,\n\
    current_phase, next_step, progress_summary,\n\
    source_document_path,\n\
    git_branch, git_commit_sha, git_pr_url, git_merge_status,\n\
    NULL,\n\
    created_at, updated_at\n\
FROM grove_graphs;\n\
DROP TABLE grove_graphs;\n\
ALTER TABLE grove_graphs_new RENAME TO grove_graphs;\n\
CREATE INDEX IF NOT EXISTS idx_grove_graphs_conversation ON grove_graphs(conversation_id);\n\
CREATE INDEX IF NOT EXISTS idx_grove_graphs_active ON grove_graphs(conversation_id, active);\n\
UPDATE meta SET value = '54' WHERE key = 'schema_version';\n\
";

/// Add `provider` column to `grove_graphs` so graph executions use the
/// explicitly selected coding agent instead of silently falling back to the
/// workspace config default.
const MIGRATION_0056_GRAPH_PROVIDER: &str = "\
ALTER TABLE grove_graphs ADD COLUMN provider TEXT;\n\
UPDATE meta SET value = '56' WHERE key = 'schema_version';\n\
";

/// Token filter statistics table for tracking per-command compression savings.
const MIGRATION_0057_TOKEN_FILTER: &str =
    include_str!("../../../../migrations/0004_token_filter.sql");

/// Standardize token_filter_stats.created_at to RFC3339 text format.
const MIGRATION_0058_FIX_TIMESTAMP: &str =
    include_str!("../../../../migrations/0005_fix_token_filter_timestamp.sql");

/// Upgrade migration for provider-native issue identity and normalized issue metadata.
const MIGRATION_0033_UPGRADE: &str = "\
ALTER TABLE issues ADD COLUMN provider_native_id TEXT;\n\
ALTER TABLE issues ADD COLUMN provider_scope_type TEXT;\n\
ALTER TABLE issues ADD COLUMN provider_scope_key TEXT;\n\
ALTER TABLE issues ADD COLUMN provider_scope_name TEXT;\n\
ALTER TABLE issues ADD COLUMN provider_metadata_json TEXT NOT NULL DEFAULT '{}';\n\
UPDATE meta SET value = '33' WHERE key = 'schema_version';\n\
";

#[derive(Debug, Clone)]
pub struct DbHandle {
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct InitDbResult {
    pub db_path: PathBuf,
    pub schema_version: i64,
}

impl DbHandle {
    pub fn new(project_root: &Path) -> Self {
        Self {
            path: db_path(project_root),
        }
    }

    /// Create a `DbHandle` from an explicit database file path.
    ///
    /// Use this when the DB lives in a different location than the project root
    /// (e.g. centralized `~/.grove/workspaces/<id>/.grove/grove.db`).
    pub fn from_db_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn connect(&self) -> GroveResult<Connection> {
        connection::open(&self.path)
    }
}

pub fn db_path(project_root: &Path) -> PathBuf {
    crate::config::paths::db_path(project_root)
}

pub fn initialize(project_root: &Path) -> GroveResult<InitDbResult> {
    // Always ensure project's .grove/ directory exists — worktrees and config live here.
    let project_grove_dir = grove_dir(project_root);
    fs::create_dir_all(&project_grove_dir)?;

    // Ensure the centralized DB directory exists.
    let central_grove_dir = crate::config::paths::project_db_dir(project_root).join(".grove");
    fs::create_dir_all(&central_grove_dir)?;

    // One-time migration: move an existing local grove.db into the centralized location.
    let local_db = project_grove_dir.join("grove.db");
    let central_db = central_grove_dir.join("grove.db");
    if local_db.exists() && !central_db.exists() {
        if fs::rename(&local_db, &central_db).is_err() {
            // rename fails across device boundaries — fall back to copy + delete.
            fs::copy(&local_db, &central_db)?;
            fs::remove_file(&local_db)?;
        }
        // WAL and SHM files are invalidated by the move — remove them.
        let _ = fs::remove_file(local_db.with_extension("db-wal"));
        let _ = fs::remove_file(local_db.with_extension("db-shm"));
        tracing::info!(
            from = %local_db.display(),
            to = %central_db.display(),
            "migrated grove.db to centralized location"
        );
    }

    let handle = DbHandle::new(project_root);
    let mut conn = handle.connect()?;

    // The consolidated schema uses CREATE TABLE IF NOT EXISTS and INSERT OR IGNORE
    // so repeated calls are safe. Wrapped in BEGIN IMMEDIATE so concurrent callers
    // wait on the busy handler instead of racing.
    {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(MIGRATION_0001)?;
        tx.commit()?;
    }

    // For databases created with older Grove versions, apply incremental
    // schema changes to bring them up to the consolidated version.
    // New databases already have the latest schema_version from the INSERT OR IGNORE above.
    apply_migration_if_needed(&mut conn, 27, MIGRATION_0027_UPGRADE)?;
    apply_migration_if_needed(&mut conn, 28, MIGRATION_0028_UPGRADE)?;
    apply_migration_if_needed(&mut conn, 29, MIGRATION_0029_UPGRADE)?;
    apply_migration_if_needed(&mut conn, 30, MIGRATION_0030_UPGRADE)?;
    fix_runs_check_constraint(&mut conn)?;
    apply_migration_if_needed(&mut conn, 33, MIGRATION_0033_UPGRADE)?;
    apply_migration_if_needed(&mut conn, 34, MIGRATION_0034_REPAIR_PROVIDER)?;
    apply_migration_if_needed(&mut conn, 35, MIGRATION_0035_RESUME_SESSION)?;
    apply_migration_if_needed(&mut conn, 36, MIGRATION_0036_PERF_INDEXES)?;
    apply_migration_if_needed(&mut conn, 37, MIGRATION_0037_PHASE_EXECUTION)?;
    apply_migration_if_needed(&mut conn, 38, MIGRATION_0038_STREAM_EVENTS)?;
    apply_migration_if_needed(&mut conn, 39, MIGRATION_0039_RUN_ARTIFACTS)?;
    apply_migration_if_needed(&mut conn, 40, MIGRATION_0040_QA_MESSAGES)?;
    apply_migration_if_needed(&mut conn, 41, MIGRATION_0041_TASK_PIPELINE)?;
    apply_migration_if_needed(&mut conn, 42, MIGRATION_0042_MERGE_QUEUE)?;
    apply_migration_if_needed(&mut conn, 43, MIGRATION_0043_RUN_PROVIDER_THREAD)?;
    apply_migration_if_needed(&mut conn, 44, MIGRATION_0044_AUTOMATIONS)?;
    apply_migration_if_needed(&mut conn, 45, MIGRATION_0045_AUTOMATION_NOTIFICATIONS)?;
    fix_runs_waiting_for_gate_constraint(&mut conn)?;
    apply_migration_if_needed(&mut conn, 47, MIGRATION_0047_DISABLE_PHASE_GATES)?;
    apply_migration_if_needed(&mut conn, 48, MIGRATION_0048_CONVERSATION_KIND)?;
    apply_migration_if_needed(&mut conn, 49, MIGRATION_0049_CHAT_KIND)?;
    apply_migration_if_needed(&mut conn, 50, MIGRATION_0050_CHATTER_SESSION_ID)?;
    apply_migration_if_needed(&mut conn, 51, MIGRATION_0051_CHAT_SETTINGS_JSON)?;
    apply_migration_if_needed(&mut conn, 52, MIGRATION_0052_GROVE_GRAPH_TABLES)?;
    apply_migration_if_needed(&mut conn, 53, MIGRATION_0053_GRAPH_CLARIFICATIONS)?;
    apply_migration_if_needed(&mut conn, 54, MIGRATION_0054_HIVE_CREATION_REDESIGN)?;
    apply_migration_if_needed(&mut conn, 55, MIGRATION_0055_PIPELINE_STAGES)?;
    apply_migration_if_needed(&mut conn, 56, MIGRATION_0056_GRAPH_PROVIDER)?;
    apply_migration_if_needed(&mut conn, 57, MIGRATION_0057_TOKEN_FILTER)?;
    apply_migration_if_needed(&mut conn, 58, MIGRATION_0058_FIX_TIMESTAMP)?;
    repair_missing_pipeline_stages(&mut conn)?;
    fix_conversation_kind_constraint(&mut conn)?;

    let schema_version = repositories::meta_repo::get_schema_version(&conn)?;

    Ok(InitDbResult {
        db_path: handle.path,
        schema_version,
    })
}

/// Apply `sql` only when the DB's schema_version is below `version`.
///
/// The version is checked twice: once before acquiring the write lock (fast
/// path) and once inside the `BEGIN IMMEDIATE` transaction (after the lock is
/// held). This eliminates the TOCTOU window where two concurrent callers could
/// both pass the pre-check and then both attempt to apply the same migration.
///
/// `ALTER TABLE <t> ADD COLUMN <c>` statements are made idempotent: the runner
/// checks `PRAGMA table_info(<t>)` and skips the ALTER if the column already
/// exists. This avoids SQLite's "duplicate column name" error when a column was
/// added outside the migration runner (e.g. a dev schema change).
fn apply_migration_if_needed(conn: &mut Connection, version: i64, sql: &str) -> GroveResult<()> {
    // Fast path: skip acquiring a write lock when the migration is already applied.
    let current: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if current >= version {
        return Ok(());
    }

    // Acquire an exclusive write lock, then re-check so two concurrent callers
    // cannot both apply the same migration.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current_locked: i64 = tx
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if current_locked < version {
        let safe_sql = skip_existing_add_columns(&tx, sql)?;
        tx.execute_batch(&safe_sql)?;
    }
    tx.commit()?;
    Ok(())
}

/// Migration 0031: Fix the runs.state CHECK constraint to include 'publishing'.
///
/// The original CREATE TABLE used a CHECK without 'publishing'. Migration 0029
/// added publish columns but couldn't alter the CHECK. SQLite doesn't support
/// ALTER CONSTRAINT, so we rewrite the schema SQL directly via writable_schema.
/// This is safe because we're only expanding the allowed set — no data changes.
fn fix_runs_check_constraint(conn: &mut Connection) -> GroveResult<()> {
    let current: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if current >= 31 {
        return Ok(());
    }

    // Check if the constraint already includes 'publishing' (fresh DBs).
    let current_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='runs'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();

    if current_sql.contains("'publishing'") {
        // Already correct — just bump the version.
        conn.execute(
            "UPDATE meta SET value = '31' WHERE key = 'schema_version'",
            [],
        )?;
        return Ok(());
    }

    // Rewrite the schema SQL to include 'publishing' in the CHECK constraint.
    let new_sql = current_sql.replace(
        "'created','planning','executing','verifying','merging','completed','failed','paused'",
        "'created','planning','executing','verifying','publishing','merging','completed','failed','paused'",
    );

    if new_sql == current_sql {
        // Unexpected schema format — skip rather than corrupt.
        tracing::warn!("migration 0031: runs CHECK constraint not in expected format, skipping");
        conn.execute(
            "UPDATE meta SET value = '31' WHERE key = 'schema_version'",
            [],
        )?;
        return Ok(());
    }

    conn.execute_batch("PRAGMA writable_schema = ON;")?;
    conn.execute(
        "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'runs'",
        [&new_sql],
    )?;
    conn.execute_batch("PRAGMA writable_schema = OFF;")?;
    // Verify integrity after schema rewrite.
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap_or_else(|_| "error".to_string());
    if integrity != "ok" {
        tracing::error!(result = %integrity, "integrity check failed after migration 0031");
    }
    conn.execute(
        "UPDATE meta SET value = '31' WHERE key = 'schema_version'",
        [],
    )?;

    tracing::info!("migration 0031: fixed runs.state CHECK constraint to include 'publishing'");
    Ok(())
}

fn fix_runs_waiting_for_gate_constraint(conn: &mut Connection) -> GroveResult<()> {
    let current: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if current >= 46 {
        return Ok(());
    }

    let current_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='runs'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();

    if current_sql.contains("'waiting_for_gate'") {
        conn.execute(
            "UPDATE meta SET value = '46' WHERE key = 'schema_version'",
            [],
        )?;
        return Ok(());
    }

    // Rebuild the runs table with the correct CHECK constraint instead of using
    // PRAGMA writable_schema, which can corrupt sqlite_master.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;

    tx.execute_batch("
        CREATE TABLE runs_new (
            id              TEXT PRIMARY KEY,
            objective        TEXT NOT NULL,
            state            TEXT NOT NULL CHECK(state IN ('created','planning','executing','waiting_for_gate','verifying','publishing','merging','completed','failed','paused')),
            budget_usd       REAL NOT NULL DEFAULT 0,
            cost_used_usd    REAL NOT NULL DEFAULT 0,
            publish_status   TEXT NOT NULL DEFAULT 'pending_retry' CHECK(publish_status IN ('pending_retry','published','failed','skipped_no_changes')),
            publish_error    TEXT,
            final_commit_sha TEXT,
            pr_url           TEXT,
            published_at     TEXT,
            conversation_id  TEXT REFERENCES conversations(id),
            provider         TEXT,
            model            TEXT,
            provider_thread_id TEXT,
            pipeline         TEXT,
            current_agent    TEXT,
            disable_phase_gates INTEGER NOT NULL DEFAULT 0,
            created_at       TEXT NOT NULL,
            updated_at       TEXT NOT NULL
        );
        INSERT INTO runs_new
            SELECT id, objective, state, budget_usd, cost_used_usd,
                   COALESCE(publish_status, 'pending_retry'), publish_error,
                   final_commit_sha, pr_url, published_at, conversation_id,
                   provider, model, provider_thread_id, pipeline, current_agent,
                   COALESCE(disable_phase_gates, 0),
                   created_at, updated_at
            FROM runs;
        DROP TABLE runs;
        ALTER TABLE runs_new RENAME TO runs;
        DROP INDEX IF EXISTS idx_active_run_per_conv;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_active_run_per_conv
            ON runs(conversation_id)
            WHERE state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging');
        UPDATE meta SET value = '46' WHERE key = 'schema_version';
    ")?;

    tx.commit()?;

    tracing::info!(
        "migration 0046: rebuilt runs table with 'waiting_for_gate' in state CHECK constraint"
    );
    Ok(())
}

/// Rewrite the conversations.conversation_kind CHECK constraint to include 'chat'
/// and 'hive_loom'.
///
/// SQLite does not support ALTER COLUMN, so we use `PRAGMA writable_schema` to
/// directly edit sqlite_master — same approach as `fix_runs_waiting_for_gate_constraint`.
/// Repair databases where schema_version >= 55 but `pipeline_stages` table is
/// missing. This can happen when the consolidated init SQL claimed a version that
/// didn't include all tables from incremental migrations.
fn repair_missing_pipeline_stages(conn: &mut Connection) -> GroveResult<()> {
    let has_table: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='pipeline_stages'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(false);

    if has_table {
        return Ok(());
    }

    tracing::info!("repairing missing pipeline_stages table");
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(MIGRATION_0055_PIPELINE_STAGES)?;
    tx.commit()?;
    Ok(())
}

fn fix_conversation_kind_constraint(conn: &mut Connection) -> GroveResult<()> {
    let current_sql: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='conversations'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_default();

    // Already fully up-to-date.
    if current_sql.contains("'hive_loom'") || !current_sql.contains("conversation_kind IN") {
        return Ok(());
    }

    // Two possible starting states:
    //   1. Original: IN ('run','cli')             → add 'chat' and 'hive_loom'
    //   2. After chat migration: IN ('run','cli','chat') → add 'hive_loom'
    let new_sql = current_sql
        .replace("IN ('run','cli')", "IN ('run','cli','chat','hive_loom')")
        .replace(
            "IN ('run','cli','chat')",
            "IN ('run','cli','chat','hive_loom')",
        );

    if new_sql == current_sql {
        tracing::warn!(
            "fix_conversation_kind_constraint: CHECK constraint not in expected format, skipping"
        );
        return Ok(());
    }

    conn.execute_batch("PRAGMA writable_schema = ON;")?;
    conn.execute(
        "UPDATE sqlite_master SET sql = ?1 WHERE type = 'table' AND name = 'conversations'",
        [&new_sql],
    )?;
    conn.execute_batch("PRAGMA writable_schema = OFF;")?;

    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap_or_else(|_| "error".to_string());
    if integrity != "ok" {
        tracing::error!(result = %integrity, "integrity check failed after conversation_kind constraint fix");
    }

    tracing::info!(
        "fix_conversation_kind_constraint: updated CHECK to include 'chat' and 'hive_loom'"
    );
    Ok(())
}

/// Return a copy of `sql` with any `ALTER TABLE <t> ADD COLUMN <c> ...`
/// statements removed when the column already exists in the table.
///
/// This makes ADD COLUMN migrations idempotent regardless of whether the column
/// was previously added outside the migration runner.
fn skip_existing_add_columns(conn: &Connection, sql: &str) -> GroveResult<String> {
    let mut filtered = String::with_capacity(sql.len());

    for line in sql.lines() {
        let trimmed = line.trim();
        if let Some((table, column)) = parse_add_column(trimmed) {
            if column_exists(conn, table, column)? {
                tracing::debug!(
                    table = table,
                    column = column,
                    "skipping ADD COLUMN — column already exists"
                );
                continue;
            }
        }
        filtered.push_str(line);
        filtered.push('\n');
    }

    Ok(filtered)
}

/// Try to parse `ALTER TABLE <table> ADD COLUMN <column> ...` from a single
/// SQL statement line. Returns `Some((table, column))` on match.
fn parse_add_column(line: &str) -> Option<(&str, &str)> {
    // Case-insensitive prefix match without pulling in regex.
    let upper = line.to_ascii_uppercase();
    let upper = upper.trim_end_matches(';').trim();

    if !upper.starts_with("ALTER TABLE ") {
        return None;
    }

    let rest = &line[b"ALTER TABLE ".len()..];
    let table_end = rest.find(|c: char| c.is_ascii_whitespace())?;
    let table = &rest[..table_end];

    let after_table = rest[table_end..].trim_start();
    let after_upper = after_table.to_ascii_uppercase();
    if !after_upper.starts_with("ADD COLUMN ") {
        return None;
    }

    let col_rest = &after_table[b"ADD COLUMN ".len()..];
    let col_end = col_rest
        .find(|c: char| c.is_ascii_whitespace() || c == ';')
        .unwrap_or(col_rest.len());
    let column = &col_rest[..col_end];

    if table.is_empty() || column.is_empty() {
        return None;
    }

    Some((table, column))
}

/// Check whether `column` already exists in `table` using `PRAGMA table_info`.
fn column_exists(conn: &Connection, table: &str, column: &str) -> GroveResult<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|name| matches!(name, Ok(ref n) if n == column));
    Ok(exists)
}

pub fn integrity_check(project_root: &Path) -> GroveResult<String> {
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    let report = integrity::check(&conn)?;
    Ok(report.integrity_detail)
}

/// Create a point-in-time backup of the Grove database using SQLite's
/// `VACUUM INTO` command, which produces a consistent, compacted copy
/// without interrupting readers or writers.
///
/// The backup is written to `<grove_dir>/grove.db.bak`.  Call this before
/// any destructive operation (`grove init --force`, `grove gc`) so users
/// can recover if something goes wrong.
///
/// Returns the path of the backup file on success.
pub fn backup(project_root: &Path) -> GroveResult<PathBuf> {
    let src = db_path(project_root);
    if !src.exists() {
        return Err(crate::errors::GroveError::Config(
            "no grove.db found — nothing to back up".to_string(),
        ));
    }
    let backup_path = src.with_extension("db.bak");
    let handle = DbHandle::new(project_root);
    let conn = handle.connect()?;
    // Remove stale backup first so VACUUM INTO doesn't fail on existing file.
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }
    conn.execute(
        "VACUUM INTO ?1",
        rusqlite::params![backup_path.to_string_lossy().as_ref()],
    )?;
    tracing::info!(
        backup = %backup_path.display(),
        "database backed up before destructive operation"
    );
    Ok(backup_path)
}
