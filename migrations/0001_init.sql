-- Grove consolidated schema (v30)
-- This is the single authoritative schema definition.
-- All previous incremental migrations (0001-0026) are folded into this file.

PRAGMA foreign_keys = ON;

-- ── Meta ──────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

INSERT OR IGNORE INTO meta(key, value) VALUES ('schema_version', '57');
INSERT OR IGNORE INTO meta(key, value) VALUES ('created_at', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

-- ── Workspaces & Users ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS workspaces (
    id          TEXT PRIMARY KEY CHECK(length(id) <= 64),
    name        TEXT,
    state       TEXT NOT NULL DEFAULT 'active',
    credits_usd REAL NOT NULL DEFAULT 0.0,
    llm_provider TEXT,
    llm_model    TEXT,
    llm_auth_mode TEXT DEFAULT 'user_key',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS users (
    id         TEXT PRIMARY KEY CHECK(length(id) <= 64),
    name       TEXT,
    state      TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- ── Projects ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS projects (
    id           TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id),
    name         TEXT NOT NULL,
    root_path    TEXT NOT NULL,
    state        TEXT NOT NULL DEFAULT 'active',
    base_ref     TEXT,
    settings     TEXT,
    source_kind  TEXT NOT NULL DEFAULT 'local',
    source_details_json TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_projects_workspace ON projects(workspace_id);

-- ── Conversations ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS conversations (
    id            TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL,
    title         TEXT,
    state         TEXT NOT NULL DEFAULT 'active',
    conversation_kind TEXT NOT NULL DEFAULT 'run' CHECK(conversation_kind IN ('run','cli','chat','hive_loom')),
    cli_provider  TEXT,
    cli_model     TEXT,
    chat_settings_json TEXT,
    branch_name   TEXT,
    remote_branch_name TEXT,
    remote_registration_state TEXT NOT NULL DEFAULT 'local_only',
    remote_registration_error TEXT,
    remote_registered_at TEXT,
    worktree_path TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    workspace_id  TEXT REFERENCES workspaces(id),
    user_id       TEXT REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_conversations_project ON conversations(project_id);

-- ── Runs ──────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS runs (
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

CREATE INDEX IF NOT EXISTS idx_runs_state ON runs(state);
CREATE UNIQUE INDEX IF NOT EXISTS idx_active_run_per_conv
    ON runs(conversation_id)
    WHERE state IN ('executing','waiting_for_gate','planning','verifying','publishing','merging');

-- ── Sessions ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS sessions (
    id                    TEXT PRIMARY KEY,
    run_id                TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    agent_type            TEXT NOT NULL,
    state                 TEXT NOT NULL CHECK(state IN ('queued','running','waiting','completed','failed','killed')),
    worktree_path         TEXT NOT NULL,
    started_at            TEXT,
    ended_at              TEXT,
    cost_usd              REAL,
    provider_session_id   TEXT,
    checkpoint_sha        TEXT,
    parent_checkpoint_sha TEXT,
    last_heartbeat        TEXT,
    stalled_since         TEXT,
    branch                TEXT,
    pid                   INTEGER,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_run_state ON sessions(run_id, state);
CREATE INDEX IF NOT EXISTS idx_sessions_agent_type ON sessions(agent_type);

-- ── Messages ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    run_id          TEXT REFERENCES runs(id) ON DELETE SET NULL,
    role            TEXT NOT NULL,
    agent_type      TEXT,
    session_id      TEXT,
    content         TEXT NOT NULL,
    user_id         TEXT,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
CREATE INDEX IF NOT EXISTS idx_messages_run ON messages(run_id);

-- ── Events (append-only) ──────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    session_id   TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    type         TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_events_run_created ON events(run_id, created_at);

CREATE TRIGGER IF NOT EXISTS trg_events_no_update
    BEFORE UPDATE ON events
BEGIN
    SELECT RAISE(ABORT, 'events table is append-only: updates not allowed');
END;

CREATE TRIGGER IF NOT EXISTS trg_events_no_delete
    BEFORE DELETE ON events
    WHEN OLD.type != '__gc_sweep__'
BEGIN
    SELECT RAISE(ABORT, 'events table is append-only: deletes not allowed');
END;

-- ── Checkpoints ───────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS checkpoints (
    id        TEXT PRIMARY KEY,
    run_id    TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    stage     TEXT NOT NULL,
    data_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_checkpoints_run_created ON checkpoints(run_id, created_at);

-- ── Merge Queue (conversation-level) ──────────────────────────────────────────

CREATE TABLE IF NOT EXISTS merge_queue (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    branch_name     TEXT NOT NULL,
    target_branch   TEXT NOT NULL,
    status          TEXT NOT NULL CHECK(status IN ('queued','running','completed','failed','conflict')),
    strategy        TEXT NOT NULL DEFAULT 'direct',
    pr_url          TEXT,
    error           TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_merge_queue_status ON merge_queue(status);

-- ── Tasks ─────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS tasks (
    id              TEXT PRIMARY KEY,
    objective       TEXT NOT NULL,
    state           TEXT NOT NULL CHECK(state IN ('queued','running','completed','failed','cancelled')),
    budget_usd      REAL,
    priority        INTEGER NOT NULL DEFAULT 0,
    run_id          TEXT,
    queued_at       TEXT NOT NULL,
    started_at      TEXT,
    completed_at    TEXT,
    publish_status  TEXT CHECK(publish_status IN ('pending_retry','published','failed','skipped_no_changes')),
    publish_error   TEXT,
    final_commit_sha TEXT,
    pr_url          TEXT,
    model           TEXT,
    provider        TEXT,
    conversation_id TEXT REFERENCES conversations(id),
    resume_provider_session_id TEXT,
    pipeline        TEXT,
    permission_mode TEXT,
    disable_phase_gates INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_tasks_state_priority ON tasks(state, priority DESC, queued_at ASC);

-- ── Subtasks ──────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS subtasks (
    id             TEXT PRIMARY KEY,
    run_id         TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    session_id     TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    title          TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    status         TEXT NOT NULL CHECK(status IN ('pending','in_progress','completed','failed','skipped')) DEFAULT 'pending',
    priority       INTEGER NOT NULL DEFAULT 0,
    depends_on_json TEXT NOT NULL DEFAULT '[]',
    assigned_agent  TEXT,
    files_hint_json TEXT NOT NULL DEFAULT '[]',
    todos_json      TEXT NOT NULL DEFAULT '[]',
    result_summary  TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_subtasks_run_status ON subtasks(run_id, status);

-- ── Plan Steps ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS plan_steps (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    step_index      INTEGER NOT NULL,
    wave            INTEGER NOT NULL DEFAULT 0,
    agent_type      TEXT NOT NULL,
    title           TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    todos_json      TEXT NOT NULL DEFAULT '[]',
    files_json      TEXT NOT NULL DEFAULT '[]',
    depends_on_json TEXT NOT NULL DEFAULT '[]',
    status          TEXT NOT NULL CHECK(status IN ('pending','running','completed','failed','skipped')) DEFAULT 'pending',
    session_id      TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    result_summary  TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_plan_steps_run_wave ON plan_steps(run_id, wave, step_index);
CREATE INDEX IF NOT EXISTS idx_plan_steps_run_status ON plan_steps(run_id, status);

-- ── Audit Log ─────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS audit_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    table_name  TEXT NOT NULL,
    row_id      TEXT NOT NULL,
    old_state   TEXT,
    new_state   TEXT,
    changed_at  TEXT NOT NULL
);

CREATE TRIGGER IF NOT EXISTS trg_runs_audit
    AFTER UPDATE OF state ON runs
    WHEN OLD.state != NEW.state
BEGIN
    INSERT INTO audit_log (table_name, row_id, old_state, new_state, changed_at)
    VALUES ('runs', NEW.id, OLD.state, NEW.state, strftime('%Y-%m-%dT%H:%M:%fZ','now'));
END;

CREATE TRIGGER IF NOT EXISTS trg_sessions_audit
    AFTER UPDATE OF state ON sessions
    WHEN OLD.state != NEW.state
BEGIN
    INSERT INTO audit_log (table_name, row_id, old_state, new_state, changed_at)
    VALUES ('sessions', NEW.id, OLD.state, NEW.state, strftime('%Y-%m-%dT%H:%M:%fZ','now'));
END;

-- ── Performance Samples ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS perf_samples (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id      TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    operation   TEXT NOT NULL,
    duration_ms REAL NOT NULL,
    recorded_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_perf_samples_run_op ON perf_samples(run_id, operation);

-- ── Signals (inter-agent) ─────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS signals (
    id           TEXT PRIMARY KEY,
    run_id       TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    from_agent   TEXT NOT NULL,
    to_agent     TEXT NOT NULL,
    signal_type  TEXT NOT NULL,
    priority     TEXT NOT NULL DEFAULT 'normal',
    payload_json TEXT,
    read         INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_signals_inbox ON signals(run_id, to_agent, read);
CREATE INDEX IF NOT EXISTS idx_signals_type ON signals(signal_type);

-- ── Issues ────────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS issues (
    id              TEXT PRIMARY KEY,
    project_id      TEXT,
    title           TEXT NOT NULL,
    body            TEXT,
    status          TEXT NOT NULL DEFAULT 'open',
    canonical_status TEXT,
    priority        TEXT,
    labels_json     TEXT DEFAULT '[]',
    assignee        TEXT,
    provider        TEXT NOT NULL DEFAULT 'grove',
    external_id     TEXT,
    external_url    TEXT,
    run_id          TEXT,
    is_native       INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    synced_at       TEXT,
    raw_json        TEXT,
    provider_native_id TEXT,
    provider_scope_type TEXT,
    provider_scope_key TEXT,
    provider_scope_name TEXT,
    provider_metadata_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE(provider, external_id)
);

CREATE INDEX IF NOT EXISTS idx_issues_project ON issues(project_id);
CREATE INDEX IF NOT EXISTS idx_issues_canonical ON issues(canonical_status);
CREATE INDEX IF NOT EXISTS idx_issues_provider ON issues(provider);
CREATE INDEX IF NOT EXISTS idx_issues_run ON issues(run_id);
CREATE INDEX IF NOT EXISTS idx_issues_external ON issues(external_id);

CREATE TABLE IF NOT EXISTS issue_comments (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    issue_id           TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    body               TEXT NOT NULL,
    author             TEXT,
    posted_to_provider INTEGER NOT NULL DEFAULT 0,
    created_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_issue_comments_issue ON issue_comments(issue_id);

CREATE TABLE IF NOT EXISTS issue_events (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    issue_id     TEXT NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    event_type   TEXT NOT NULL,
    actor        TEXT,
    old_value    TEXT,
    new_value    TEXT,
    payload_json TEXT,
    created_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_issue_events_issue ON issue_events(issue_id);
CREATE INDEX IF NOT EXISTS idx_issue_events_type ON issue_events(event_type);
CREATE INDEX IF NOT EXISTS idx_issue_events_created ON issue_events(created_at);

CREATE TABLE IF NOT EXISTS issue_sync_state (
    provider        TEXT NOT NULL,
    project_id      TEXT NOT NULL,
    last_synced_at  TEXT,
    issues_synced   INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    sync_duration_ms INTEGER,
    PRIMARY KEY (provider, project_id)
);

-- Legacy issues_cache kept for data preservation; no longer written to.
CREATE TABLE IF NOT EXISTS issues_cache (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    external_id TEXT UNIQUE NOT NULL,
    run_id      TEXT,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'open',
    labels_json TEXT DEFAULT '[]',
    body        TEXT,
    cached_at   TEXT NOT NULL,
    raw_json    TEXT,
    project_id  TEXT,
    provider    TEXT DEFAULT 'github',
    url         TEXT,
    assignee    TEXT
);

CREATE INDEX IF NOT EXISTS idx_issues_cache_project ON issues_cache(project_id);

-- ── Ownership Locks (per-run file locking) ────────────────────────────────────

CREATE TABLE IF NOT EXISTS ownership_locks (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id           TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    path             TEXT NOT NULL,
    owner_session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    created_at       TEXT NOT NULL,
    UNIQUE(run_id, path)
);

-- ── Performance Indexes (hot query paths) ────────────────────────────────────

CREATE INDEX IF NOT EXISTS idx_projects_root_path ON projects(root_path);
CREATE INDEX IF NOT EXISTS idx_issues_project_updated ON issues(project_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_issues_provider_ext_project ON issues(provider, external_id, project_id);
CREATE INDEX IF NOT EXISTS idx_runs_created ON runs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_runs_conversation ON runs(conversation_id, created_at DESC);

-- ── Chatter Threads (chat conversation kind) ────────────────────────────────

CREATE TABLE IF NOT EXISTS chatter_threads (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    coding_agent    TEXT NOT NULL,
    ordinal         INTEGER NOT NULL,
    state           TEXT NOT NULL DEFAULT 'active',
    provider_session_id TEXT,
    started_at      TEXT NOT NULL,
    ended_at        TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_chatter_thread_conv_ord ON chatter_threads(conversation_id, ordinal);
CREATE INDEX IF NOT EXISTS idx_chatter_threads_conv ON chatter_threads(conversation_id);

-- ── Grove Graphs (DAG-based agentic loop orchestrator) ──────────────────────

CREATE TABLE IF NOT EXISTS grove_graphs (
    id                    TEXT PRIMARY KEY,
    conversation_id       TEXT NOT NULL REFERENCES conversations(id),
    title                 TEXT NOT NULL,
    description           TEXT,
    objective             TEXT,
    status                TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','inprogress','closed','failed')),
    runtime_status        TEXT NOT NULL DEFAULT 'idle' CHECK(runtime_status IN ('idle','queued','running','paused','aborted')),
    parsing_status        TEXT NOT NULL DEFAULT 'pending' CHECK(parsing_status IN ('pending','planning','parsing','generating','draft_ready','complete','error')),
    execution_mode        TEXT NOT NULL DEFAULT 'sequential' CHECK(execution_mode IN ('sequential','parallel')),
    active                INTEGER NOT NULL DEFAULT 1,
    rerun_count           INTEGER NOT NULL DEFAULT 0,
    max_reruns            INTEGER NOT NULL DEFAULT 3,
    phases_created_count  INTEGER NOT NULL DEFAULT 0,
    steps_created_count   INTEGER NOT NULL DEFAULT 0,
    current_phase         TEXT,
    next_step             TEXT,
    progress_summary      TEXT,
    source_document_path  TEXT,
    git_branch            TEXT,
    git_commit_sha        TEXT,
    git_pr_url            TEXT,
    git_merge_status      TEXT CHECK(git_merge_status IN ('pending','merged','failed')),
    pipeline_error        TEXT,
    provider              TEXT,
    created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_grove_graphs_conversation ON grove_graphs(conversation_id);
CREATE INDEX IF NOT EXISTS idx_grove_graphs_active ON grove_graphs(conversation_id, active);

CREATE TABLE IF NOT EXISTS graph_phases (
    id                TEXT PRIMARY KEY,
    graph_id          TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,
    task_name         TEXT NOT NULL,
    task_objective    TEXT NOT NULL,
    outcome           TEXT,
    ai_comments       TEXT,
    grade             INTEGER,
    reference_doc_path TEXT,
    ref_required      INTEGER NOT NULL DEFAULT 0,
    status            TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','inprogress','closed','failed')),
    validation_status TEXT NOT NULL DEFAULT 'pending' CHECK(validation_status IN ('pending','validating','passed','failed','fixing')),
    ordinal           INTEGER NOT NULL,
    depends_on_json   TEXT NOT NULL DEFAULT '[]',
    git_commit_sha    TEXT,
    conversation_id   TEXT REFERENCES conversations(id),
    created_run_id    TEXT,
    executed_run_id   TEXT,
    validator_run_id  TEXT,
    judge_run_id      TEXT,
    execution_agent   TEXT,
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(graph_id, task_name)
);

CREATE INDEX IF NOT EXISTS idx_graph_phases_graph ON graph_phases(graph_id);

CREATE TABLE IF NOT EXISTS graph_steps (
    id                  TEXT PRIMARY KEY,
    phase_id            TEXT NOT NULL REFERENCES graph_phases(id) ON DELETE CASCADE,
    graph_id            TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,
    task_name           TEXT NOT NULL,
    task_objective      TEXT NOT NULL,
    step_type           TEXT NOT NULL DEFAULT 'code' CHECK(step_type IN ('code','config','docs','infra','test')),
    outcome             TEXT,
    ai_comments         TEXT,
    grade               INTEGER,
    reference_doc_path  TEXT,
    ref_required        INTEGER NOT NULL DEFAULT 0,
    status              TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','inprogress','closed','failed')),
    ordinal             INTEGER NOT NULL,
    execution_mode      TEXT NOT NULL DEFAULT 'auto' CHECK(execution_mode IN ('auto','manual')),
    depends_on_json     TEXT NOT NULL DEFAULT '[]',
    run_iteration       INTEGER NOT NULL DEFAULT 0,
    max_iterations      INTEGER NOT NULL DEFAULT 3,
    judge_feedback_json TEXT NOT NULL DEFAULT '[]',
    builder_run_id      TEXT,
    verdict_run_id      TEXT,
    judge_run_id        TEXT,
    conversation_id     TEXT REFERENCES conversations(id),
    created_run_id      TEXT,
    executed_run_id     TEXT,
    execution_agent     TEXT,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(phase_id, task_name)
);

CREATE INDEX IF NOT EXISTS idx_graph_steps_phase ON graph_steps(phase_id);
CREATE INDEX IF NOT EXISTS idx_graph_steps_graph ON graph_steps(graph_id);
CREATE INDEX IF NOT EXISTS idx_graph_steps_status ON graph_steps(graph_id, status);

CREATE TABLE IF NOT EXISTS graph_config (
    id           TEXT PRIMARY KEY,
    graph_id     TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,
    config_key   TEXT NOT NULL,
    config_value TEXT NOT NULL DEFAULT 'false',
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(graph_id, config_key)
);

CREATE INDEX IF NOT EXISTS idx_graph_config_graph ON graph_config(graph_id);

CREATE TABLE IF NOT EXISTS graph_clarifications (
    id           TEXT PRIMARY KEY,
    graph_id     TEXT NOT NULL REFERENCES grove_graphs(id) ON DELETE CASCADE,
    question     TEXT NOT NULL,
    answer       TEXT,
    answered     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_graph_clarifications_graph ON graph_clarifications(graph_id);

-- ── Phase Checkpoints (gate decisions per agent) ────────────────────────────

CREATE TABLE IF NOT EXISTS phase_checkpoints (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id        TEXT NOT NULL,
    agent         TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',
    decision      TEXT,
    decided_at    TEXT,
    artifact_path TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_phase_checkpoints_run ON phase_checkpoints(run_id);

-- ── Stream Events (real-time agent output) ──────────────────────────────────

CREATE TABLE IF NOT EXISTS stream_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id        TEXT NOT NULL,
    session_id    TEXT,
    kind          TEXT NOT NULL,
    content_json  TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_stream_events_run ON stream_events(run_id, id);

-- ── Run Artifacts (files produced by agents) ────────────────────────────────

CREATE TABLE IF NOT EXISTS run_artifacts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id        TEXT NOT NULL,
    agent         TEXT NOT NULL,
    filename      TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    size_bytes    INTEGER NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_run_artifacts_run ON run_artifacts(run_id, agent);

-- ── QA Messages (bidirectional agent ↔ user Q&A) ────────────────────────────

CREATE TABLE IF NOT EXISTS qa_messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id       TEXT NOT NULL,
    session_id   TEXT,
    direction    TEXT NOT NULL,
    content      TEXT NOT NULL,
    options_json TEXT,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_qa_messages_run ON qa_messages(run_id, id);

-- ── Automations ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS automations (
    id                         TEXT PRIMARY KEY,
    project_id                 TEXT NOT NULL REFERENCES projects(id),
    name                       TEXT NOT NULL,
    description                TEXT,
    enabled                    INTEGER NOT NULL DEFAULT 1,
    trigger_type               TEXT NOT NULL,
    trigger_config             TEXT NOT NULL,
    default_provider           TEXT,
    default_model              TEXT,
    default_budget_usd         REAL,
    default_pipeline           TEXT,
    default_permission_mode    TEXT,
    session_mode               TEXT NOT NULL DEFAULT 'new',
    dedicated_conversation_id  TEXT REFERENCES conversations(id),
    source_path                TEXT,
    last_triggered_at          TEXT,
    notifications_json         TEXT,
    created_at                 TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at                 TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automations_project ON automations(project_id);
CREATE INDEX IF NOT EXISTS idx_automations_enabled_trigger ON automations(enabled, trigger_type);

CREATE TABLE IF NOT EXISTS automation_steps (
    id                TEXT PRIMARY KEY,
    automation_id     TEXT NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
    step_key          TEXT NOT NULL,
    ordinal           INTEGER NOT NULL,
    objective         TEXT NOT NULL,
    depends_on        TEXT,
    provider          TEXT,
    model             TEXT,
    budget_usd        REAL,
    pipeline          TEXT,
    permission_mode   TEXT,
    condition         TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(automation_id, step_key)
);

CREATE INDEX IF NOT EXISTS idx_automation_steps_automation ON automation_steps(automation_id);

CREATE TABLE IF NOT EXISTS automation_runs (
    id               TEXT PRIMARY KEY,
    automation_id    TEXT NOT NULL REFERENCES automations(id),
    state            TEXT NOT NULL DEFAULT 'pending',
    trigger_info     TEXT,
    conversation_id  TEXT REFERENCES conversations(id),
    started_at       TEXT,
    completed_at     TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automation_runs_automation ON automation_runs(automation_id);
CREATE INDEX IF NOT EXISTS idx_automation_runs_state ON automation_runs(state);

CREATE TABLE IF NOT EXISTS automation_run_steps (
    id                  TEXT PRIMARY KEY,
    automation_run_id   TEXT NOT NULL REFERENCES automation_runs(id) ON DELETE CASCADE,
    step_id             TEXT NOT NULL REFERENCES automation_steps(id),
    step_key            TEXT NOT NULL,
    state               TEXT NOT NULL DEFAULT 'pending',
    task_id             TEXT REFERENCES tasks(id),
    run_id              TEXT REFERENCES runs(id),
    condition_result    INTEGER,
    error               TEXT,
    started_at          TEXT,
    completed_at        TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_run_steps_run ON automation_run_steps(automation_run_id);
CREATE INDEX IF NOT EXISTS idx_run_steps_task ON automation_run_steps(task_id);

CREATE TABLE IF NOT EXISTS automation_events (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    event_type         TEXT NOT NULL,
    payload            TEXT NOT NULL,
    source             TEXT,
    automation_id      TEXT REFERENCES automations(id),
    automation_run_id  TEXT REFERENCES automation_runs(id),
    created_at         TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_automation_events_run ON automation_events(automation_run_id);

-- ── Pipeline Stages ──────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS pipeline_stages (
    id              TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    stage_name      TEXT NOT NULL,
    ordinal         INTEGER NOT NULL,
    instructions    TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','inprogress','completed','gate_pending','skipped','failed')),
    gate_required   INTEGER NOT NULL DEFAULT 0,
    gate_decision   TEXT CHECK(gate_decision IN ('pending','approved','approved_with_note','rejected','retry','auto_approved')),
    gate_context    TEXT,
    summary         TEXT,
    artifacts_json  TEXT NOT NULL DEFAULT '[]',
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    completed_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_pipeline_stages_run ON pipeline_stages(run_id, ordinal);

-- ── Token Filter Stats ───────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS token_filter_stats (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id           TEXT    NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    session_id       TEXT,
    command          TEXT    NOT NULL,
    filter_type      TEXT    NOT NULL,
    raw_bytes        INTEGER NOT NULL,
    filtered_bytes   INTEGER NOT NULL,
    compression_level INTEGER NOT NULL DEFAULT 1,
    created_at       DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_token_filter_stats_run ON token_filter_stats(run_id);
