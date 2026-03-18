mod error;
mod output;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(e.exit_code());
    }
}

fn run() -> error::CliResult<()> {
    Ok(())
}
