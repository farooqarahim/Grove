// TypeScript types mirroring Rust structs from grove-core.
// These match the JSON serialization of the Rust serde types.

export interface WorkspaceRow {
  id: string;
  name: string | null;
  state: string;
  created_at: string;
  updated_at: string;
  credits_usd: number;
  llm_provider: string | null;
  llm_model: string | null;
  llm_auth_mode: string;
}

export interface ProjectRow {
  id: string;
  workspace_id: string;
  name: string | null;
  root_path: string;
  state: string;
  created_at: string;
  updated_at: string;
  source_kind: string;
  source_details: ProjectSourceDetails | null;
}

export interface ProjectSourceDetails {
  repo_provider: string | null;
  repo_url: string | null;
  repo_visibility: string | null;
  remote_name: string | null;
  gitignore_template: string | null;
  gitignore_entries: string[];
  source_path: string | null;
  preserve_git: boolean | null;
  ssh_host: string | null;
  ssh_user: string | null;
  ssh_port: number | null;
  ssh_remote_path: string | null;
}

export interface WorkflowStepConfig {
  on_start: string | null;
  on_success: string | null;
  on_failure: string | null;
  comment_on_failure: boolean;
  comment_on_success: boolean;
}

export interface IssueTrackerStatus {
  id: string;
  name: string;
  category: "backlog" | "todo" | "in_progress" | "done" | "cancelled";
  color: string | null;
}

export interface ProjectSettings {
  default_provider: string | null;
  default_project_key?: string | null;
  github_project_key?: string | null;
  linear_project_key?: string | null;
  jira_project_key?: string | null;
  github_workflow?: WorkflowStepConfig | null;
  linear_workflow?: WorkflowStepConfig | null;
  jira_workflow?: WorkflowStepConfig | null;
  grove_workflow?: WorkflowStepConfig | null;
  max_parallel_agents: number | null;
  default_pipeline: string | null;
  default_permission_mode: string | null;
  issue_board?: IssueBoardConfig | null;
}

export interface ConversationRow {
  id: string;
  project_id: string;
  title: string | null;
  state: string;
  conversation_kind: "run" | "cli" | "hive_loom";
  cli_provider: string | null;
  cli_model: string | null;
  branch_name: string | null;
  remote_branch_name: string | null;
  remote_registration_state: string;
  remote_registration_error: string | null;
  remote_registered_at: string | null;
  worktree_path: string | null;
  created_at: string;
  updated_at: string;
  workspace_id: string | null;
  user_id: string | null;
}

export interface RunRecord {
  id: string;
  objective: string;
  state: string;
  cost_used_usd: number;
  publish_status: string;
  publish_error: string | null;
  final_commit_sha: string | null;
  pr_url: string | null;
  created_at: string;
  updated_at: string;
  conversation_id: string | null;
  pipeline: string | null;
  current_agent: string | null;
  disable_phase_gates: boolean;
  provider: string | null;
  model: string | null;
}

export interface SessionRecord {
  id: string;
  run_id: string;
  agent_type: string;
  state: string;
  worktree_path: string;
  started_at: string | null;
  ended_at: string | null;
  created_at: string;
  updated_at: string;
  cost_usd: number | null;
  provider_session_id: string | null;
  last_heartbeat: string | null;
  stalled_since: string | null;
}

export interface PlanStep {
  id: string;
  run_id: string;
  step_index: number;
  wave: number;
  agent_type: string;
  title: string;
  description: string;
  todos: string[];
  files: string[];
  depends_on: string[];
  status: string;
  session_id: string | null;
  result_summary: string | null;
  created_at: string;
  updated_at: string;
}

export interface TaskRecord {
  id: string;
  objective: string;
  state: string;
  priority: number;
  run_id: string | null;
  queued_at: string;
  started_at: string | null;
  completed_at: string | null;
  publish_status: string | null;
  publish_error: string | null;
  final_commit_sha: string | null;
  pr_url: string | null;
  model: string | null;
  provider: string | null;
  conversation_id: string | null;
  disable_phase_gates: boolean;
}

export interface AgentModelEntry {
  id: string;
  name: string;
  description: string;
  is_default: boolean;
}

export interface AgentCatalogEntry {
  id: string;
  name: string;
  cli: string;
  model_flag: string | null;
  models: AgentModelEntry[];
  enabled: boolean;
  /** true when the agent's CLI binary is found on PATH */
  detected: boolean;
}

export interface PublishResult {
  run_id: string;
  publish_status: string;
  final_commit_sha: string | null;
  pr_url: string | null;
  published_at: string | null;
  error: string | null;
}

export interface EventRecord {
  id: number;
  run_id: string;
  session_id: string | null;
  event_type: string;
  payload: unknown;
  created_at: string;
}

export interface MessageRow {
  id: string;
  conversation_id: string;
  run_id: string | null;
  role: string;
  agent_type: string | null;
  session_id: string | null;
  content: string;
  created_at: string;
  user_id: string | null;
}

export interface LogEntry {
  role: string;
  content: string;
  tool_name: string | null;
  session_id: string | null;
  cost_usd: number | null;
  is_error: boolean;
  line_no: number | null;
  event_type: string | null;
  subtype: string | null;
  detail: string | null;
  metadata_json: string | null;
  request_id?: string | null;
  options?: string[] | null;
  blocking?: boolean | null;
  status?: string | null;
  decision?: string | null;
  timeout_at?: string | null;
  answer?: string | null;
}

// ── LLM Provider types ──────────────────────────────────────────────────────

export interface ProviderStatus {
  kind: string;
  name: string;
  authenticated: boolean;
  model_count: number;
  default_model: string;
}

export interface ModelDef {
  id: string;
  name: string;
  context_window: number;
  max_output_tokens: number;
  cost_input_per_m: number;
  cost_output_per_m: number;
  vision: boolean;
  tools: boolean;
  reasoning: boolean;
}

export interface LlmSelection {
  provider: string;
  model: string | null;
  auth_mode: string;
}

export interface EditorIntegrationStatus {
  id: string;
  name: string;
  description: string;
  command: string;
  detected: boolean;
  path: string | null;
}

// ── Subtask types ────────────────────────────────────────────────────────────

export interface SubtaskRecord {
  id: string;
  run_id: string;
  session_id: string | null;
  title: string;
  description: string;
  status: string;
  priority: number;
  depends_on: string[];
  assigned_agent: string | null;
  files_hint: string[];
  todos: string[];
  result_summary: string | null;
  created_at: string;
  updated_at: string;
}

// ── Ownership lock types ─────────────────────────────────────────────────────

export interface OwnershipLockRow {
  id: number;
  run_id: string;
  path: string;
  owner_session_id: string;
  created_at: string;
}

// ── Merge queue types ────────────────────────────────────────────────────────

export interface MergeQueueRow {
  id: number;
  conversation_id: string;
  branch_name: string;
  target_branch: string;
  status: string;
  strategy: string;
  pr_url: string | null;
  error: string | null;
  created_at: string;
  updated_at: string;
}

export interface MergeConversationResult {
  conversation_id: string;
  source_branch: string;
  target_branch: string;
  strategy: string;
  outcome: string;
  pr_url: string | null;
  conflicting_files: string[];
}

// ── Report types ─────────────────────────────────────────────────────────────

export interface SessionSummary {
  id: string;
  agent_type: string;
  state: string;
  worktree_path: string;
  started_at: string | null;
  ended_at: string | null;
}

export interface EventEntry {
  created_at: string;
  event_type: string;
  session_id: string | null;
  payload: Record<string, unknown>;
}

export interface RunReport {
  run_id: string;
  objective: string;
  state: string;
  created_at: string;
  sessions: SessionSummary[];
  events: EventEntry[];
}

// ── File diff types ──────────────────────────────────────────────────────────

export interface FileDiffEntry {
  status: string;
  path: string;
  committed: boolean; // true = committed but not pushed, false = uncommitted
  area: "staged" | "unstaged" | "untracked" | "committed";
}

export interface RightPanelData {
  files: FileDiffEntry[];
  branch: BranchStatus | null;
  latest_commit: GitLogEntry | null;
  cwd: string;
  /** Pre-fetched diffs keyed by file path. */
  diffs: Record<string, string>;
}

export interface ProjectPanelData {
  files: FileDiffEntry[];
  branch: BranchStatus | null;
  latest_commit: GitLogEntry | null;
  cwd: string;
  diffs: Record<string, string>;
}

// ── Signal types ─────────────────────────────────────────────────────────────

export interface Signal {
  id: string;
  run_id: string;
  from_agent: string;
  to_agent: string;
  signal_type: string;
  priority: string;
  payload: unknown;
  read: boolean;
  created_at: string;
}

// ── Checkpoint types ─────────────────────────────────────────────────────────

export interface CheckpointRow {
  id: string;
  run_id: string;
  stage: string;
  data_json: string;
  created_at: string;
}

// ── Issue tracker types ──────────────────────────────────────────────────────

export interface Issue {
  external_id: string;
  provider: string;
  title: string;
  status: string;
  labels: string[];
  body: string | null;
  url: string | null;
  assignee: string | null;
  raw_json: unknown;
  provider_native_id?: string;
  provider_scope_type?: string;
  provider_scope_key?: string;
  provider_scope_name?: string;
  provider_metadata?: unknown;
  // DB-enriched fields — present when read from local SQLite, absent otherwise
  id?: string;
  project_id?: string;
  canonical_status?: string;
  priority?: string;
  is_native?: boolean;
  created_at?: string;
  updated_at?: string;
  synced_at?: string;
  run_id?: string | null;
}

/// A project / repository / team on an external issue tracker.
export interface ProviderProject {
  id: string;
  name: string;
  key: string | null;
  url: string | null;
}

export interface ConnectionStatus {
  provider: string;
  connected: boolean;
  user_display: string | null;
  error: string | null;
}

// ── Issue board types ────────────────────────────────────────────────────────

export type CanonicalStatus = "open" | "in_progress" | "in_review" | "blocked" | "done" | "cancelled";

export interface IssueComment {
  id: number;
  issue_id: string;
  body: string;
  author: string | null;
  posted_to_provider: boolean;
  created_at: string;
}

export interface IssueEvent {
  id: number;
  issue_id: string;
  event_type: string;
  actor: string | null;
  old_value: string | null;
  new_value: string | null;
  payload: unknown;
  created_at: string;
}

export interface SyncState {
  provider: string;
  project_id: string;
  last_synced_at: string | null;
  issues_synced: number;
  last_error: string | null;
  sync_duration_ms: number | null;
}

export interface BoardColumn {
  id: string;
  canonical_status: CanonicalStatus;
  label: string;
  issues: Issue[];
  count: number;
}

export interface IssueBoardColumnConfig {
  id: string;
  label: string;
  canonical_status: CanonicalStatus;
  match_rules: Record<string, string[]>;
  provider_targets: Record<string, string>;
}

export interface IssueBoardConfig {
  columns: IssueBoardColumnConfig[];
}

export interface IssueBoard {
  columns: BoardColumn[];
  total: number;
  sync_states: SyncState[];
}

export interface SyncResult {
  provider: string;
  new_count: number;
  updated_count: number;
  closed_count: number;
  errors: string[];
  duration_ms: number;
  synced_at: string;
}

export interface MultiSyncResult {
  results: SyncResult[];
  total_new: number;
  total_updated: number;
  total_errors: number;
}

export interface IssueUpdate {
  title?: string;
  body?: string;
  labels?: string[];
  priority?: string;
  assignee?: string;
}

// ── Hooks config types ───────────────────────────────────────────────────────

export interface HookConfig {
  hooks: unknown;
  guards: unknown;
}

// ── Capability types ─────────────────────────────────────────────────────────

export interface CapabilityCheck {
  name: string;
  available: boolean;
  message: string;
}

export interface CapabilityReport {
  level: string;
  checks: CapabilityCheck[];
}

// ── Bootstrap types ──────────────────────────────────────────────────────────

export interface BootstrapData {
  workspace: WorkspaceRow | null;
  projects: ProjectRow[];
  conversations: ConversationRow[];
  recent_runs: RunRecord[];
  open_issue_count: number;
  default_provider: string;
  agent_catalog: AgentCatalogEntry[];
  connections: ConnectionStatus[];
}

export type NavScreen = "dashboard" | "sessions" | "settings" | "issues" | "automations";

// ── Git types ────────────────────────────────────────────────────────────────

export interface BranchStatus {
  branch: string;
  default_branch: string;
  ahead: number;
  behind: number;
  has_upstream: boolean;
  remote_branch_exists: boolean;
  comparison_mode: string;
  remote_registration_state: string;
  remote_error: string | null;
}

export interface GitLogEntry {
  hash: string;
  subject: string;
  body: string;
  author: string;
  date: string;
  is_pushed: boolean;
}

export interface PrStatus {
  number: number;
  url: string;
  state: string;
  is_draft: boolean;
  merge_state: string;
  title: string;
  additions: number;
  deletions: number;
  changed_files: number;
  conflicting_files: string[];
}

export interface GeneratedPrContent {
  title: string;
  description: string;
}

export interface SoftResetResult {
  subject: string;
  body: string;
}

// ── Worktree types ──────────────────────────────────────────────────────────

export interface WorktreeEntry {
  session_id: string;
  path: string;
  size_bytes: number;
  size_display: string;
  run_id: string | null;
  agent_type: string | null;
  state: string | null;
  created_at: string | null;
  ended_at: string | null;
  is_active: boolean;
  conversation_id: string | null;
  project_id: string | null;
}

export interface WorktreeCleanResult {
  deleted_count: number;
  freed_bytes: number;
}

// ── Doctor/Health check ─────────────────────────────────────────────────────

export interface DoctorResult {
  ok: boolean;
  git: boolean;
  sqlite: boolean;
  config: boolean;
  db: boolean;
}

// ── Automation types ────────────────────────────────────────────────

export interface AutomationDef {
  id: string;
  project_id: string;
  name: string;
  description: string | null;
  enabled: boolean;
  trigger: TriggerConfig;
  defaults: AutomationDefaults;
  session_mode: "new" | "dedicated";
  dedicated_conversation_id: string | null;
  source_path: string | null;
  last_triggered_at: string | null;
  created_at: string;
  updated_at: string;
  notifications: NotificationConfig | null;
}

export type TriggerConfig =
  | { type: "cron"; schedule: string }
  | { type: "event"; event_type: string; filter?: unknown }
  | { type: "manual" }
  | { type: "webhook"; filter?: unknown }
  | { type: "issue"; schedule: string; statuses: string[]; labels: string[] };

export interface AutomationDefaults {
  provider: string | null;
  model: string | null;
  pipeline: string | null;
  permission_mode: string | null;
}

export interface AutomationStep {
  id: string;
  automation_id: string;
  step_key: string;
  ordinal: number;
  objective: string;
  depends_on: string[];
  provider: string | null;
  model: string | null;
  pipeline: string | null;
  permission_mode: string | null;
  condition: string | null;
  created_at: string;
  updated_at: string;
}

export interface AutomationRun {
  id: string;
  automation_id: string;
  state: "pending" | "running" | "completed" | "failed" | "cancelled";
  trigger_info: unknown | null;
  conversation_id: string | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface AutomationRunStep {
  id: string;
  automation_run_id: string;
  step_id: string;
  step_key: string;
  state: "pending" | "queued" | "running" | "completed" | "failed" | "skipped";
  task_id: string | null;
  run_id: string | null;
  condition_result: boolean | null;
  error: string | null;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface NotificationConfig {
  on_success: NotificationTarget[];
  on_failure: NotificationTarget[];
}

export type NotificationTarget =
  | { type: "slack"; webhook_url: string; channel?: string }
  | { type: "system" }
  | { type: "webhook"; url: string; headers?: Record<string, string> };

// ── Review types ────────────────────────────────────────────────────────────

export interface ReviewCapabilities {
  canStage: boolean;
  canUnstage: boolean;
  canRevert: boolean;
  canCommit: boolean;
  canAiReview: boolean;
}

export interface ReviewContext {
  mode: 'run' | 'project';
  runId: string | null;
  projectRoot: string | null;
  files: FileDiffEntry[];
  diffs: Record<string, string>;
  branch: BranchStatus | null;
  capabilities: ReviewCapabilities;
}

export type ReviewFindingSeverity = 'critical' | 'major' | 'minor' | 'info';

export interface ReviewFinding {
  severity: ReviewFindingSeverity;
  title: string;
  body: string;
  file: string | null;
  line: number | null;
}

export interface ReviewInsights {
  sourceArtifacts: string[];
  reviewerVerdict: string | null;
  judgeVerdict: string | null;
  findings: ReviewFinding[];
  rawMarkdown: string[];
}

// ── Grove Graph types ────────────────────────────────────────────────────────

export type GraphStatus = "open" | "inprogress" | "closed" | "failed";
export type RuntimeStatus = "idle" | "queued" | "running" | "paused" | "aborted";
export type ParsingStatus = "pending" | "generating" | "draft_ready" | "planning" | "parsing" | "complete" | "error";
export type ValidationStatus = "pending" | "validating" | "passed" | "failed" | "fixing";
export type StepMode = "auto" | "manual";
export type StepType = "code" | "config" | "docs" | "infra" | "test";
export type GraphExecutionMode = "sequential" | "parallel";
export type GitMergeStatus = "pending" | "merged" | "failed";
export type StepPipelineStage = "pending" | "building" | "verdict" | "judging" | "done" | "failed";

export interface GraphConfig {
  doc_prd: boolean;
  doc_system_design: boolean;
  doc_guidelines: boolean;
  platform_frontend: boolean;
  platform_backend: boolean;
  platform_desktop: boolean;
  platform_mobile: boolean;
  arch_tech_stack: boolean;
  arch_saas: boolean;
  arch_multiuser: boolean;
  arch_dlib: boolean;
  git_push: boolean;
  git_create_pr: boolean;
}

export interface GraphRecord {
  id: string;
  conversation_id: string;
  title: string;
  description: string | null;
  objective: string | null;
  status: GraphStatus;
  runtime_status: RuntimeStatus;
  parsing_status: ParsingStatus;
  execution_mode: GraphExecutionMode;
  active: boolean;
  rerun_count: number;
  max_reruns: number;
  phases_created_count: number;
  steps_created_count: number;
  steps_closed_count: number;
  current_phase: string | null;
  next_step: string | null;
  progress_summary: string | null;
  source_document_path: string | null;
  git_branch: string | null;
  git_commit_sha: string | null;
  git_pr_url: string | null;
  git_merge_status: GitMergeStatus | null;
  pipeline_error: string | null;
  created_at: string;
  updated_at: string;
}

export interface GraphDocumentDto {
  title: string;
  content: string;
  path: string;
}

export interface GraphPhaseRecord {
  id: string;
  graph_id: string;
  task_name: string;
  task_objective: string;
  outcome: string | null;
  ai_comments: string | null;
  grade: number | null;
  reference_doc_path: string | null;
  ref_required: boolean;
  status: GraphStatus;
  validation_status: ValidationStatus;
  ordinal: number;
  depends_on_json: string;
  git_commit_sha: string | null;
  conversation_id: string | null;
  created_run_id: string | null;
  executed_run_id: string | null;
  validator_run_id: string | null;
  judge_run_id: string | null;
  execution_agent: string | null;
  created_at: string;
  updated_at: string;
}

export interface GraphStepRecord {
  id: string;
  phase_id: string;
  graph_id: string;
  task_name: string;
  task_objective: string;
  step_type: StepType;
  outcome: string | null;
  ai_comments: string | null;
  grade: number | null;
  reference_doc_path: string | null;
  ref_required: boolean;
  status: GraphStatus;
  ordinal: number;
  execution_mode: StepMode;
  depends_on_json: string;
  run_iteration: number;
  max_iterations: number;
  judge_feedback_json: string;
  builder_run_id: string | null;
  verdict_run_id: string | null;
  judge_run_id: string | null;
  conversation_id: string | null;
  created_run_id: string | null;
  executed_run_id: string | null;
  execution_agent: string | null;
  created_at: string;
  updated_at: string;
}

export interface GraphConfigRecord {
  id: string;
  graph_id: string;
  config_key: string;
  config_value: string;
  created_at: string;
  updated_at: string;
}

export interface PhaseDetail {
  phase: GraphPhaseRecord;
  steps: GraphStepRecord[];
}

export interface GraphDetail {
  graph: GraphRecord;
  config: GraphConfig;
  phases: PhaseDetail[];
}

export interface StepMetadata {
  run: { iteration: number; maxIterations: number; created: string };
  status: GraphStatus;
  dag: { dependsOn: string[]; blockedBy: string[] };
  aiComments: string | null;
  objectives: string;
  outcome: string | null;
  grade: { score: number | null; max: 10 };
  agent: string | null;
}

export interface StepFeedbackResult {
  step: GraphStepRecord;
  feedback: string[];
}

export interface GraphGitStatus {
  branch: string | null;
  commit_sha: string | null;
  pr_url: string | null;
  merge_status: GitMergeStatus | null;
}
