//! Live status-watch TUI — shows all recent runs and auto-refreshes.
//!
//! For Task 16 this delegates to the dashboard. A dedicated multi-run
//! view is planned for Task 17.

#[cfg(feature = "tui")]
use crate::transport::GroveTransport;

/// Run the live status-watch loop (refreshing list of all runs).
#[cfg(feature = "tui")]
pub fn run(transport: GroveTransport) -> crate::error::CliResult<()> {
    super::dashboard::run(transport)
}
