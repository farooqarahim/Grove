use anyhow::Result;
use serde_json::Value;

use crate::cli::{Commands, OutputFormat};
use crate::command_context::CommandContext;

pub mod abort;
pub mod auth;
pub mod ci;
pub mod cleanup;
pub mod conflicts;
pub mod connect;
pub mod conversation;
pub mod costs;
pub mod doctor;
pub mod fix;
pub mod gc;
pub mod git;
pub mod hook;
pub mod init;
pub mod issue;
pub mod lint;
pub mod llm;
pub mod logs;
pub mod merge_status;
pub mod ownership;
pub mod plan;
pub mod project;
pub mod publish;
pub mod queue;
pub mod report;
pub mod resume;
pub mod run;
pub mod sessions;
pub mod signal;
pub mod status;
pub mod subtasks;
pub mod task_cancel;
pub mod tasks;
pub mod workspace;
pub mod worktrees;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub text: String,
    pub json: Value,
}

pub fn dispatch(ctx: &CommandContext, command: &Commands) -> Result<CommandOutput> {
    match command {
        Commands::Init(args) => init::handle(ctx, args),
        Commands::Doctor(args) => doctor::handle(ctx, args),
        Commands::Run(args) => run::handle(ctx, args),
        Commands::Queue(args) => queue::handle(ctx, args),
        Commands::Tasks(args) => tasks::handle(ctx, args),
        Commands::TaskCancel(args) => task_cancel::handle(ctx, args),
        Commands::Status(args) => status::handle(ctx, args),
        Commands::Resume(args) => resume::handle(ctx, args),
        Commands::Abort(args) => abort::handle(ctx, args),
        Commands::Logs(args) => logs::handle(ctx, args),
        Commands::Report(args) => report::handle(ctx, args),
        Commands::Worktrees(args) => worktrees::handle(ctx, args),
        Commands::Costs(args) => costs::handle(ctx, args),
        Commands::Subtasks(args) => subtasks::handle(ctx, args),
        Commands::Plan(args) => plan::handle(ctx, args),
        Commands::Sessions(args) => sessions::handle(ctx, args),
        Commands::Ownership(args) => ownership::handle(ctx, args),
        Commands::MergeStatus(args) => merge_status::handle(ctx, args),
        Commands::Conflicts(args) => conflicts::handle(ctx, args),
        Commands::Workspace(args) => workspace::handle(ctx, args),
        Commands::Project(args) => project::handle(ctx, args),
        Commands::Conversation(args) => conversation::handle(ctx, args),
        Commands::Auth(args) => auth::handle(ctx, args),
        Commands::Llm(args) => llm::handle(ctx, args),
        Commands::Signal(args) => signal::handle(ctx, args),
        Commands::Hook(args) => hook::handle(ctx, args),
        Commands::Issue(args) => issue::handle(ctx, args),
        Commands::Publish(args) => publish::handle(ctx, args),
        Commands::Fix(args) => fix::handle(ctx, args),
        Commands::Connect(args) => connect::handle(ctx, args),
        Commands::Lint(args) => lint::handle(ctx, args),
        Commands::Ci(args) => ci::handle(ctx, args),
        Commands::Cleanup(args) => cleanup::handle(ctx, args),
        Commands::Gc(args) => gc::handle(ctx, args),
        Commands::Git(args) => git::handle(ctx, args),
    }
}

pub fn render_path(path: &std::path::Path) -> String {
    path.to_string_lossy().to_string()
}

pub fn to_text_or_json(format: OutputFormat, text: String, json: Value) -> CommandOutput {
    match format {
        OutputFormat::Text | OutputFormat::Json => CommandOutput { text, json },
    }
}
