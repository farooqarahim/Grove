use std::path::Path;
use crate::error::CliResult;
use crate::output::OutputMode;

pub fn run(_project: &Path, _mode: OutputMode) -> CliResult<()> {
    Ok(())
}
