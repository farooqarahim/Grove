use crate::cli::ConversationArgs;
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn dispatch(_a: ConversationArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> {
    Ok(())
}
