use crate::cli::LlmArgs;
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn dispatch(_a: LlmArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
