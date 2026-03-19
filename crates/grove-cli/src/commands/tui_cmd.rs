use crate::error::CliResult;
use crate::transport::GroveTransport;

#[cfg(feature = "tui")]
pub fn run(transport: GroveTransport) -> CliResult<()> {
    crate::tui::dashboard::run(transport)
}
