mod cli;
mod commands;
mod error;
mod output;
mod transport;
mod tui;

use clap::Parser;
use cli::Cli;

fn main() {
    let cli = Cli::parse();
    let mode_json = cli.json;

    let workspace_root = match grove_core::app::GroveApp::init() {
        Ok(app) => app.data_root.clone(),
        Err(e) => {
            if mode_json {
                println!("{}", output::json::emit_error_json(&e.to_string(), 1));
            } else {
                eprintln!("error: {e}");
            }
            std::process::exit(1);
        }
    };

    let transport = transport::GroveTransport::detect(&cli.project, &workspace_root);

    if let Err(e) = commands::dispatch(cli, transport) {
        if mode_json {
            println!(
                "{}",
                output::json::emit_error_json(&e.to_string(), e.exit_code())
            );
        } else {
            eprintln!("error: {e}");
        }
        std::process::exit(e.exit_code());
    }
}
