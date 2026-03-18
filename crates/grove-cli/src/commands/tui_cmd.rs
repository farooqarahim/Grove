use crate::error::CliResult;
use crate::transport::GroveTransport;

#[cfg(feature = "tui")]
pub fn run(_t: GroveTransport) -> CliResult<()> {
    Ok(())
}
