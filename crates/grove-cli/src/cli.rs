use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "grove", about = "Grove — AI-powered development platform CLI")]
pub struct Cli {
    #[arg(long, global = true, default_value = ".")]
    pub project: PathBuf,
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub verbose: bool,
    #[arg(long = "no-color", global = true)]
    pub no_color: bool,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PermissionModeArg {
    SkipAll,
    HumanGate,
    AutonomousGate,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init,
    Doctor(DoctorArgs),
    Run(RunArgs),
    Queue(QueueArgs),
    Tasks(TasksArgs),
    TaskCancel(TaskCancelArgs),
    Status(StatusArgs),
    Resume(ResumeArgs),
    Abort(AbortArgs),
    Logs(LogsArgs),
    Report(ReportArgs),
    Plan(PlanArgs),
    Subtasks(SubtasksArgs),
    Sessions(SessionsArgs),
    Git(GitArgs),
    Issue(IssueArgs),
    Fix(FixArgs),
    Connect(ConnectArgs),
    Auth(AuthArgs),
    Llm(LlmArgs),
    Workspace(WorkspaceArgs),
    Project(ProjectArgs),
    Conversation(ConversationArgs),
    Signal(SignalArgs),
    Hook(HookArgs),
    Worktrees(WorktreesArgs),
    Cleanup(CleanupArgs),
    Gc(GcArgs),
    Ownership(OwnershipArgs),
    Conflicts(ConflictsArgs),
    MergeStatus(MergeStatusArgs),
    Publish(PublishArgs),
    Lint(LintArgs),
    Ci(CiArgs),
    #[cfg(feature = "tui")]
    Tui,
}

// ── Bootstrap ─────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub fix: bool,
    #[arg(long = "fix-all")]
    pub fix_all: bool,
}

// ── Runs ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct RunArgs {
    pub objective: String,
    #[arg(long = "max-agents")]
    pub max_agents: Option<u16>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub pipeline: Option<String>,
    #[arg(long = "permission-mode", value_enum)]
    pub permission_mode: Option<PermissionModeArg>,
    #[arg(long)]
    pub conversation: Option<String>,
    #[arg(long = "continue-last", short = 'c')]
    pub continue_last: bool,
    #[arg(long)]
    pub issue: Option<String>,
    /// Live TUI view (requires feature = "tui"). Errors at runtime on lean binary.
    #[arg(long)]
    pub watch: bool,
}

#[derive(Debug, Args)]
pub struct QueueArgs {
    pub objective: String,
    #[arg(long, default_value_t = 0)]
    pub priority: i64,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub conversation: Option<String>,
    #[arg(long = "continue-last", short = 'c')]
    pub continue_last: bool,
}

#[derive(Debug, Args)]
pub struct TasksArgs {
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(i64).range(1..))]
    pub limit: i64,
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Debug, Args)]
pub struct TaskCancelArgs {
    pub task_id: String,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: i64,
    /// Live TUI view (requires feature = "tui"). Errors at runtime on lean binary.
    #[arg(long)]
    pub watch: bool,
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
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    pub run_id: String,
}

#[derive(Debug, Args)]
pub struct PlanArgs {
    pub run_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct SubtasksArgs {
    pub run_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct SessionsArgs {
    pub run_id: String,
}

// ── Git ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct GitArgs {
    #[command(subcommand)]
    pub action: GitAction,
}

#[derive(Debug, Subcommand)]
pub enum GitAction {
    Status,
    Stage {
        paths: Vec<String>,
    },
    Unstage {
        paths: Vec<String>,
    },
    Revert {
        paths: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    Commit {
        #[arg(short = 'm')]
        msg: Option<String>,
        #[arg(short = 'a')]
        all: bool,
        #[arg(long)]
        push: bool,
    },
    Push,
    Pull,
    Branch,
    Log {
        #[arg(short = 'n', default_value_t = 10)]
        n: u32,
    },
    Undo,
    Pr {
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        base: Option<String>,
        #[arg(long)]
        push: bool,
    },
    PrStatus,
    Merge {
        #[arg(long, value_enum)]
        strategy: Option<MergeStrategy>,
        #[arg(long)]
        admin: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MergeStrategy {
    Squash,
    Merge,
    Rebase,
}

// ── Issues ────────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct IssueArgs {
    #[command(subcommand)]
    pub action: IssueAction,
}

#[derive(Debug, Subcommand)]
pub enum IssueAction {
    List {
        #[arg(long)]
        cached: bool,
    },
    Show {
        id: String,
    },
    Create {
        title: String,
        #[arg(long)]
        body: Option<String>,
        #[arg(long)]
        labels: Vec<String>,
        #[arg(long)]
        priority: Option<String>,
    },
    Close {
        id: String,
    },
    Update {
        id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        label: Vec<String>,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long)]
        priority: Option<String>,
    },
    Comment {
        id: String,
        body: String,
    },
    Assign {
        id: String,
        assignee: String,
    },
    Move {
        id: String,
        status: String,
    },
    Reopen {
        id: String,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u32,
        #[arg(long)]
        provider: Option<String>,
    },
    Sync {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        full: bool,
    },
    Board {
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long)]
        priority: Option<String>,
    },
    BoardConfig {
        #[command(subcommand)]
        action: BoardConfigAction,
    },
    Activity {
        id: String,
    },
    Ready,
    Push {
        id: String,
        #[arg(long = "to")]
        to: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum BoardConfigAction {
    Show,
    Set {
        #[arg(long)]
        file: String,
    },
    Reset,
}

#[derive(Debug, Args)]
pub struct FixArgs {
    pub issue_id: Option<String>,
    #[arg(long)]
    pub prompt: Option<String>,
    #[arg(long)]
    pub ready: bool,
    #[arg(long)]
    pub max: Option<u32>,
    #[arg(long)]
    pub parallel: bool,
}

#[derive(Debug, Args)]
pub struct ConnectArgs {
    #[command(subcommand)]
    pub action: ConnectAction,
}

#[derive(Debug, Subcommand)]
pub enum ConnectAction {
    Github {
        #[arg(long)]
        token: Option<String>,
    },
    Jira {
        #[arg(long)]
        site: String,
        #[arg(long)]
        email: String,
        #[arg(long)]
        token: String,
    },
    Linear {
        #[arg(long)]
        token: String,
    },
    Status,
    Disconnect {
        provider: String,
    },
}

// ── Auth & LLM ────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub action: AuthAction,
}

#[derive(Debug, Subcommand)]
pub enum AuthAction {
    Set {
        provider: String,
        api_key: String,
    },
    Remove {
        provider: String,
    },
    List,
}

#[derive(Debug, Args)]
pub struct LlmArgs {
    #[command(subcommand)]
    pub action: LlmAction,
}

#[derive(Debug, Subcommand)]
pub enum LlmAction {
    List,
    Models {
        provider: String,
    },
    Select {
        provider: String,
        model: Option<String>,
        #[arg(long = "own-key")]
        own_key: bool,
        #[arg(long = "workspace-credits")]
        workspace_credits: bool,
    },
}

// ── Workspace & Project ───────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub action: WorkspaceAction,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceAction {
    Show,
    SetName {
        name: String,
    },
    Archive {
        id: String,
    },
    Delete {
        id: String,
    },
}

#[derive(Debug, Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub action: ProjectAction,
}

#[derive(Debug, Subcommand)]
pub enum ProjectAction {
    Show,
    List,
    OpenFolder {
        path: String,
        #[arg(long)]
        name: Option<String>,
    },
    Clone {
        repo: String,
        path: String,
        #[arg(long)]
        name: Option<String>,
    },
    CreateRepo {
        repo: String,
        path: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        visibility: Option<String>,
        #[arg(long)]
        gitignore: Option<String>,
    },
    ForkRepo {
        src: String,
        target: String,
        repo: String,
        #[arg(long)]
        provider: Option<String>,
    },
    ForkFolder {
        src: String,
        target: String,
        #[arg(long = "preserve-git")]
        preserve_git: bool,
    },
    Ssh {
        host: String,
        remote_path: String,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        port: Option<u16>,
    },
    SshShell {
        id: Option<String>,
    },
    SetName {
        name: String,
    },
    Set {
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        parallel: Option<u32>,
        #[arg(long)]
        pipeline: Option<String>,
        #[arg(long = "permission-mode")]
        permission_mode: Option<String>,
        #[arg(long)]
        reset: bool,
    },
    Archive {
        id: Option<String>,
    },
    Delete {
        id: Option<String>,
    },
}

// ── Conversations ─────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct ConversationArgs {
    #[command(subcommand)]
    pub action: ConversationAction,
}

#[derive(Debug, Subcommand)]
pub enum ConversationAction {
    List {
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    Show {
        id: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },
    Archive {
        id: String,
    },
    Delete {
        id: String,
    },
    Rebase {
        id: String,
    },
    Merge {
        id: String,
    },
}

// ── Plumbing ──────────────────────────────────────────────────────────────────

#[derive(Debug, Args)]
pub struct SignalArgs {
    #[command(subcommand)]
    pub action: SignalAction,
}

#[derive(Debug, Subcommand)]
pub enum SignalAction {
    Send {
        run_id: String,
        from: String,
        to: String,
        signal_type: String,
        #[arg(long)]
        payload: Option<String>,
        #[arg(long)]
        priority: Option<i32>,
    },
    Check {
        run_id: String,
        agent: String,
    },
    List {
        run_id: String,
    },
}

#[derive(Debug, Args)]
pub struct HookArgs {
    pub event: String,
    pub agent_type: String,
    #[arg(long = "run-id")]
    pub run_id: Option<String>,
    #[arg(long = "session-id")]
    pub session_id: Option<String>,
    #[arg(long)]
    pub tool: Option<String>,
    #[arg(long = "file-path")]
    pub file_path: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorktreesArgs {
    #[arg(long)]
    pub clean: bool,
    #[arg(long)]
    pub delete: Option<String>,
    #[arg(long = "delete-all")]
    pub delete_all: bool,
    #[arg(short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct CleanupArgs {
    #[arg(long)]
    pub project: bool,
    #[arg(long)]
    pub conversation: bool,
    #[arg(long = "dry-run")]
    pub dry_run: bool,
    #[arg(short = 'y')]
    pub yes: bool,
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct GcArgs {
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct OwnershipArgs {
    pub run_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConflictsArgs {
    #[arg(long)]
    pub show: Option<String>,
    #[arg(long)]
    pub resolve: Option<String>,
}

#[derive(Debug, Args)]
pub struct MergeStatusArgs {
    pub conversation_id: String,
}

#[derive(Debug, Args)]
pub struct PublishArgs {
    #[command(subcommand)]
    pub action: PublishAction,
}

#[derive(Debug, Subcommand)]
pub enum PublishAction {
    Retry { run_id: String },
}

#[derive(Debug, Args)]
pub struct LintArgs {
    #[arg(long)]
    pub fix: bool,
    #[arg(long)]
    pub model: Option<String>,
}

#[derive(Debug, Args)]
pub struct CiArgs {
    pub branch: Option<String>,
    #[arg(long)]
    pub wait: bool,
    #[arg(long)]
    pub timeout: Option<u64>,
    #[arg(long)]
    pub fix: bool,
    #[arg(long)]
    pub model: Option<String>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_run_command() {
        let cli = Cli::try_parse_from(["grove", "run", "add dark mode"]).unwrap();
        match cli.command {
            Commands::Run(a) => assert_eq!(a.objective, "add dark mode"),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parses_json_global_flag() {
        let cli = Cli::try_parse_from(["grove", "--json", "status"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn parses_status_limit() {
        let cli = Cli::try_parse_from(["grove", "status", "--limit", "5"]).unwrap();
        match cli.command {
            Commands::Status(a) => assert_eq!(a.limit, 5),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn run_watch_flag_parses() {
        let cli = Cli::try_parse_from(["grove", "run", "obj", "--watch"]).unwrap();
        match cli.command {
            Commands::Run(a) => assert!(a.watch),
            _ => panic!(),
        }
    }
}
