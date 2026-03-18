use std::path::Path;
use crate::cli::DoctorArgs;
use crate::error::CliResult;
use crate::output::OutputMode;

pub fn run(_a: DoctorArgs, _project: &Path, _mode: OutputMode) -> CliResult<()> {
    Ok(())
}
