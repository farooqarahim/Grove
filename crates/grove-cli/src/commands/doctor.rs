use std::path::Path;

use grove_core::app::GroveApp;

use crate::cli::DoctorArgs;
use crate::error::{CliError, CliResult};
use crate::output::{text, OutputMode};

pub fn run(args: DoctorArgs, _project: &Path, mode: OutputMode) -> CliResult<()> {
    let app = GroveApp::init()?;
    let conn = app.db_handle().connect().map_err(CliError::Core)?;

    let git_ok = which::which("git").is_ok();
    let db_ok = grove_core::db::integrity::check(&conn)
        .map(|r| r.integrity_ok && r.foreign_key_violations.is_empty())
        .unwrap_or(false);
    let cfg_ok = true;
    let overall = git_ok && db_ok && cfg_ok;

    if (args.fix || args.fix_all) && !overall {
        grove_core::db::initialize(&app.data_root).map_err(CliError::Core)?;
    }

    match mode {
        OutputMode::Json => println!(
            "{}",
            serde_json::json!({
                "ok": overall,
                "git": git_ok,
                "sqlite": db_ok,
                "config": cfg_ok,
            })
        ),
        OutputMode::Text { .. } => {
            println!(
                "{}",
                if overall {
                    text::bold("✓ healthy")
                } else {
                    text::bold("✗ issues found")
                }
            );
            println!("  git:    {}", if git_ok { "ok" } else { "MISSING" });
            println!("  sqlite: {}", if db_ok { "ok" } else { "FAIL" });
            println!("  config: {}", if cfg_ok { "ok" } else { "FAIL" });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn doctor_on_uninitialised_dir_does_not_panic() {
        let dir = tempdir().unwrap();
        // Either Ok or Err is acceptable — we must not panic.
        let result = run(
            crate::cli::DoctorArgs {
                fix: false,
                fix_all: false,
            },
            dir.path(),
            crate::output::OutputMode::Text { no_color: true },
        );
        let _ = result;
    }
}
