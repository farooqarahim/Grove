mod cli;
mod command_context;
mod commands;
mod error_envelope;
mod exit_codes;
mod output;

use anyhow::Result;
use clap::Parser;

fn main() {
    let exit_code = match run() {
        Ok(()) => exit_codes::SUCCESS,
        Err(err) => {
            let classified = error_envelope::classify(&err);
            let envelope = error_envelope::to_json(&classified);
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&envelope)
                    .unwrap_or_else(|_| "{\"error\":{\"code\":\"UNKNOWN\"}}".to_string())
            );
            classified.exit_code
        }
    };

    std::process::exit(exit_code);
}

fn run() -> Result<()> {
    let cli = cli::Cli::parse();
    let ctx = command_context::CommandContext::from_cli(&cli);
    let output = commands::dispatch(&ctx, &cli.command)?;
    output::print_output(ctx.format, &output)?;
    Ok(())
}
