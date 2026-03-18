use anyhow::Result;

use crate::cli::OutputFormat;
use crate::commands::CommandOutput;

pub mod json;
pub mod text;

pub fn print_output(format: OutputFormat, output: &CommandOutput) -> Result<()> {
    match format {
        OutputFormat::Text => text::print(&output.text),
        OutputFormat::Json => json::print(&output.json),
    }
}
