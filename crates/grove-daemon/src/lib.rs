//! grove-daemon library entry point. The binary thin-wraps `run()`.

pub mod config;
pub mod lifecycle;
pub mod queue_drain;
pub mod rpc;
pub mod server;
pub mod session_host;

use anyhow::Result;

/// Start the daemon with the given config. Blocks until shutdown signal.
pub async fn run(cfg: config::DaemonConfig) -> Result<()> {
    server::serve(cfg).await
}
