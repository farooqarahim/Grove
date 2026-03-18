use crate::cli::{CleanupArgs, GcArgs};
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn cleanup_cmd(_a: CleanupArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn gc_cmd(_a: GcArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
