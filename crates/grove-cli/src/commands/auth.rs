use crate::cli::AuthArgs;
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn dispatch(_a: AuthArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
