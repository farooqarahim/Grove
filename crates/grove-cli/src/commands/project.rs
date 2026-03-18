use std::path::Path;
use crate::cli::ProjectArgs;
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn dispatch(
    _a: ProjectArgs,
    _p: &Path,
    _t: GroveTransport,
    _m: OutputMode,
) -> CliResult<()> {
    Ok(())
}
