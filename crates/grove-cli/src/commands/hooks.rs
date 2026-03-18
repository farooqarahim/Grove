use std::path::Path;
use crate::cli::HookArgs;
use crate::error::CliResult;
use crate::output::OutputMode;

pub fn run(_a: HookArgs, _p: &Path, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
