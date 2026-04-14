use anyhow::Result;
use clap::Parser;
use grove_daemon::config::DaemonConfig;

#[derive(Parser, Debug)]
#[command(name = "grove-daemon", about = "Grove background process")]
struct Args {
    /// Project root (defaults to CWD).
    #[arg(long)]
    project_root: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let cwd = std::env::current_dir()?;
    let project_root = args.project_root.unwrap_or(cwd);
    let cfg = DaemonConfig::from_project_root(&project_root)?;
    grove_daemon::run(cfg).await
}
