use crate::cli::{QueueArgs, RunArgs, TaskCancelArgs, TasksArgs};
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn run_cmd(_a: RunArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn queue_cmd(_a: QueueArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn tasks_cmd(_a: TasksArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}

pub fn task_cancel_cmd(_a: TaskCancelArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
