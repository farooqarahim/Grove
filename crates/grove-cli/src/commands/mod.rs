use crate::cli::{Cli, Commands};
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub mod auth;
pub mod cleanup;
pub mod conversation;
pub mod doctor;
pub mod git;
pub mod hooks;
pub mod init;
pub mod issues;
pub mod llm;
pub mod project;
pub mod run;
pub mod signals;
pub mod status;
pub mod worktrees;
pub mod workspace;

#[cfg(feature = "tui")]
pub mod tui_cmd;

pub fn dispatch(cli: Cli, transport: GroveTransport) -> CliResult<()> {
    let mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Text {
            no_color: cli.no_color,
        }
    };
    let p = &cli.project;

    match cli.command {
        Commands::Init => init::run(p, mode),
        Commands::Doctor(a) => doctor::run(a, p, mode),
        Commands::Run(a) => run::run_cmd(a, transport, mode),
        Commands::Queue(a) => run::queue_cmd(a, transport, mode),
        Commands::Tasks(a) => run::tasks_cmd(a, transport, mode),
        Commands::TaskCancel(a) => run::task_cancel_cmd(a, transport, mode),
        Commands::Status(a) => status::status_cmd(a, transport, mode),
        Commands::Resume(a) => status::resume_cmd(a, transport, mode),
        Commands::Abort(a) => status::abort_cmd(a, transport, mode),
        Commands::Logs(a) => status::logs_cmd(a, transport, mode),
        Commands::Report(a) => status::report_cmd(a, transport, mode),
        Commands::Plan(a) => status::plan_cmd(a, transport, mode),
        Commands::Subtasks(a) => status::subtasks_cmd(a, transport, mode),
        Commands::Sessions(a) => status::sessions_cmd(a, transport, mode),
        Commands::Ownership(a) => status::ownership_cmd(a, transport, mode),
        Commands::Conflicts(a) => status::conflicts_cmd(a, transport, mode),
        Commands::MergeStatus(a) => status::merge_status_cmd(a, transport, mode),
        Commands::Publish(a) => status::publish_cmd(a, transport, mode),
        Commands::Git(a) => git::dispatch(a, p, mode),
        Commands::Issue(a) => issues::dispatch(a, transport, mode),
        Commands::Fix(a) => issues::fix_cmd(a, transport, mode),
        Commands::Connect(a) => issues::connect_dispatch(a, transport, mode),
        Commands::Lint(a) => issues::lint_cmd(a, transport, mode),
        Commands::Ci(a) => issues::ci_cmd(a, transport, mode),
        Commands::Auth(a) => auth::dispatch(a, transport, mode),
        Commands::Llm(a) => llm::dispatch(a, transport, mode),
        Commands::Workspace(a) => workspace::dispatch(a, transport, mode),
        Commands::Project(a) => project::dispatch(a, p, transport, mode),
        Commands::Conversation(a) => conversation::dispatch(a, transport, mode),
        Commands::Signal(a) => signals::dispatch(a, transport, mode),
        Commands::Hook(a) => hooks::run(a, p, mode),
        Commands::Worktrees(a) => worktrees::run(a, transport, mode),
        Commands::Cleanup(a) => cleanup::cleanup_cmd(a, transport, mode),
        Commands::Gc(a) => cleanup::gc_cmd(a, transport, mode),
        #[cfg(feature = "tui")]
        Commands::Tui => tui_cmd::run(transport),
    }
}
