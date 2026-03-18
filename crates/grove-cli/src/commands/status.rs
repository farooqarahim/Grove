use crate::cli::{
    AbortArgs, ConflictsArgs, LogsArgs, MergeStatusArgs, OwnershipArgs, PlanArgs, PublishArgs,
    ReportArgs, ResumeArgs, SessionsArgs, StatusArgs, SubtasksArgs,
};
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn status_cmd(_a: StatusArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn resume_cmd(_a: ResumeArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn abort_cmd(_a: AbortArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn logs_cmd(_a: LogsArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn report_cmd(_a: ReportArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn plan_cmd(_a: PlanArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn subtasks_cmd(_a: SubtasksArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn sessions_cmd(_a: SessionsArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn ownership_cmd(_a: OwnershipArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn conflicts_cmd(_a: ConflictsArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn merge_status_cmd(
    _a: MergeStatusArgs,
    _t: GroveTransport,
    _m: OutputMode,
) -> CliResult<()> {
    Ok(())
}

pub fn publish_cmd(_a: PublishArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
