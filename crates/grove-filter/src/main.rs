//! grove-filter — PATH shim binary for token reduction.
//!
//! This binary is invoked via symlinks named after real commands (git, cargo, etc.).
//! It detects which command it was invoked as via `argv[0]`, runs the real command
//! (found by searching PATH minus the shim directory), captures stdout, applies
//! the appropriate filter, updates session state, and prints filtered output.
//!
//! Exit code is always propagated from the real command.
//!
//! Security: All subprocess invocations use Rust's `std::process::Command` which
//! does NOT invoke a shell — arguments are passed directly, preventing injection.

use std::env;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use grove_core::token_filter::session::{CommandStat, FilterState};
use grove_core::token_filter::static_filters;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Detect which command we were invoked as via argv[0].
    let command_name = match args.first() {
        Some(arg0) => Path::new(arg0)
            .file_name()
            .unwrap_or(OsStr::new("unknown"))
            .to_string_lossy()
            .to_string(),
        None => {
            eprintln!("[grove-filter] no argv[0] — cannot determine command");
            std::process::exit(127);
        }
    };

    // TTY passthrough: if stdin is a terminal, the agent is running interactively.
    // Replace the process image with the real binary — no interception, no filtering.
    // This prevents breaking interactive tools that depend on terminal state.
    #[cfg(unix)]
    {
        if unsafe { libc::isatty(0) } == 1 {
            run_passthrough(&command_name, &args[1..]);
        }
    }

    // Load session state from the env-specified path.
    let state_path = match env::var("GROVE_FILTER_STATE") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            // No state file — run the real command raw (graceful degradation).
            run_passthrough(&command_name, &args[1..]);
        }
    };

    let shim_dir = env::var("GROVE_FILTER_BIN_DIR").unwrap_or_default();

    // Find the real binary by searching PATH, excluding the shim directory.
    let real_path = match find_real_binary(&command_name, &shim_dir) {
        Some(p) => p,
        None => {
            eprintln!(
                "[grove-filter] cannot find real '{}' in PATH (excluding shim dir)",
                command_name
            );
            std::process::exit(127);
        }
    };

    // Load filter state — fall back to passthrough on any failure.
    let mut state = match FilterState::load(&state_path) {
        Some(s) => s,
        None => {
            run_real_command_raw(&real_path, &args[1..]);
        }
    };

    // Run the real command and capture output.
    // Note: std::process::Command does NOT use a shell — safe from injection.
    let output = match Command::new(&real_path)
        .args(&args[1..])
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[grove-filter] failed to run '{}': {}", command_name, e);
            std::process::exit(127);
        }
    };

    let exit_code = output.status.code().unwrap_or(1);

    // Always forward stderr unfiltered.
    if !output.stderr.is_empty() {
        let _ = std::io::stderr().write_all(&output.stderr);
    }

    // On non-zero exit: print raw stdout (errors are too important to filter).
    if !output.status.success() {
        let _ = std::io::stdout().write_all(&output.stdout);
        std::process::exit(exit_code);
    }

    // Apply filter to stdout.
    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let result = static_filters::apply(
        &command_name,
        &stdout_text,
        state.compression_level,
        &mut state,
    );

    // Update session state.
    state.record_invocation(CommandStat {
        command: format!("{} {}", command_name, args[1..].join(" ")),
        filter_type: result.filter_type,
        raw_bytes: result.raw_bytes,
        filtered_bytes: result.filtered_bytes,
        compression_level: state.compression_level,
    });
    state.save(&state_path);

    // Print filtered output.
    print!("{}", result.text);
    std::process::exit(exit_code);
}

/// Find the real binary for `name` by searching PATH entries, excluding `shim_dir`.
///
/// This prevents the shim from calling itself in an infinite loop.
fn find_real_binary(name: &str, shim_dir: &str) -> Option<PathBuf> {
    let path_var = env::var("PATH").unwrap_or_default();
    let shim_canonical = if shim_dir.is_empty() {
        PathBuf::new()
    } else {
        std::fs::canonicalize(shim_dir).unwrap_or_else(|_| PathBuf::from(shim_dir))
    };

    for dir in env::split_paths(&path_var) {
        // Skip the shim directory to prevent infinite recursion.
        let dir_canonical = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if !shim_dir.is_empty() && dir_canonical == shim_canonical {
            continue;
        }
        // Also skip if the directory path string matches exactly.
        if !shim_dir.is_empty() && dir.to_string_lossy() == shim_dir {
            continue;
        }

        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Run the real command without any filtering (passthrough mode).
///
/// On Unix, replaces the current process image via execv for zero overhead.
/// This function never returns.
fn run_passthrough(command_name: &str, args: &[String]) -> ! {
    let shim_dir = env::var("GROVE_FILTER_BIN_DIR").unwrap_or_default();

    let real_path = find_real_binary(command_name, &shim_dir).unwrap_or_else(|| {
        eprintln!("[grove-filter] cannot find '{}' in PATH", command_name);
        std::process::exit(127);
    });

    run_real_command_raw(&real_path, args);
}

/// Replace the current process with the real command (no filtering).
/// This function never returns.
fn run_real_command_raw(real_path: &Path, args: &[String]) -> ! {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // execv replaces the process — no shell involved, safe from injection.
        let err = Command::new(real_path).args(args).exec();
        eprintln!("[grove-filter] failed to replace process: {}", err);
        std::process::exit(127);
    }

    #[cfg(not(unix))]
    {
        let status = Command::new(real_path)
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();
        match status {
            Ok(s) => std::process::exit(s.code().unwrap_or(1)),
            Err(e) => {
                eprintln!("[grove-filter] failed to run command: {}", e);
                std::process::exit(127);
            }
        }
    }
}
