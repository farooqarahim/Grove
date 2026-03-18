mod error;
mod output;
mod transport;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(e.exit_code());
    }
}

fn run() -> error::CliResult<()> {
    Ok(())
}
