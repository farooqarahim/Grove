use crate::cli::WorktreesArgs;
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn run(_a: WorktreesArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
