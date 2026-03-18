import { invoke } from "@tauri-apps/api/core";
import type {
  CapabilityReport,
  CheckpointRow,
  ConnectionStatus,
  ConversationRow,
  DoctorResult,
  EditorIntegrationStatus,
  EventRecord,
  FileDiffEntry,
  HookConfig,
  Issue,
  IssueBoard,
  IssueComment,
  IssueEvent,
  IssueUpdate,
  LlmSelection,
  LogEntry,
  MergeConversationResult,
  MergeQueueRow,
  MessageRow,
  ModelDef,
  MultiSyncResult,
  OwnershipLockRow,
  PlanStep,
  PublishResult,
  ProjectRow,
  ProviderProject,
  ProviderStatus,
  RunRecord,
  RunReport,
  SessionRecord,
  Signal,
  SubtaskRecord,
  SyncResult,
  TaskRecord,
  WorkspaceRow,
  WorktreeCleanResult,
  WorktreeEntry,
} from "../types";

export async function getBootstrapData(): Promise<import("@/types").BootstrapData> {
  return invoke("get_bootstrap_data");
}

export async function getWorkspace(): Promise<WorkspaceRow | null> {
  return invoke<WorkspaceRow | null>("get_workspace");
}

export async function listConversations(
  limit = 50,
  projectId: string | null = null,
): Promise<ConversationRow[]> {
  return invoke<ConversationRow[]>("list_conversations", { limit, projectId });
}

export async function getConversation(
  id: string,
): Promise<ConversationRow | null> {
  return invoke<ConversationRow | null>("get_conversation", { id });
}

export async function listRuns(limit = 50): Promise<RunRecord[]> {
  return invoke<RunRecord[]>("list_runs", { limit });
}

export async function getRun(id: string): Promise<RunRecord | null> {
  return invoke<RunRecord | null>("get_run", { id });
}

export async function listSessions(runId: string): Promise<SessionRecord[]> {
  return invoke<SessionRecord[]>("list_sessions", { runId });
}

export async function listPlanSteps(runId: string): Promise<PlanStep[]> {
  return invoke<PlanStep[]>("list_plan_steps", { runId });
}

export async function runEvents(runId: string): Promise<EventRecord[]> {
  return invoke<EventRecord[]>("run_events", { runId });
}

export async function listTasks(): Promise<TaskRecord[]> {
  return invoke<TaskRecord[]>("list_tasks");
}

export async function listTasksForConversation(
  conversationId: string,
): Promise<TaskRecord[]> {
  return invoke<TaskRecord[]>("list_tasks_for_conversation", { conversationId });
}

export interface StartRunResult {
  conversation_id: string;
}

export interface CreateConversationResult {
  conversation_id: string;
}

export async function startRun(
  objective: string,
  _budgetUsd: number | null,
  model: string | null,
  provider: string | null,
  conversationId: string | null,
  continueLast: boolean = false,
  projectId: string | null = null,
  pipeline: string | null = null,
  maxAgents: number | null = null,
  permissionMode: string | null = null,
  disablePhaseGates: boolean = false,
  interactive: boolean = false,
  resumeProviderSessionId: string | null = null,
  sessionName: string | null = null,
): Promise<StartRunResult> {
  return invoke<StartRunResult>("start_run", {
    objective,
    budgetUsd: null,
    model,
    provider,
    conversationId,
    continueLast,
    projectId,
    pipeline,
    maxAgents,
    permissionMode,
    disablePhaseGates,
    interactive,
    resumeProviderSessionId,
    sessionName,
  });
}

export async function createConversation(
  projectId: string,
  sessionName: string | null,
  conversationKind: "cli",
  cliProvider: string,
  cliModel: string | null = null,
): Promise<CreateConversationResult> {
  return invoke<CreateConversationResult>("create_conversation", {
    projectId,
    sessionName,
    conversationKind,
    cliProvider,
    cliModel,
  });
}

export async function createHiveLoomConversation(
  projectId: string,
  sessionName: string,
): Promise<ConversationRow> {
  return invoke<ConversationRow>("create_hive_loom_conversation", {
    projectId,
    sessionName,
  });
}

export interface LastSessionInfo {
  provider: string | null;
  model: string | null;
  provider_session_id: string | null;
}

export async function getLastSessionInfo(
  runId: string,
): Promise<LastSessionInfo | null> {
  return invoke<LastSessionInfo | null>("get_last_session_info", { runId });
}

export async function listRunsForConversation(
  conversationId: string,
): Promise<RunRecord[]> {
  return invoke<RunRecord[]>("list_runs_for_conversation", { conversationId });
}

export async function queueTask(
  objective: string,
  _budgetUsd: number | null,
  conversationId: string | null,
  priority: number | null = null,
  model: string | null = null,
  provider: string | null = null,
  projectId: string | null = null,
  disablePhaseGates: boolean = false,
): Promise<TaskRecord> {
  return invoke<TaskRecord>("queue_task", {
    objective,
    budgetUsd: null,
    conversationId,
    priority,
    model,
    provider,
    projectId,
    disablePhaseGates,
  });
}

export async function cancelTask(id: string): Promise<void> {
  return invoke<void>("cancel_task", { id });
}

export async function refreshQueue(): Promise<number> {
  return invoke<number>("refresh_queue");
}

export async function deleteTask(id: string): Promise<void> {
  return invoke<void>("delete_task", { id });
}

export async function clearQueue(): Promise<number> {
  return invoke<number>("clear_queue");
}

export async function retryTask(id: string): Promise<void> {
  return invoke<void>("retry_task", { id });
}

export async function abortRun(id: string): Promise<void> {
  return invoke<void>("abort_run", { id });
}

export async function retryPublishRun(id: string): Promise<PublishResult> {
  return invoke<PublishResult>("retry_publish_run", { id });
}

export async function listMessages(
  conversationId: string,
  limit = 500,
): Promise<MessageRow[]> {
  return invoke<MessageRow[]>("list_messages", { conversationId, limit });
}

export async function listRunMessages(runId: string): Promise<MessageRow[]> {
  return invoke<MessageRow[]>("list_run_messages", { runId });
}

// ── Session Logs ─────────────────────────────────────────────────────────────

export type { LogEntry } from "../types";

export async function readSessionLog(runId: string, sessionId: string): Promise<LogEntry[]> {
  return invoke<LogEntry[]>("read_session_log", { runId, sessionId });
}

// ── LLM Providers ────────────────────────────────────────────────────────────

export async function listProviders(): Promise<ProviderStatus[]> {
  return invoke<ProviderStatus[]>("list_providers");
}

export async function listModels(provider: string): Promise<ModelDef[]> {
  return invoke<ModelDef[]>("list_models", { provider });
}

export async function setApiKey(
  provider: string,
  key: string,
): Promise<void> {
  return invoke<void>("set_api_key", { provider, key });
}

export async function removeApiKey(provider: string): Promise<void> {
  return invoke<void>("remove_api_key", { provider });
}

export async function isAuthenticated(provider: string): Promise<boolean> {
  return invoke<boolean>("is_authenticated", { provider });
}

export async function getLlmSelection(): Promise<LlmSelection | null> {
  return invoke<LlmSelection | null>("get_llm_selection");
}

export async function setLlmSelection(
  provider: string,
  model: string | null,
  authMode: string,
): Promise<void> {
  return invoke<void>("set_llm_selection", { provider, model, authMode });
}

export async function detectEditors(): Promise<EditorIntegrationStatus[]> {
  return invoke<EditorIntegrationStatus[]>("detect_editors");
}

// ── Workspace / Project management ───────────────────────────────────────────

export async function updateWorkspaceName(name: string): Promise<void> {
  return invoke<void>("update_workspace_name", { name });
}

export async function updateProjectName(
  id: string,
  name: string,
): Promise<void> {
  return invoke<void>("update_project_name", { id, name });
}

export async function getProjectSettings(
  projectId: string,
): Promise<import("@/types").ProjectSettings> {
  return invoke("get_project_settings", { projectId });
}

export async function updateProjectSettings(
  projectId: string,
  settings: import("@/types").ProjectSettings,
): Promise<void> {
  return invoke<void>("update_project_settings", { projectId, settings });
}

// ── Conversation management ──────────────────────────────────────────────────

export async function updateConversationTitle(
  id: string,
  title: string,
): Promise<void> {
  return invoke<void>("update_conversation_title", { id, title });
}

export async function archiveConversation(id: string): Promise<void> {
  return invoke<void>("archive_conversation", { id });
}

export async function deleteConversation(id: string): Promise<void> {
  return invoke<void>("delete_conversation", { id });
}

export async function mergeConversation(id: string): Promise<MergeConversationResult> {
  return invoke<MergeConversationResult>("merge_conversation", { id });
}

export async function rebaseConversation(conversationId: string): Promise<string> {
  return invoke<string>("rebase_conversation_sync", { conversationId });
}

// ── Resume / Config ──────────────────────────────────────────────────────────

export async function resumeRun(id: string): Promise<unknown> {
  return invoke<unknown>("resume_run", { id });
}

export async function getConfig(): Promise<Record<string, unknown>> {
  return invoke<Record<string, unknown>>("get_config");
}

// ── Projects ─────────────────────────────────────────────────────────────────

export async function listProjects(): Promise<ProjectRow[]> {
  return invoke<ProjectRow[]>("list_projects");
}

export async function getWorkspaceRoot(): Promise<string> {
  return invoke<string>("get_workspace_root");
}

export async function createProject(
  rootPath: string,
  name: string | null,
): Promise<ProjectRow> {
  return invoke<ProjectRow>("create_project", { rootPath, name });
}

export type ProjectCreateRequest =
  | { kind: "open_folder"; root_path: string; name: string | null }
  | { kind: "clone_git_repo"; repo_url: string; target_path: string; name: string | null }
  | {
      kind: "create_repo";
      provider: string;
      repo_name: string;
      target_path: string;
      owner: string | null;
      visibility: string;
      gitignore_template: string | null;
      gitignore_entries: string[];
      name: string | null;
    }
  | {
      kind: "fork_repo_to_remote";
      provider: string;
      source_path: string;
      target_path: string;
      repo_name: string;
      owner: string | null;
      visibility: string;
      remote_name: string | null;
      name: string | null;
    }
  | {
      kind: "fork_folder_to_folder";
      source_path: string;
      target_path: string;
      preserve_git: boolean;
      name: string | null;
    }
  | {
      kind: "ssh";
      host: string;
      remote_path: string;
      user: string | null;
      port: number | null;
      name: string | null;
    };

export async function createProjectFromSource(
  request: ProjectCreateRequest,
): Promise<ProjectRow> {
  return invoke<ProjectRow>("create_project_from_source", { request });
}

export async function archiveProject(id: string): Promise<void> {
  return invoke<void>("archive_project", { id });
}

export async function deleteProject(id: string): Promise<void> {
  return invoke<void>("delete_project", { id });
}

// ── Worktrees ────────────────────────────────────────────────────────────────

export async function listWorktrees(): Promise<WorktreeEntry[]> {
  return invoke<WorktreeEntry[]>("list_worktrees");
}

export async function cleanWorktrees(): Promise<WorktreeCleanResult> {
  return invoke<WorktreeCleanResult>("clean_worktrees");
}

export async function cleanWorktreesScoped(
  projectId: string | null = null,
  conversationId: string | null = null,
): Promise<WorktreeCleanResult> {
  return invoke<WorktreeCleanResult>("clean_worktrees_scoped", {
    projectId,
    conversationId,
  });
}

export async function deleteWorktree(sessionId: string): Promise<number> {
  return invoke<number>("delete_worktree", { sessionId });
}

// ── Subtasks ──────────────────────────────────────────────────────────────────

export async function listSubtasks(runId: string): Promise<SubtaskRecord[]> {
  return invoke<SubtaskRecord[]>("list_subtasks", { runId });
}

// ── Ownership Locks ──────────────────────────────────────────────────────────

export async function listOwnershipLocks(
  runId: string | null = null,
): Promise<OwnershipLockRow[]> {
  return invoke<OwnershipLockRow[]>("list_ownership_locks", { runId });
}

// ── Merge Queue ──────────────────────────────────────────────────────────────

export async function listMergeQueue(runId: string): Promise<MergeQueueRow[]> {
  return invoke<MergeQueueRow[]>("list_merge_queue", { runId });
}

// ── Reports ──────────────────────────────────────────────────────────────────

export async function getRunReport(runId: string): Promise<RunReport> {
  return invoke<RunReport>("get_run_report", { runId });
}

export async function getRunReportMarkdown(runId: string): Promise<string> {
  return invoke<string>("get_run_report_markdown", { runId });
}

// ── File Diff ────────────────────────────────────────────────────────────────

export async function listRunFiles(runId: string): Promise<FileDiffEntry[]> {
  return invoke<FileDiffEntry[]>("list_run_files", { runId });
}

export async function getFileDiff(
  runId: string,
  filePath: string,
  area?: string,
): Promise<string> {
  return invoke<string>("get_file_diff", { runId, filePath, area: area ?? null });
}

// ── Batch panel data ─────────────────────────────────────────────────────────

export async function getRightPanelData(runId: string): Promise<RightPanelData> {
  return invoke<RightPanelData>("get_right_panel_data", { runId });
}

export async function getProjectPanelData(projectRoot: string): Promise<ProjectPanelData> {
  return invoke<ProjectPanelData>("get_project_panel_data", { projectRoot });
}

export async function getAllFileDiffs(runId: string): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_all_file_diffs", { runId });
}

// ── Signals ──────────────────────────────────────────────────────────────────

export async function listSignals(runId: string): Promise<Signal[]> {
  return invoke<Signal[]>("list_signals", { runId });
}

export async function markSignalRead(signalId: string): Promise<void> {
  return invoke<void>("mark_signal_read", { signalId });
}

// ── Checkpoints ──────────────────────────────────────────────────────────────

export async function listCheckpoints(runId: string): Promise<CheckpointRow[]> {
  return invoke<CheckpointRow[]>("list_checkpoints", { runId });
}

// ── Issue Tracker ────────────────────────────────────────────────────────────

export async function listIssues(
  projectId: string | null = null,
): Promise<Issue[]> {
  return invoke<Issue[]>("list_issues", { projectId });
}

export async function createIssue(
  title: string,
  body: string,
  projectId: string | null = null,
): Promise<Issue> {
  return invoke<Issue>("create_issue", { title, body, projectId });
}

export async function closeIssue(externalId: string): Promise<void> {
  return invoke<void>("close_issue", { externalId });
}

export async function refreshIssues(
  projectId: string | null = null,
): Promise<Issue[]> {
  return invoke<Issue[]>("refresh_issues", { projectId });
}

// ── Connections (issue tracker providers) ────────────────────────────────────

export async function checkConnections(): Promise<ConnectionStatus[]> {
  return invoke<ConnectionStatus[]>("check_connections");
}

export async function connectProvider(
  provider: string,
  credentials: Record<string, string>,
  storage: "keychain" | "file",
): Promise<ConnectionStatus> {
  return invoke<ConnectionStatus>("connect_provider", { provider, credentials, storage });
}

export async function disconnectProvider(provider: string): Promise<void> {
  return invoke<void>("disconnect_provider", { provider });
}

export async function listProviderIssues(
  provider: string,
  projectId: string | null = null,
): Promise<Issue[]> {
  return invoke<Issue[]>("list_provider_issues", { provider, projectId });
}

export async function searchIssues(
  query: string,
  provider: string | null = null,
  limit: number | null = null,
): Promise<Issue[]> {
  return invoke<Issue[]>("search_issues", { query, provider, limit });
}

export async function fetchReadyIssues(): Promise<Issue[]> {
  return invoke<Issue[]>("fetch_ready_issues");
}

export async function startRunFromIssue(
  issueId: string,
  additionalPrompt: string | null = null,
  _budgetUsd: number | null = null,
  model: string | null = null,
  projectId: string | null = null,
  provider: string | null = null,
  conversationId: string | null = null,
  disablePhaseGates: boolean = false,
): Promise<StartRunResult> {
  return invoke<StartRunResult>("start_run_from_issue", {
    issueId,
    additionalPrompt,
    budgetUsd: null,
    model,
    projectId,
    provider,
    conversationId,
    disablePhaseGates,
  });
}

// ── Hooks Config ─────────────────────────────────────────────────────────────

export async function getHooksConfig(): Promise<HookConfig> {
  return invoke<HookConfig>("get_hooks_config");
}

// ── Capability Detection ─────────────────────────────────────────────────────

export async function detectCapabilities(): Promise<CapabilityReport> {
  return invoke<CapabilityReport>("detect_capabilities");
}

// ── Doctor / Health Check ────────────────────────────────────────────────────

export async function doctorCheck(): Promise<DoctorResult> {
  return invoke<DoctorResult>("doctor_check");
}

// ── Git Operations ──────────────────────────────────────────────────────────

export interface GitStatusEntry {
  path: string;
  area: "staged" | "unstaged";
  status: string;
  additions: number;
  deletions: number;
}

export async function gitStatusDetailed(runId: string): Promise<GitStatusEntry[]> {
  return invoke<GitStatusEntry[]>("git_status_detailed", { runId });
}

export async function gitStageFiles(runId: string, paths: string[]): Promise<void> {
  return invoke<void>("git_stage_files", { runId, paths });
}

export async function gitUnstageFiles(runId: string, paths: string[]): Promise<void> {
  return invoke<void>("git_unstage_files", { runId, paths });
}

export async function gitStageAll(runId: string): Promise<void> {
  return invoke<void>("git_stage_all", { runId });
}

export async function gitRevertFiles(runId: string, paths: string[]): Promise<void> {
  return invoke<void>("git_revert_files", { runId, paths });
}

export async function gitRevertAll(runId: string): Promise<void> {
  return invoke<void>("git_revert_all", { runId });
}

export interface GitCommitResult {
  sha: string;
  message: string;
}

export async function gitCommit(
  runId: string,
  message: string,
  includeUnstaged: boolean,
): Promise<GitCommitResult> {
  return invoke<GitCommitResult>("git_commit", { runId, message, includeUnstaged });
}

export async function gitPush(runId: string): Promise<string> {
  return invoke<string>("git_push", { runId });
}

export interface PrResult {
  url: string;
  number: number;
  code: string | null;
}

export async function gitCreatePr(
  runId: string,
  title: string,
  body: string,
): Promise<PrResult> {
  return invoke<PrResult>("git_create_pr", { runId, title, body });
}

// ── Unified publish pipeline ─────────────────────────────────────────────────

export interface PublishChangesResult {
  sha: string;
  commit_message: string;
  branch: string;
  pushed: boolean;
  pr: PrResult | null;
}

export async function publishChanges(opts: {
  runId?: string;
  projectRoot?: string;
  step: "commit" | "push" | "pr";
  message: string;
  includeUnstaged: boolean;
  prTitle?: string;
  prBody?: string;
}): Promise<PublishChangesResult> {
  return invoke<PublishChangesResult>("publish_changes", {
    runId: opts.runId ?? null,
    projectRoot: opts.projectRoot ?? null,
    step: opts.step,
    message: opts.message,
    includeUnstaged: opts.includeUnstaged,
    prTitle: opts.prTitle ?? null,
    prBody: opts.prBody ?? null,
  });
}

export async function forkRunWorktree(
  runId: string,
  newBranchName: string | null = null,
): Promise<string> {
  return invoke<string>("fork_run_worktree", { runId, newBranchName });
}

export async function gitMergeRunToMain(runId: string): Promise<string> {
  return invoke<string>("git_merge_run_to_main", { runId });
}

// ── New Git Commands ─────────────────────────────────────────────────────────

import type {
  BranchStatus,
  GitLogEntry,
  PrStatus,
  GeneratedPrContent,
  ProjectPanelData,
  RightPanelData,
  SoftResetResult,
} from "../types";

export async function gitPull(runId: string): Promise<string> {
  return invoke<string>("git_pull", { runId });
}

export async function gitBranchStatus(runId: string): Promise<BranchStatus> {
  return invoke<BranchStatus>("git_branch_status", { runId });
}

export async function gitGetLog(
  runId: string,
  maxCount: number | null = null,
): Promise<GitLogEntry[]> {
  return invoke<GitLogEntry[]>("git_get_log", { runId, maxCount });
}

export async function gitGetLatestCommit(
  runId: string,
): Promise<GitLogEntry | null> {
  return invoke<GitLogEntry | null>("git_get_latest_commit", { runId });
}

export async function gitSoftReset(runId: string): Promise<SoftResetResult> {
  return invoke<SoftResetResult>("git_soft_reset", { runId });
}

export async function gitGetPrStatus(
  runId: string,
): Promise<PrStatus | null> {
  return invoke<PrStatus | null>("git_get_pr_status", { runId });
}

export async function gitMergePr(
  runId: string,
  strategy: string,
  adminOverride: boolean = false,
): Promise<string> {
  return invoke<string>("git_merge_pr", { runId, strategy, adminOverride });
}

export async function gitGeneratePrContent(
  runId: string,
  base: string | null = null,
): Promise<GeneratedPrContent> {
  return invoke<GeneratedPrContent>("git_generate_pr_content", { runId, base });
}

// ── Project-root git operations (no run ID needed) ──────────────────────────

export async function gitProjectFiles(projectRoot: string): Promise<FileDiffEntry[]> {
  return invoke<FileDiffEntry[]>("git_project_files", { projectRoot });
}

export async function gitProjectStatus(projectRoot: string): Promise<GitStatusEntry[]> {
  return invoke<GitStatusEntry[]>("git_project_status", { projectRoot });
}

export async function gitProjectCommit(
  projectRoot: string,
  message: string,
  includeUnstaged: boolean,
): Promise<{ sha: string; message: string }> {
  return invoke("git_project_commit", { projectRoot, message, includeUnstaged });
}

export async function gitProjectPush(projectRoot: string): Promise<string> {
  return invoke<string>("git_project_push", { projectRoot });
}

export async function gitProjectPull(projectRoot: string): Promise<string> {
  return invoke<string>("git_project_pull", { projectRoot });
}

export async function gitProjectBranchStatus(projectRoot: string): Promise<BranchStatus> {
  return invoke<BranchStatus>("git_project_branch_status", { projectRoot });
}

export async function gitProjectDiff(projectRoot: string, filePath: string): Promise<string> {
  return invoke<string>("git_project_diff", { projectRoot, filePath });
}

export async function gitProjectIsRepo(projectRoot: string): Promise<boolean> {
  return invoke<boolean>("git_project_is_repo", { projectRoot });
}

export async function gitProjectInit(projectRoot: string): Promise<void> {
  return invoke<void>("git_project_init", { projectRoot });
}

export async function gitProjectStageFiles(projectRoot: string, paths: string[]): Promise<void> {
  return invoke<void>("git_project_stage_files", { projectRoot, paths });
}

export async function gitProjectUnstageFiles(projectRoot: string, paths: string[]): Promise<void> {
  return invoke<void>("git_project_unstage_files", { projectRoot, paths });
}

export async function gitProjectStageAll(projectRoot: string): Promise<void> {
  return invoke<void>("git_project_stage_all", { projectRoot });
}

export async function gitProjectRevertFiles(projectRoot: string, paths: string[]): Promise<void> {
  return invoke<void>("git_project_revert_files", { projectRoot, paths });
}

export async function gitProjectRevertAll(projectRoot: string): Promise<void> {
  return invoke<void>("git_project_revert_all", { projectRoot });
}

export async function gitProjectGetPrStatus(projectRoot: string): Promise<PrStatus | null> {
  return invoke<PrStatus | null>("git_project_get_pr_status", { projectRoot });
}

export async function gitProjectCreatePr(
  projectRoot: string,
  title: string,
  body: string,
): Promise<PrResult> {
  return invoke<PrResult>("git_project_create_pr", { projectRoot, title, body });
}

export async function gitProjectSoftReset(projectRoot: string): Promise<SoftResetResult> {
  return invoke<SoftResetResult>("git_project_soft_reset", { projectRoot });
}

export async function gitProjectGeneratePrContent(
  projectRoot: string,
  base: string | null = null,
): Promise<GeneratedPrContent> {
  return invoke<GeneratedPrContent>("git_project_generate_pr_content", { projectRoot, base });
}

export async function gitProjectMergePr(
  projectRoot: string,
  strategy: string,
  adminOverride: boolean = false,
): Promise<string> {
  return invoke<string>("git_project_merge_pr", { projectRoot, strategy, adminOverride });
}

// ── Issue board API ──────────────────────────────────────────────────────────

export async function issueBoard(projectId: string): Promise<IssueBoard> {
  return invoke<IssueBoard>("issue_board", { projectId });
}

export async function issueGet(issueId: string): Promise<Issue | null> {
  return invoke<Issue | null>("issue_get", { issueId });
}

export async function issueCreateNative(
  projectId: string,
  title: string,
  body: string | null,
  labels: string[] | null,
  priority: string | null,
): Promise<Issue> {
  return invoke<Issue>("issue_create_native", { projectId, title, body, labels, priority });
}

export async function issueUpdate(
  issueId: string,
  update: IssueUpdate,
): Promise<void> {
  return invoke<void>("issue_update", {
    issueId,
    title: update.title ?? null,
    body: update.body ?? null,
    labels: update.labels ?? null,
    priority: update.priority ?? null,
    assignee: update.assignee ?? null,
  });
}

export async function issueMove(issueId: string, status: string): Promise<void> {
  return invoke<void>("issue_move", { issueId, status });
}

export async function issueAssign(
  issueId: string,
  assignee: string,
  pushToProvider: boolean = false,
): Promise<void> {
  return invoke<void>("issue_assign", { issueId, assignee, pushToProvider });
}

export async function issueCommentAdd(
  issueId: string,
  body: string,
  author: string | null,
  pushToProvider: boolean = false,
): Promise<IssueComment> {
  return invoke<IssueComment>("issue_comment_add", { issueId, body, author, pushToProvider });
}

export async function issueListComments(issueId: string): Promise<IssueComment[]> {
  return invoke<IssueComment[]>("issue_list_comments", { issueId });
}

export async function issueListActivity(issueId: string): Promise<IssueEvent[]> {
  return invoke<IssueEvent[]>("issue_list_activity", { issueId });
}

export async function issueLinkRun(issueId: string, runId: string): Promise<void> {
  return invoke<void>("issue_link_run", { issueId, runId });
}

export async function issueSyncAll(
  projectId: string,
  incremental: boolean = true,
): Promise<MultiSyncResult> {
  return invoke<MultiSyncResult>("issue_sync_all", { projectId, incremental });
}

export async function issueSyncProvider(
  projectId: string,
  provider: string,
  incremental: boolean = true,
): Promise<SyncResult> {
  return invoke<SyncResult>("issue_sync_provider", { projectId, provider, incremental });
}

export async function issueReopen(
  issueId: string,
  pushToProvider: boolean = false,
): Promise<void> {
  return invoke<void>("issue_reopen", { issueId, pushToProvider });
}

export async function issueDelete(issueId: string): Promise<void> {
  return invoke<void>("issue_delete", { issueId });
}

export async function issueCountOpen(projectId: string): Promise<number> {
  return invoke<number>("issue_count_open", { projectId });
}

// ── Provider project/board listing ───────────────────────────────────────────

export async function issueListProviderProjects(
  provider: string,
): Promise<ProviderProject[]> {
  return invoke<ProviderProject[]>("issue_list_provider_projects", { provider });
}

export async function listProviderStatuses(
  provider: string,
  projectId: string | null = null,
): Promise<import("@/types").IssueTrackerStatus[]> {
  return invoke("list_provider_statuses", { provider, projectId });
}

export async function pushIssueToProvider(
  issueId: string,
  provider: string,
  projectKey: string,
  projectId?: string | null,
): Promise<Issue> {
  return invoke<Issue>("push_issue_to_provider", {
    issueId,
    provider,
    projectKey,
    projectId: projectId ?? null,
  });
}

export async function issueCreateOnProvider(
  projectId: string,
  provider: string,
  projectKey: string,
  title: string,
  body: string | null,
  labels: string[],
  priority: string | null,
): Promise<Issue> {
  return invoke<Issue>("issue_create_on_provider", {
    projectId, provider, projectKey, title, body, labels, priority,
  });
}

// ── Agent catalog ──────────────────────────────────────────────────────────────

export async function getAgentCatalog(): Promise<import("@/types").AgentCatalogEntry[]> {
  return invoke("get_agent_catalog");
}

export interface PipelineDto {
  id: string;
  name: string;
  description: string;
  agents: string[];
  gates: string[];
  is_default: boolean;
}

export async function getPipelines(): Promise<PipelineDto[]> {
  return invoke<PipelineDto[]>('get_pipelines');
}

export async function getDefaultProvider(): Promise<string> {
  return invoke("get_default_provider");
}

export async function setDefaultProvider(providerId: string): Promise<void> {
  return invoke("set_default_provider", { providerId });
}

export async function setAgentEnabled(agentId: string, enabled: boolean): Promise<void> {
  return invoke("set_agent_enabled", { agentId, enabled });
}

export async function getAppVersion(): Promise<string> {
  return invoke("get_app_version");
}

// ── Phase checkpoints (pipeline gates) ──────────────────────────────────────

export interface PhaseCheckpointDto {
  id: number;
  run_id: string;
  agent: string;
  status: string;
  decision: string | null;
  decided_at: string | null;
  artifact_path: string | null;
  created_at: string;
}

export async function listPhaseCheckpoints(runId: string): Promise<PhaseCheckpointDto[]> {
  return invoke("list_phase_checkpoints", { runId });
}

export async function getPendingCheckpoint(runId: string): Promise<PhaseCheckpointDto | null> {
  return invoke("get_pending_checkpoint", { runId });
}

export async function submitGateDecision(
  checkpointId: number,
  decision: string,
  notes?: string,
): Promise<void> {
  return invoke("submit_gate_decision", { checkpointId, decision, notes: notes ?? null });
}

// ── Agent Studio: config CRUD ───────────────────────────────────────────────

export interface AgentConfigDto {
  id: string;
  name: string;
  description: string;
  can_write: boolean;
  can_run_commands: boolean;
  artifact: string | null;
  allowed_tools: string[] | null;
  skills: string[];
  upstream_artifacts: { label: string; filename: string }[];
  prompt: string;
}

export interface PipelineConfigDto {
  id: string;
  name: string;
  description: string;
  agents: string[];
  gates: string[];
  default: boolean;
  aliases: string[];
  content: string;
}

export interface SkillConfigDto {
  id: string;
  name: string;
  description: string;
  applies_to: string[];
  content: string;
}

export async function listAgentConfigs(): Promise<AgentConfigDto[]> {
  return invoke("list_agent_configs");
}

export async function getAgentConfig(agentId: string): Promise<AgentConfigDto> {
  return invoke("get_agent_config", { agentId });
}

export async function saveAgentConfig(config: AgentConfigDto): Promise<void> {
  return invoke("save_agent_config", { config });
}

export async function deleteAgentConfig(agentId: string): Promise<void> {
  return invoke("delete_agent_config", { agentId });
}

export async function listPipelineConfigs(): Promise<PipelineConfigDto[]> {
  return invoke("list_pipeline_configs");
}

export async function savePipelineConfig(config: PipelineConfigDto): Promise<void> {
  return invoke("save_pipeline_config", { config });
}

export async function deletePipelineConfig(pipelineId: string): Promise<void> {
  return invoke("delete_pipeline_config", { pipelineId });
}

export async function listSkillConfigs(): Promise<SkillConfigDto[]> {
  return invoke("list_skill_configs");
}

export async function saveSkillConfig(config: SkillConfigDto): Promise<void> {
  return invoke("save_skill_config", { config });
}

export async function deleteSkillConfig(skillId: string): Promise<void> {
  return invoke("delete_skill_config", { skillId });
}

export async function previewAgentPrompt(agentId: string, objective: string): Promise<string> {
  return invoke("preview_agent_prompt", { agentId, objective });
}

// ── Stream events ──────────────────────────────────────────────────────────

export interface StreamEventRow {
  id: number;
  run_id: string;
  session_id: string | null;
  kind: string;
  content_json: string;
  created_at: string;
}

export async function getStreamEvents(runId: string, afterId: number = 0, limit: number = 500): Promise<StreamEventRow[]> {
  return invoke("get_stream_events", { runId, afterId, limit });
}

// ── Run artifacts ──────────────────────────────────────────────────────────

export interface RunArtifactDto {
  id: number;
  run_id: string;
  agent: string;
  filename: string;
  content_hash: string;
  size_bytes: number;
  created_at: string;
}

export async function getRunArtifacts(runId: string): Promise<RunArtifactDto[]> {
  return invoke("get_run_artifacts", { runId });
}

export async function getArtifactContent(runId: string, filename: string): Promise<string> {
  return invoke("get_artifact_content", { runId, filename });
}

// ── QA Messages ────────────────────────────────────────────────────────────

export interface QaMessageDto {
  id: number;
  run_id: string;
  session_id: string | null;
  direction: string;
  content: string;
  options_json: string | null;
  created_at: string;
}

export async function sendAgentMessage(runId: string, content: string, sessionId?: string): Promise<void> {
  return invoke("send_agent_message", { runId, content, sessionId });
}

export async function listQaMessages(runId: string): Promise<QaMessageDto[]> {
  return invoke("list_qa_messages", { runId });
}

// ── Automations ─────────────────────────────────────────────────────

export async function listAutomations(projectId: string): Promise<import("@/types").AutomationDef[]> {
  return invoke("list_automations", { projectId });
}

export async function getAutomation(automationId: string): Promise<import("@/types").AutomationDef> {
  return invoke("get_automation", { automationId });
}

export async function createAutomation(
  projectId: string,
  name: string,
  triggerConfigJson: string,
  defaultsJson?: string,
  description?: string,
  sessionMode?: string,
  dedicatedConversationId?: string,
): Promise<import("@/types").AutomationDef> {
  return invoke("create_automation", {
    projectId,
    name,
    triggerConfigJson,
    defaultsJson: defaultsJson ?? null,
    description: description ?? null,
    sessionMode: sessionMode ?? null,
    dedicatedConversationId: dedicatedConversationId ?? null,
  });
}

export async function updateAutomation(
  automationId: string,
  name?: string,
  description?: string,
  enabled?: boolean,
  triggerConfigJson?: string,
  defaultsJson?: string,
): Promise<import("@/types").AutomationDef> {
  return invoke("update_automation", {
    automationId,
    name: name ?? null,
    description: description ?? null,
    enabled: enabled ?? null,
    triggerConfigJson: triggerConfigJson ?? null,
    defaultsJson: defaultsJson ?? null,
  });
}

export async function deleteAutomation(automationId: string): Promise<void> {
  return invoke("delete_automation", { automationId });
}

export async function toggleAutomation(automationId: string, enabled: boolean): Promise<void> {
  return invoke("toggle_automation", { automationId, enabled });
}

export async function listAutomationSteps(automationId: string): Promise<import("@/types").AutomationStep[]> {
  return invoke("list_automation_steps", { automationId });
}

export async function addAutomationStep(params: {
  automationId: string;
  stepKey: string;
  objective: string;
  ordinal: number;
  dependsOnJson?: string;
  provider?: string;
  model?: string;
  pipeline?: string;
  permissionMode?: string;
  condition?: string;
}): Promise<import("@/types").AutomationStep> {
  return invoke("add_automation_step", {
    automationId: params.automationId,
    stepKey: params.stepKey,
    objective: params.objective,
    ordinal: params.ordinal,
    dependsOnJson: params.dependsOnJson ?? null,
    provider: params.provider ?? null,
    model: params.model ?? null,
    budgetUsd: null,
    pipeline: params.pipeline ?? null,
    permissionMode: params.permissionMode ?? null,
    condition: params.condition ?? null,
  });
}

export async function updateAutomationStep(params: {
  stepId: string;
  objective?: string;
  dependsOnJson?: string;
  provider?: string;
  model?: string;
  pipeline?: string;
  permissionMode?: string;
  condition?: string;
}): Promise<import("@/types").AutomationStep> {
  return invoke("update_automation_step", {
    stepId: params.stepId,
    objective: params.objective ?? null,
    dependsOnJson: params.dependsOnJson ?? null,
    provider: params.provider ?? null,
    model: params.model ?? null,
    budgetUsd: null,
    pipeline: params.pipeline ?? null,
    permissionMode: params.permissionMode ?? null,
    condition: params.condition ?? null,
  });
}

export async function deleteAutomationStep(stepId: string): Promise<void> {
  return invoke("delete_automation_step", { stepId });
}

export async function triggerAutomationManually(automationId: string): Promise<string> {
  return invoke("trigger_automation_manually", { automationId });
}

export async function listAutomationRuns(automationId: string, limit?: number): Promise<import("@/types").AutomationRun[]> {
  return invoke("list_automation_runs", { automationId, limit: limit ?? null });
}

export async function getAutomationRun(runId: string): Promise<import("@/types").AutomationRun> {
  return invoke("get_automation_run", { runId });
}

export async function getAutomationRunSteps(runId: string): Promise<import("@/types").AutomationRunStep[]> {
  return invoke("get_automation_run_steps", { runId });
}

export async function cancelAutomationRun(runId: string): Promise<void> {
  return invoke("cancel_automation_run", { runId });
}

export async function importAutomationsFromFiles(projectId: string, projectRoot: string): Promise<import("@/types").AutomationDef[]> {
  return invoke("import_automations_from_files", { projectId, projectRoot });
}

// ── Grove Graph API ──────────────────────────────────────────────────────────

import type {
  GraphRecord,
  GraphDetail,
  GraphPhaseRecord,
  GraphStepRecord,
  GraphConfig,
  StepFeedbackResult,
  GraphGitStatus,
  GraphDocumentDto,
} from "../types";

// -- Graph CRUD --

export async function createGraph(
  conversationId: string,
  title: string,
  description?: string,
): Promise<GraphRecord> {
  return invoke<GraphRecord>("create_graph", {
    conversationId,
    title,
    description: description ?? null,
  });
}

export async function getGraph(graphId: string): Promise<GraphRecord> {
  return invoke<GraphRecord>("get_graph", { graphId });
}

export async function getGraphDetail(graphId: string): Promise<GraphDetail> {
  return invoke<GraphDetail>("get_graph_detail", { graphId });
}

export async function listGraphs(conversationId: string): Promise<GraphRecord[]> {
  return invoke<GraphRecord[]>("list_graphs", { conversationId });
}

export async function updateGraphStatus(graphId: string, status: string): Promise<void> {
  return invoke<void>("update_graph_status", { graphId, status });
}

export async function deleteGraph(graphId: string): Promise<void> {
  return invoke<void>("delete_graph", { graphId });
}

// -- Phase CRUD --

export async function createGraphPhase(
  graphId: string,
  taskName: string,
  taskObjective: string,
  ordinal: number,
  dependsOnJson?: string,
  refRequired?: boolean,
  referenceDocPath?: string,
): Promise<{ phase_id: string }> {
  return invoke<{ phase_id: string }>("create_graph_phase", {
    graphId,
    taskName,
    taskObjective,
    ordinal,
    dependsOnJson: dependsOnJson ?? null,
    refRequired: refRequired ?? null,
    referenceDocPath: referenceDocPath ?? null,
  });
}

export async function listGraphPhases(graphId: string): Promise<GraphPhaseRecord[]> {
  return invoke<GraphPhaseRecord[]>("list_graph_phases", { graphId });
}

export async function updateGraphPhaseStatus(phaseId: string, status: string): Promise<void> {
  return invoke<void>("update_graph_phase_status", { phaseId, status });
}

export async function deleteGraphPhase(phaseId: string): Promise<void> {
  return invoke<void>("delete_graph_phase", { phaseId });
}

// -- Step CRUD --

export async function createGraphStep(
  phaseId: string,
  graphId: string,
  taskName: string,
  taskObjective: string,
  ordinal: number,
  stepType?: string,
  executionMode?: string,
  dependsOnJson?: string,
  refRequired?: boolean,
  referenceDocPath?: string,
): Promise<{ step_id: string }> {
  return invoke<{ step_id: string }>("create_graph_step", {
    phaseId,
    graphId,
    taskName,
    taskObjective,
    ordinal,
    stepType: stepType ?? null,
    executionMode: executionMode ?? null,
    dependsOnJson: dependsOnJson ?? null,
    refRequired: refRequired ?? null,
    referenceDocPath: referenceDocPath ?? null,
  });
}

export async function listGraphSteps(phaseId: string): Promise<GraphStepRecord[]> {
  return invoke<GraphStepRecord[]>("list_graph_steps", { phaseId });
}

export async function updateGraphStepStatus(stepId: string, status: string): Promise<void> {
  return invoke<void>("update_graph_step_status", { stepId, status });
}

export async function deleteGraphStep(stepId: string): Promise<void> {
  return invoke<void>("delete_graph_step", { stepId });
}

// -- Batch --

export async function populateGraph(graphId: string, phasesJson: string): Promise<void> {
  return invoke<void>("populate_graph", { graphId, phasesJson });
}

// -- DAG + Pipeline Queries --

export async function getReadyGraphSteps(graphId: string): Promise<GraphStepRecord[]> {
  return invoke<GraphStepRecord[]>("get_ready_graph_steps", { graphId });
}

export async function getPhasesPendingValidation(graphId: string): Promise<GraphPhaseRecord[]> {
  return invoke<GraphPhaseRecord[]>("get_phases_pending_validation", { graphId });
}

export async function getStepWithFeedback(stepId: string): Promise<StepFeedbackResult> {
  return invoke<StepFeedbackResult>("get_step_with_feedback", { stepId });
}

// -- Config --

export async function setGraphConfig(graphId: string, configJson: string): Promise<void> {
  return invoke<void>("set_graph_config", { graphId, configJson });
}

export async function getGraphConfig(graphId: string): Promise<GraphConfig> {
  return invoke<GraphConfig>("get_graph_config", { graphId });
}

// -- Active + Runtime --

export async function setActiveGraph(graphId: string): Promise<void> {
  return invoke<void>("set_active_graph", { graphId });
}

export async function getActiveGraph(conversationId: string): Promise<GraphRecord | null> {
  return invoke<GraphRecord | null>("get_active_graph", { conversationId });
}

export async function setGraphExecutionMode(graphId: string, mode: string): Promise<void> {
  return invoke<void>("set_graph_execution_mode", { graphId, mode });
}

// -- Bug Report --

export async function reportGraphBug(
  graphId: string,
  description: string,
  stepId?: string,
  phaseId?: string,
): Promise<void> {
  return invoke<void>("report_graph_bug", {
    graphId,
    description,
    stepId: stepId ?? null,
    phaseId: phaseId ?? null,
  });
}

// -- Git Status --

export async function getGraphGitStatus(graphId: string): Promise<GraphGitStatus> {
  return invoke<GraphGitStatus>("get_graph_git_status", { graphId });
}

// -- Loop Controls (require Task 18 commands) --

export async function startGraphLoop(graphId: string): Promise<{ started: boolean }> {
  return invoke<{ started: boolean }>("start_graph_loop", { graphId });
}

export async function pauseGraph(graphId: string): Promise<void> {
  return invoke<void>("pause_graph", { graphId });
}

export async function resumeGraph(graphId: string): Promise<void> {
  return invoke<void>("resume_graph", { graphId });
}

export async function abortGraph(graphId: string): Promise<void> {
  return invoke<void>("abort_graph", { graphId });
}

export async function restartGraph(graphId: string, fullRestart?: boolean): Promise<void> {
  return invoke<void>("restart_graph", { graphId, fullRestart: fullRestart ?? false });
}

export async function rerunStep(stepId: string): Promise<void> {
  return invoke<void>("rerun_step", { stepId });
}

export async function rerunPhase(phaseId: string): Promise<void> {
  return invoke<void>("rerun_phase", { phaseId });
}

// -- Graph Readiness & Clarifications --

// Serde externally-tagged enum: unit variant → "Ready" (string),
// struct variant → { NeedsClarification: { ... } }
export type ReadinessResult =
  | "Ready"
  | {
      NeedsClarification: {
        missing_docs: string[];
        questions: Array<{
          id: string;
          graph_id: string;
          question: string;
          answer: string | null;
          answered: boolean;
        }>;
      };
    };

export async function checkGraphReadiness(graphId: string): Promise<ReadinessResult> {
  return invoke("check_graph_readiness", { graphId });
}

export async function submitClarificationAnswer(
  clarificationId: string,
  answer: string,
): Promise<void> {
  return invoke<void>("submit_clarification_answer", { clarificationId, answer });
}

export async function listGraphClarifications(graphId: string): Promise<
  Array<{
    id: string;
    graph_id: string;
    question: string;
    answer: string | null;
    answered: boolean;
  }>
> {
  return invoke("list_graph_clarifications", { graphId });
}

// -- Graph Creation (full pipeline, requires Task 13 command) --

export async function createGraphFromSpec(
  conversationId: string,
  title: string,
  configJson: string,
  specPath?: string,
  specText?: string,
  provider?: string,
  model?: string,
): Promise<GraphDetail> {
  return invoke<GraphDetail>("create_graph_from_spec", {
    conversationId,
    title,
    configJson,
    specPath: specPath ?? null,
    specText: specText ?? null,
    provider: provider ?? null,
    model: model ?? null,
  });
}

export async function createGraphSimple(
  conversationId: string,
  objective: string,
  hasDocs: boolean,
  docPaths: string | null,
  provider?: string,
  model?: string,
): Promise<GraphDetail> {
  return invoke<GraphDetail>("create_graph_simple", {
    conversationId,
    objective,
    hasDocs,
    docPaths,
    provider: provider ?? null,
    model: model ?? null,
  });
}

export async function getGraphDocument(graphId: string): Promise<GraphDocumentDto> {
  return invoke<GraphDocumentDto>("get_graph_document", { graphId });
}

export async function saveGraphDocument(
  graphId: string,
  title: string,
  content: string,
): Promise<GraphDetail> {
  return invoke<GraphDetail>("save_graph_document", { graphId, title, content });
}

export async function retryDocumentGeneration(graphId: string): Promise<GraphDetail> {
  return invoke<GraphDetail>("retry_document_generation", { graphId });
}
