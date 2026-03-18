use crate::cli::SignalArgs;
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn dispatch(_a: SignalArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
