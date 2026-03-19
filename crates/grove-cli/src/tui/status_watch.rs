//! Live status-watch TUI — shows all recent runs and auto-refreshes.

#[cfg(feature = "tui")]
use crate::transport::GroveTransport;

/// Run the live status-watch loop (refreshing list of all runs).
#[cfg(feature = "tui")]
pub fn run(transport: GroveTransport) -> crate::error::CliResult<()> {
    super::run_watch::run_status_watch(transport)
}
