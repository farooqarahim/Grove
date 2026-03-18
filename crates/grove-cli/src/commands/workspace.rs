use crate::cli::WorkspaceArgs;
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn dispatch(_a: WorkspaceArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
