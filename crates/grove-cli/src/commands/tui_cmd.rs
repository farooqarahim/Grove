#[cfg(feature = "tui")]
use crate::error::CliResult;
#[cfg(feature = "tui")]
use crate::transport::GroveTransport;

#[cfg(feature = "tui")]
pub fn run(transport: GroveTransport) -> CliResult<()> {
    crate::tui::dashboard::run(transport)
}
