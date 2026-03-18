use std::path::Path;

use grove_core::app::GroveApp;

use crate::error::{CliError, CliResult};
use crate::output::{text, OutputMode};

pub fn run(project: &Path, mode: OutputMode) -> CliResult<()> {
    // Bootstrap the global Grove workspace (~/.grove).
    let _app = GroveApp::init()?;

    // Ensure the local .grove/ config dir exists for this project.
    let grove_dir = project.join(".grove");
    std::fs::create_dir_all(&grove_dir)
        .map_err(|e| CliError::Other(e.to_string()))?;

    match mode {
        OutputMode::Json => println!("{}", serde_json::json!({"ok": true})),
        OutputMode::Text { .. } => text::success("grove initialised"),
    }
    Ok(())
}
