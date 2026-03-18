use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

/// CLI representation of `PermissionMode` — converted to the core type in `run.rs`.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum PermissionModeArg {
    /// Auto-approve all tools via `--dangerously-skip-permissions` (default).
    SkipAll,
    /// Pause and ask the human operator via TTY for each tool request.
    HumanGate,
    /// Spawn a gatekeeper Claude instance to decide each tool request.
    AutonomousGate,
}

#[derive(Debug, Parser)]
#[command(name = "grove")]
#[command(about = "Local single-user orchestration engine")]
pub struct Cli {
    #[arg(long, global = true, default_value = ".")]
    pub project: PathBuf,

    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,

    #[arg(long, global = true)]
    pub verbose: bool,

    #[arg(long, global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init(InitArgs),
    Doctor(DoctorArgs),
    Run(RunArgs),
    /// Add an objective to the task queue; starts immediately if nothing is running.
    Queue(QueueArgs),
    /// List the task queue (queued, running, completed).
    Tasks(TasksArgs),
    /// Cancel a queued task by ID.
    TaskCancel(TaskCancelArgs),
    Status(StatusArgs),
    Resume(ResumeArgs),
    Abort(AbortArgs),
    Logs(LogsArgs),
    Report(ReportArgs),
    /// List, clean, or delete agent worktrees.
    Worktrees(WorktreesArgs),
    /// Show cost breakdown by agent type and recent runs.
    Costs(CostsArgs),
    /// Show sub-task breakdown and to-do list for a run.
    Subtasks(SubtasksArgs),
    /// Show the structured plan (waves, todos, statuses) for a run.
    Plan(PlanArgs),
    /// List all sessions for a run.
    Sessions(SessionsArgs),
    /// List currently held file ownership locks.
    Ownership(OwnershipArgs),
    /// Show merge-queue status for a run.
    MergeStatus(MergeStatusArgs),
    /// List, show, or resolve merge conflicts from the most recent run.
    Conflicts(ConflictsArgs),
    /// Manage the workspace identity for this machine.
    Workspace(WorkspaceArgs),
    /// Manage projects registered in this workspace.
    Project(ProjectArgs),
    /// Manage conversation threads.
    Conversation(ConversationArgs),
    /// Manage API keys for direct LLM providers (Anthropic, OpenAI, DeepSeek, Inception).
    Auth(AuthArgs),
    /// Browse LLM providers and their available models.
    Llm(LlmArgs),
    /// Send, check, or list inter-agent signals for a run.
    Signal(SignalArgs),
    /// Run lifecycle hooks (called by Claude Code's hooks mechanism).
    Hook(HookArgs),
    /// Manage external issue tracker integration.
    Issue(IssueArgs),
    /// Retry or inspect run publication.
    Publish(PublishArgs),
    /// Fetch an issue from a connected tracker and run agents to fix it.
    Fix(FixArgs),
    /// Connect or disconnect external issue tracker providers (GitHub, Jira, Linear).
    Connect(ConnectArgs),
    /// Run configured linters and show results; optionally spawn an agent to fix issues.
    Lint(LintArgs),
    /// Check CI status for a branch; optionally wait for completion or fix failures.
    Ci(CiArgs),
    /// Clean up finished worktrees, optionally scoped to a project or conversation.
    Cleanup(CleanupArgs),
    /// Full garbage collection: sweep expired pool holds, prune orphaned branches, git gc.
    Gc(GcArgs),
    /// Git operations: status, commit, push, pull, log, PR, and branch management.
    Git(GitArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub fix: bool,

    /// Run all checks AND apply every available automatic fix.
    #[arg(long = "fix-all")]
    pub fix_all: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    pub objective: String,

    #[arg(long = "budget-usd")]
    pub budget_usd: Option<f64>,

    #[arg(long = "max-agents")]
    pub max_agents: Option<u16>,

    /// Claude model to use for all agents and planning in this run.
    /// e.g. --model claude-opus-4-6  or  --model claude-haiku-4-5-20251001
    #[arg(long)]
    pub model: Option<String>,

    /// Pause interactively after every agent for review and control.
    #[arg(long)]
    pub interactive: bool,

    /// Comma-separated list of agent types to pause after.
    /// e.g. --pause-after architect,tester
    #[arg(long = "pause-after")]
    pub pause_after: Option<String>,

    /// Tool permission mode for this run. Overrides the project config.
    ///
    /// `skip_all`        — auto-approve all tools (default)
    /// `human_gate`      — pause and ask a human via TTY for each blocked tool
    /// `autonomous_gate` — ask a gatekeeper Claude instance for each blocked tool
    #[arg(long = "permission-mode", value_enum)]
    pub permission_mode: Option<PermissionModeArg>,

    /// Named pipeline to use for this run. Overrides auto-detection and AI planning.
    ///
    /// Available pipelines:
    ///   instant, quick, standard, parallel-build, secure, refactor, test-coverage,
    ///   migration, bugfix, docs, review-only, plan-only, fullstack, security-audit,
    ///   hardened, prototype, cleanup, investigate, ci-fix, autonomous
    #[arg(long)]
    pub pipeline: Option<String>,

    /// Continue an existing conversation thread by ID.
    #[arg(long)]
    pub conversation: Option<String>,

    /// Continue the most recent conversation for this project.
    #[arg(long, short = 'c')]
    pub continue_last: bool,

    /// Link this run to an external issue by ID.
    #[arg(long)]
    pub issue: Option<String>,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: i64,
}

#[derive(Debug, Args)]
pub struct ResumeArgs {
    pub run_id: String,
}

#[derive(Debug, Args)]
pub struct AbortArgs {
    pub run_id: String,
}

#[derive(Debug, Args)]
pub struct LogsArgs {
    pub run_id: String,

    /// Fetch all events without the default 200-event tail cap.
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    pub run_id: String,
}

#[derive(Debug, Args)]
pub struct QueueArgs {
    pub objective: String,

    #[arg(long = "budget-usd")]
    pub budget_usd: Option<f64>,

    /// Higher priority tasks execute before lower ones. Default: 0.
    #[arg(long, default_value_t = 0)]
    pub priority: i64,

    /// Claude model to use when this task executes.
    #[arg(long)]
    pub model: Option<String>,

    /// Continue an existing conversation thread by ID.
    #[arg(long)]
    pub conversation: Option<String>,

    /// Continue the most recent conversation for this project.
    #[arg(long, short = 'c')]
    pub continue_last: bool,
}

#[derive(Debug, Args)]
pub struct TasksArgs {
    #[arg(long, default_value_t = 50)]
    pub limit: i64,

    /// Reconcile stale 'running' tasks (from crashes, aborts) and restart the queue.
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Debug, Args)]
pub struct WorktreesArgs {
    /// Delete all finished (completed/failed) worktrees to free disk space.
    #[arg(long)]
    pub clean: bool,

    /// Delete a specific worktree by session ID.
    #[arg(long, value_name = "SESSION_ID")]
    pub delete: Option<String>,

    /// Delete all agent worktrees. Active (queued/running) sessions are automatically skipped.
    #[arg(long = "delete-all")]
    pub delete_all: bool,

    /// Skip confirmation prompt for --delete-all.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct CostsArgs {
    /// Number of recent completed runs to include in the breakdown.
    #[arg(long, default_value_t = 5)]
    pub recent_runs: i64,
}

#[derive(Debug, Args)]
pub struct SubtasksArgs {
    /// Run ID to inspect. Defaults to most recent run with sub-tasks.
    pub run_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct PlanArgs {
    /// Run ID to inspect. Defaults to most recent run with a structured plan.
    pub run_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct TaskCancelArgs {
    /// ID of the queued task to cancel.
    pub task_id: String,
}

#[derive(Debug, Args)]
pub struct SessionsArgs {
    /// Run ID whose sessions to list.
    pub run_id: String,
}

#[derive(Debug, Args)]
pub struct OwnershipArgs {
    /// Filter by run ID (optional — omit to list all locks).
    pub run_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct MergeStatusArgs {
    /// Conversation ID whose merge-queue entries to show.
    pub conversation_id: String,
}

#[derive(Debug, Args)]
pub struct ConflictsArgs {
    /// Show details for a specific conflicted file path.
    #[arg(long)]
    pub show: Option<String>,

    /// Mark a conflict as resolved and remove its artifacts.
    #[arg(long)]
    pub resolve: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub action: WorkspaceAction,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceAction {
    /// Show current workspace (id, name, state).
    Show,
    /// Set a friendly name for the workspace.
    SetName(WorkspaceSetNameArgs),
    /// Soft-delete (archive) a workspace by ID.
    Archive(WorkspaceIdArgs),
    /// Hard-delete a workspace by ID.
    Delete(WorkspaceIdArgs),
}

#[derive(Debug, Args)]
pub struct WorkspaceSetNameArgs {
    /// The new name for the workspace.
    pub name: String,
}

#[derive(Debug, Args)]
pub struct WorkspaceIdArgs {
    /// Workspace ID.
    pub id: String,
}

#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub action: ProjectAction,
}

#[derive(Debug, Subcommand)]
pub enum ProjectAction {
    /// Show the current project.
    Show,
    /// List all projects in the workspace.
    List,
    /// Register an existing local folder as a project.
    OpenFolder(ProjectOpenFolderArgs),
    /// Clone a git repository into a path and register it.
    Clone(ProjectCloneArgs),
    /// Create a new Git repository, scaffold a local checkout, and register it.
    CreateRepo(ProjectCreateRepoArgs),
    /// Fork a local git repo into a new remote repo and local folder.
    ForkRepo(ProjectForkRepoArgs),
    /// Copy a local folder into a new folder and register it.
    ForkFolder(ProjectForkFolderArgs),
    /// Register an SSH project for remote shell work.
    Ssh(ProjectSshArgs),
    /// Open an interactive SSH shell for an SSH project.
    SshShell(ProjectIdArgs),
    /// Set a friendly name for the current project.
    SetName(ProjectSetNameArgs),
    /// Configure default settings for the current project (tracker, pipeline, budget, etc.).
    Set(ProjectSetArgs),
    /// Archive a project (defaults to current).
    Archive(ProjectIdArgs),
    /// Delete a project (defaults to current).
    Delete(ProjectIdArgs),
}

#[derive(Debug, Args)]
pub struct ProjectSetNameArgs {
    /// The new name for the project.
    pub name: String,
}

#[derive(Debug, Args)]
pub struct ProjectOpenFolderArgs {
    /// Existing folder to register.
    pub path: PathBuf,

    /// Optional display name override.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectCloneArgs {
    /// Git repository URL to clone.
    pub repo: String,

    /// Target directory for the clone.
    pub path: PathBuf,

    /// Optional display name override.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectCreateRepoArgs {
    /// Repository name on the selected provider.
    pub repo: String,

    /// Local checkout directory to create.
    pub path: PathBuf,

    /// Git provider: github, gitlab, bitbucket.
    #[arg(long, default_value = "github")]
    pub provider: String,

    /// Repository visibility.
    #[arg(long, value_parser = ["private", "public"], default_value = "private")]
    pub visibility: String,

    /// Optional owner/org/group. Defaults to the authenticated user.
    #[arg(long)]
    pub owner: Option<String>,

    /// Built-in gitignore template: node, python, rust, go, java, none.
    #[arg(long)]
    pub gitignore: Option<String>,

    /// Extra gitignore entries to append.
    #[arg(long = "gitignore-entry")]
    pub gitignore_entries: Vec<String>,

    /// Optional display name override.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectForkRepoArgs {
    /// Existing local git repo to fork from.
    pub source_path: PathBuf,

    /// New local checkout directory to create.
    pub target_path: PathBuf,

    /// Repository name on the selected provider.
    pub repo: String,

    /// Git provider: github, gitlab, bitbucket.
    #[arg(long, default_value = "github")]
    pub provider: String,

    /// Repository visibility.
    #[arg(long, value_parser = ["private", "public"], default_value = "private")]
    pub visibility: String,

    /// Optional owner/org/group.
    #[arg(long)]
    pub owner: Option<String>,

    /// Remote name to attach the newly created repo to.
    #[arg(long)]
    pub remote_name: Option<String>,

    /// Optional display name override.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectForkFolderArgs {
    /// Existing local folder to copy from.
    pub source_path: PathBuf,

    /// New local directory to create.
    pub target_path: PathBuf,

    /// Preserve the source folder's .git directory if present.
    #[arg(long)]
    pub preserve_git: bool,

    /// Optional display name override.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectSshArgs {
    /// SSH host or alias.
    pub host: String,

    /// Remote working directory on the SSH host.
    pub remote_path: String,

    /// SSH username.
    #[arg(long)]
    pub user: Option<String>,

    /// SSH port.
    #[arg(long)]
    pub port: Option<u16>,

    /// Optional display name override.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectSetArgs {
    /// Default issue-tracker provider (github, jira, linear, grove).
    #[arg(long)]
    pub provider: Option<String>,

    /// Default project/team/repo key for the chosen provider.
    /// Linear: team key (e.g. ENG). Jira: project key (e.g. PROJ). GitHub: owner/repo.
    #[arg(long)]
    pub project_key: Option<String>,

    /// Maximum number of parallel agents for new runs.
    #[arg(long)]
    pub parallel: Option<i64>,

    /// Default pipeline (e.g. auto, standard, quick, bugfix).
    #[arg(long)]
    pub pipeline: Option<String>,

    /// Default run budget in USD.
    #[arg(long)]
    pub budget: Option<f64>,

    /// Default permission mode (skip_all, human_gate, autonomous_gate).
    #[arg(long)]
    pub permission_mode: Option<String>,

    /// Clear all settings, resetting to workspace defaults.
    #[arg(long, conflicts_with_all = ["provider", "project_key", "parallel", "pipeline", "budget", "permission_mode"])]
    pub reset: bool,
}

#[derive(Debug, Args)]
pub struct ProjectIdArgs {
    /// Project ID (defaults to the current project if omitted).
    pub id: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConversationArgs {
    #[command(subcommand)]
    pub action: ConversationAction,
}

#[derive(Debug, Subcommand)]
pub enum ConversationAction {
    /// List conversations for the current project.
    List(ConversationListArgs),
    /// Show conversation details and messages.
    Show(ConversationShowArgs),
    /// Archive a conversation.
    Archive(ConversationIdArgs),
    /// Delete a conversation and its messages.
    Delete(ConversationIdArgs),
    /// Rebase the conversation branch onto the latest default branch (e.g. main).
    ///
    /// Run this when a conversation has fallen behind upstream changes. If the
    /// rebase hits conflicts, the branch is left unchanged and the conflicting
    /// files are reported.
    Rebase(ConversationIdArgs),
    /// Merge the conversation branch into the project's default branch.
    ///
    /// Uses the configured merge strategy: `direct` (git merge --no-ff)
    /// or `github` (push + open PR via `gh`).
    Merge(ConversationIdArgs),
}

#[derive(Debug, Args)]
pub struct ConversationListArgs {
    /// Max conversations to show.
    #[arg(long, default_value_t = 20)]
    pub limit: i64,
}

#[derive(Debug, Args)]
pub struct ConversationShowArgs {
    /// Conversation ID.
    pub id: String,

    /// Max messages to show.
    #[arg(long, default_value_t = 50)]
    pub limit: i64,
}

#[derive(Debug, Args)]
pub struct ConversationIdArgs {
    /// Conversation ID.
    pub id: String,
}

// ── Auth command ──────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub action: AuthAction,
}

#[derive(Debug, Subcommand)]
pub enum AuthAction {
    /// Store an API key for a provider.
    ///
    /// Example: grove auth set anthropic sk-ant-...
    Set(AuthSetArgs),
    /// Remove the stored API key for a provider.
    Remove(AuthRemoveArgs),
    /// List all providers and their authentication status.
    List,
}

#[derive(Debug, Args)]
pub struct AuthSetArgs {
    /// Provider id: anthropic, openai, deepseek, inception.
    pub provider: String,
    /// API key to store (stored with 0o600 permissions).
    pub api_key: String,
}

#[derive(Debug, Args)]
pub struct AuthRemoveArgs {
    /// Provider id: anthropic, openai, deepseek, inception.
    pub provider: String,
}

// ── Llm command ───────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct LlmArgs {
    #[command(subcommand)]
    pub action: LlmAction,
}

#[derive(Debug, Subcommand)]
pub enum LlmAction {
    /// List all supported providers with auth status, model count, and workspace selection.
    List,
    /// List available models for a provider.
    ///
    /// Example: grove llm models anthropic
    Models(LlmModelsArgs),
    /// Set the workspace-level default LLM provider and model.
    ///
    /// Example: grove llm select anthropic claude-sonnet-4-6 --own-key
    Select(LlmSelectArgs),
    /// Manage workspace credits (for Grove-hosted API key pooling).
    Credits(LlmCreditsArgs),
}

#[derive(Debug, Args)]
pub struct LlmModelsArgs {
    /// Provider id: anthropic, openai, deepseek, inception.
    pub provider: String,
}

#[derive(Debug, Args)]
pub struct LlmSelectArgs {
    /// Provider id: anthropic, openai, deepseek, inception.
    pub provider: String,
    /// Model id to use (e.g. claude-sonnet-4-6). Defaults to provider's built-in default.
    pub model: Option<String>,
    /// Use your own API key (default).
    #[arg(long = "own-key", conflicts_with = "workspace_credits")]
    pub own_key: bool,
    /// Use Grove's pooled API key and deduct from workspace credits.
    #[arg(long = "workspace-credits", conflicts_with = "own_key")]
    pub workspace_credits: bool,
}

#[derive(Debug, Args)]
pub struct LlmCreditsArgs {
    #[command(subcommand)]
    pub action: LlmCreditsAction,
}

#[derive(Debug, Subcommand)]
pub enum LlmCreditsAction {
    /// Show the current workspace credit balance.
    Balance,
    /// Add credits to the workspace balance (admin / testing only).
    Add(LlmCreditsAddArgs),
}

#[derive(Debug, Args)]
pub struct LlmCreditsAddArgs {
    /// Amount in USD to add (e.g. 10.00).
    pub amount_usd: f64,
}

// ── Signal command ────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct SignalArgs {
    #[command(subcommand)]
    pub action: SignalAction,
}

#[derive(Debug, Subcommand)]
pub enum SignalAction {
    /// Send a signal from one agent to another.
    Send(SignalSendArgs),
    /// Check unread signals for an agent.
    Check(SignalCheckArgs),
    /// List all signals for a run.
    List(SignalListArgs),
}

#[derive(Debug, Args)]
pub struct SignalSendArgs {
    /// Run ID.
    pub run_id: String,
    /// Sending agent name (e.g. "architect").
    pub from: String,
    /// Receiving agent name or group (@all, @builders, @leads).
    pub to: String,
    /// Signal type (status, question, result, error, worker_done, etc.).
    pub signal_type: String,
    /// JSON payload.
    #[arg(long)]
    pub payload: Option<String>,
    /// Signal priority (low, normal, high, urgent).
    #[arg(long)]
    pub priority: Option<String>,
}

#[derive(Debug, Args)]
pub struct SignalCheckArgs {
    /// Run ID.
    pub run_id: String,
    /// Agent name to check signals for.
    pub agent_name: String,
}

#[derive(Debug, Args)]
pub struct SignalListArgs {
    /// Run ID.
    pub run_id: String,
}

// ── Hook command ──────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct HookArgs {
    /// Hook event: session_start, user_prompt_submit, pre_tool_use, post_tool_use, stop, pre_compact, post_run.
    pub event: String,
    /// Agent type (e.g. "builder").
    pub agent_type: String,
    /// Run ID.
    #[arg(long)]
    pub run_id: Option<String>,
    /// Session ID.
    #[arg(long)]
    pub session_id: Option<String>,
    /// Worktree path.
    #[arg(long)]
    pub worktree: Option<String>,
    /// Tool name (for pre_tool_use / post_tool_use events).
    #[arg(long)]
    pub tool: Option<String>,
    /// File path (for file-write guard checks).
    #[arg(long)]
    pub file_path: Option<String>,
}

// ── Issue command ─────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct IssueArgs {
    #[command(subcommand)]
    pub action: IssueAction,
}

#[derive(Debug, Args)]
pub struct PublishArgs {
    #[command(subcommand)]
    pub action: PublishAction,
}

#[derive(Debug, Subcommand)]
pub enum PublishAction {
    /// Retry the publish phase for a completed run without rerunning agents.
    Retry(PublishRetryArgs),
}

#[derive(Debug, Args)]
pub struct PublishRetryArgs {
    pub run_id: String,
}

#[derive(Debug, Subcommand)]
pub enum IssueAction {
    /// List issues from external tracker.
    List(IssueListArgs),
    /// Show details for a specific issue.
    Show(IssueShowArgs),
    /// Create a new issue.
    Create(IssueCreateArgs),
    /// Close an issue.
    Close(IssueCloseArgs),
    /// List issues marked as ready.
    Ready,
    /// Sync issues from external tracker(s) into local board.
    Sync(IssueSyncArgs),
    /// Show the issue board as a text-mode kanban.
    Board(IssueBoardArgs),
    /// Manage project-scoped issue board configuration.
    BoardConfig(IssueBoardConfigArgs),
    /// Search issues by text.
    Search(IssueSearchArgs),
    /// Update fields on an existing issue.
    Update(IssueUpdateArgs),
    /// Post a comment on an issue.
    Comment(IssueCommentArgs),
    /// Assign an issue to a user.
    Assign(IssueAssignArgs),
    /// Move (transition) an issue to a new status.
    Move(IssueMoveArgs),
    /// Re-open a closed issue.
    Reopen(IssueReopenArgs),
    /// Push a Grove-native issue to an external tracker.
    Push(IssuePushArgs),
    /// Show audit activity log for an issue.
    Activity(IssueActivityArgs),
    /// Run configured linters and sync results to the board.
    Lint(IssueLintArgs),
}

#[derive(Debug, Args)]
pub struct IssueListArgs {
    /// Show locally cached issues instead of fetching from remote.
    #[arg(long)]
    pub cached: bool,
}

#[derive(Debug, Args)]
pub struct IssueShowArgs {
    /// Issue ID (external or grove:{uuid}).
    pub id: String,
}

#[derive(Debug, Args)]
pub struct IssueCreateArgs {
    /// Issue title.
    pub title: String,
    /// Issue body.
    #[arg(long)]
    pub body: Option<String>,
    /// Comma-separated labels.
    #[arg(long)]
    pub labels: Option<String>,
    /// Priority: low, medium, high, critical.
    #[arg(long)]
    pub priority: Option<String>,
}

#[derive(Debug, Args)]
pub struct IssueCloseArgs {
    /// Issue ID (external).
    pub id: String,
}

#[derive(Debug, Args)]
pub struct IssueSyncArgs {
    /// Restrict sync to a single provider: github, jira, linear.
    #[arg(long)]
    pub provider: Option<String>,
    /// Force a full re-fetch even if an incremental sync would suffice.
    #[arg(long)]
    pub full: bool,
}

#[derive(Debug, Args)]
pub struct IssueBoardArgs {
    /// Filter by canonical status: open, in_progress, in_review, blocked, done, cancelled.
    #[arg(long)]
    pub status: Option<String>,
    /// Filter by provider.
    #[arg(long)]
    pub provider: Option<String>,
    /// Filter by assignee.
    #[arg(long)]
    pub assignee: Option<String>,
    /// Filter by priority: low, medium, high, critical.
    #[arg(long)]
    pub priority: Option<String>,
}

#[derive(Debug, Args)]
pub struct IssueBoardConfigArgs {
    #[command(subcommand)]
    pub action: IssueBoardConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum IssueBoardConfigAction {
    /// Show the effective project-scoped issue board configuration.
    Show,
    /// Set the project-scoped issue board configuration from a JSON file.
    Set(IssueBoardConfigSetArgs),
    /// Reset the project-scoped issue board configuration back to defaults.
    Reset,
}

#[derive(Debug, Args)]
pub struct IssueBoardConfigSetArgs {
    /// Path to a JSON file containing an IssueBoardConfig object.
    #[arg(long)]
    pub file: String,
}

#[derive(Debug, Args)]
pub struct IssueSearchArgs {
    /// Text to search for.
    pub query: String,
    /// Maximum number of results per provider.
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    /// Restrict to a specific provider.
    #[arg(long)]
    pub provider: Option<String>,
}

#[derive(Debug, Args)]
pub struct IssueUpdateArgs {
    /// Issue ID.
    pub id: String,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub label: Option<String>,
    #[arg(long)]
    pub assignee: Option<String>,
    #[arg(long)]
    pub priority: Option<String>,
}

#[derive(Debug, Args)]
pub struct IssueCommentArgs {
    /// Issue ID.
    pub id: String,
    /// Comment body text.
    pub body: String,
}

#[derive(Debug, Args)]
pub struct IssueAssignArgs {
    /// Issue ID.
    pub id: String,
    /// Assignee login / display name / email (provider-dependent).
    pub assignee: String,
}

#[derive(Debug, Args)]
pub struct IssueMoveArgs {
    /// Issue ID.
    pub id: String,
    /// Target status: accepts raw provider status or canonical name.
    pub status: String,
}

#[derive(Debug, Args)]
pub struct IssueReopenArgs {
    /// Issue ID.
    pub id: String,
}

#[derive(Debug, Args)]
pub struct IssuePushArgs {
    /// Grove-native issue ID (grove:{uuid}).
    pub id: String,
    /// Target provider: github, jira, linear.
    #[arg(long)]
    pub to: String,
}

#[derive(Debug, Args)]
pub struct IssueActivityArgs {
    /// Issue ID.
    pub id: String,
}

#[derive(Debug, Args)]
pub struct IssueLintArgs {
    /// Start a Grove run to automatically fix all detected linter errors.
    #[arg(long)]
    pub fix: bool,
}

// ── Connect command ──────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct ConnectArgs {
    #[command(subcommand)]
    pub action: ConnectAction,
}

#[derive(Debug, Subcommand)]
pub enum ConnectAction {
    /// Connect to GitHub Issues (via `gh` CLI). Optionally provide a token.
    Github(ConnectGithubArgs),
    /// Connect to Jira (REST API with email + API token).
    Jira(ConnectJiraArgs),
    /// Connect to Linear (GraphQL API with API token).
    Linear(ConnectLinearArgs),
    /// Show connection status for all providers.
    Status,
    /// Disconnect a provider and remove stored credentials.
    Disconnect(ConnectDisconnectArgs),
}

#[derive(Debug, Args)]
pub struct ConnectGithubArgs {
    /// GitHub personal access token. If omitted, checks existing `gh` auth.
    #[arg(long)]
    pub token: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConnectJiraArgs {
    /// Jira site URL (e.g., https://mycompany.atlassian.net).
    #[arg(long)]
    pub site: String,
    /// Jira account email.
    #[arg(long)]
    pub email: String,
    /// Jira API token.
    #[arg(long)]
    pub token: String,
}

#[derive(Debug, Args)]
pub struct ConnectLinearArgs {
    /// Linear API token.
    #[arg(long)]
    pub token: String,
}

#[derive(Debug, Args)]
pub struct ConnectDisconnectArgs {
    /// Provider to disconnect: github, jira, or linear.
    pub provider: String,
}

// ── Fix command ──────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct FixArgs {
    /// Issue ID to fetch and fix (e.g. "PROJ-123", "42").
    pub issue_id: Option<String>,

    /// Additional instructions for the agent beyond the issue description.
    #[arg(long)]
    pub prompt: Option<String>,

    /// Fix all issues marked as "ready" in connected trackers.
    #[arg(long)]
    pub ready: bool,

    /// Maximum number of ready issues to fix (used with --ready).
    #[arg(long)]
    pub max: Option<usize>,

    /// Queue ready issues as parallel tasks instead of running sequentially.
    #[arg(long)]
    pub parallel: bool,

    /// Budget per run in USD.
    #[arg(long = "budget-usd")]
    pub budget_usd: Option<f64>,

    /// Claude model to use.
    #[arg(long)]
    pub model: Option<String>,
}

// ── Lint command ─────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct LintArgs {
    /// Run agents to automatically fix lint issues after reporting.
    #[arg(long)]
    pub fix: bool,

    /// Budget per fix-run in USD.
    #[arg(long = "budget-usd")]
    pub budget_usd: Option<f64>,

    /// Claude model to use for the fix-run.
    #[arg(long)]
    pub model: Option<String>,
}

// ── CI command ───────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct CiArgs {
    /// Branch to check (defaults to current branch).
    pub branch: Option<String>,

    /// Wait for all CI checks to finish (poll every 15s).
    #[arg(long)]
    pub wait: bool,

    /// Timeout in seconds when using --wait (default: 600).
    #[arg(long, default_value_t = 600)]
    pub timeout: u64,

    /// If CI is failing, spawn an agent run to fix the failures.
    #[arg(long)]
    pub fix: bool,

    /// Budget per fix-run in USD.
    #[arg(long = "budget-usd")]
    pub budget_usd: Option<f64>,

    /// Claude model to use for the fix-run.
    #[arg(long)]
    pub model: Option<String>,
}

// ── Cleanup command ──────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct CleanupArgs {
    /// Only clean worktrees belonging to this project.
    #[arg(long)]
    pub project: Option<String>,

    /// Only clean worktrees belonging to this conversation.
    #[arg(long)]
    pub conversation: Option<String>,

    /// Show what would be deleted without actually deleting.
    #[arg(long)]
    pub dry_run: bool,

    /// Skip confirmation prompt.
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Force-release ALL pool slots and delete ALL worktree directories,
    /// regardless of whether runs are active. Use with caution — this will
    /// interrupt any running agents.
    #[arg(long)]
    pub force: bool,
}

// ── GC command ──────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct GcArgs {
    /// Show what would be cleaned without actually cleaning.
    #[arg(long)]
    pub dry_run: bool,
}

// ── Git command ──────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct GitArgs {
    #[command(subcommand)]
    pub action: GitAction,

    /// Run ID to operate on. Defaults to the most recent run.
    #[arg(long, global = true)]
    pub run_id: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum GitAction {
    /// Show git status with staging info and per-file line counts.
    Status,
    /// Stage files for commit.
    Stage(GitStageArgs),
    /// Unstage previously staged files.
    Unstage(GitUnstageArgs),
    /// Revert files to their last committed state.
    Revert(GitRevertArgs),
    /// Commit staged (or all) changes.
    Commit(GitCommitArgs),
    /// Push the current branch to origin.
    Push,
    /// Pull changes from the remote tracking branch.
    Pull,
    /// Show current branch with ahead/behind counts.
    Branch,
    /// Show commit log.
    Log(GitLogArgs),
    /// Undo the last commit (soft reset). Blocked if already pushed.
    Undo,
    /// Create a pull request on GitHub via `gh`.
    Pr(GitPrArgs),
    /// Show the status of an existing pull request.
    PrStatus,
    /// Merge the current pull request.
    Merge(GitMergeArgs),
}

#[derive(Debug, Args)]
pub struct GitStageArgs {
    /// Files to stage. Use "." or omit to stage all.
    #[arg(default_value = ".")]
    pub paths: Vec<String>,
}

#[derive(Debug, Args)]
pub struct GitUnstageArgs {
    /// Files to unstage.
    pub paths: Vec<String>,
}

#[derive(Debug, Args)]
pub struct GitRevertArgs {
    /// Files to revert. If omitted, reverts all changes.
    pub paths: Vec<String>,

    /// Revert all changes (tracked and untracked).
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct GitCommitArgs {
    /// Commit message. Auto-generated if omitted.
    #[arg(short, long)]
    pub message: Option<String>,

    /// Include all unstaged changes (runs `git add -A` first).
    #[arg(long, short = 'a')]
    pub all: bool,

    /// Push after committing.
    #[arg(long)]
    pub push: bool,
}

#[derive(Debug, Args)]
pub struct GitLogArgs {
    /// Maximum number of commits to show.
    #[arg(long, short = 'n', default_value_t = 20)]
    pub max_count: u32,
}

#[derive(Debug, Args)]
pub struct GitPrArgs {
    /// PR title. Auto-generated if omitted.
    #[arg(long)]
    pub title: Option<String>,

    /// PR body. Auto-generated if omitted.
    #[arg(long)]
    pub body: Option<String>,

    /// Base branch for the PR. Auto-detected if omitted.
    #[arg(long)]
    pub base: Option<String>,

    /// Push before creating the PR (default: true).
    #[arg(long, default_value_t = true)]
    pub push: bool,
}

#[derive(Debug, Args)]
pub struct GitMergeArgs {
    /// Merge strategy: squash, merge, or rebase.
    #[arg(long, default_value = "squash")]
    pub strategy: String,

    /// Use admin privileges to bypass branch protection.
    #[arg(long)]
    pub admin: bool,
}
