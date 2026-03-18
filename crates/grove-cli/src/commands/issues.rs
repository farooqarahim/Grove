use crate::cli::{CiArgs, ConnectArgs, FixArgs, IssueArgs, LintArgs};
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn dispatch(_a: IssueArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn fix_cmd(_a: FixArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn connect_dispatch(_a: ConnectArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn lint_cmd(_a: LintArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn ci_cmd(_a: CiArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
