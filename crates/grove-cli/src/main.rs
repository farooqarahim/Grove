mod cli;
mod commands;
mod error;
mod output;
mod transport;

use clap::Parser;
use cli::Cli;

fn main() {
    let cli = Cli::parse();
    let mode_json = cli.json;
    let transport = transport::GroveTransport::detect(&cli.project);

    if let Err(e) = commands::dispatch(cli, transport) {
        if mode_json {
            println!("{}", output::json::emit_error_json(&e.to_string(), e.exit_code()));
        } else {
            eprintln!("error: {e}");
        }
        std::process::exit(e.exit_code());
    }
}
