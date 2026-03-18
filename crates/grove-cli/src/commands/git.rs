use std::path::Path;
use crate::cli::GitArgs;
use crate::error::CliResult;
use crate::output::OutputMode;

pub fn dispatch(_a: GitArgs, _p: &Path, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
