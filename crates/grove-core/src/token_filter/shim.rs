//! Shim directory management for PATH-based command interception.
//!
//! Before an agent spawns, Grove creates a `.grove-filter-bin/` directory in the
//! worktree containing symlinks for each known command — all pointing to the
//! `grove-filter` binary. When this directory is prepended to PATH, agents
//! transparently invoke `grove-filter` instead of the real command.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::project_type::{self, ProjectType};
use super::session::FilterState;
use super::token_count;

/// Info returned after shim setup, used to inject env vars into the agent process.
#[derive(Debug)]
pub struct ShimSetup {
    pub shim_dir: PathBuf,
    pub state_file: PathBuf,
}

/// Commands to create shims for, grouped by project type.
const ALWAYS_SHIMMED: &[&str] = &["git", "cat", "head", "tail", "less", "bat"];
const RUST_SHIMMED: &[&str] = &["cargo"];
const NODE_SHIMMED: &[&str] = &[
    "tsc", "eslint", "vitest", "jest", "npx", "node", "next", "pnpm", "npm", "yarn", "bun",
];
const PYTHON_SHIMMED: &[&str] = &["pytest", "python", "ruff", "mypy"];
const GO_SHIMMED: &[&str] = &["go"];

/// Set up the shim directory and initial filter state for a run.
///
/// Returns `None` if the grove-filter binary cannot be located (graceful
/// degradation — the agent runs without output filtering).
pub fn setup(
    worktree: &Path,
    run_id: &str,
    model: &str,
    config: Option<&crate::config::TokenFilterConfig>,
) -> Option<ShimSetup> {
    // Respect the enabled flag — return None to skip filtering entirely.
    if let Some(cfg) = config {
        if !cfg.enabled {
            return None;
        }
    }

    let filter_bin = resolve_grove_filter_binary()?;

    let shim_dir = worktree.join(".grove-filter-bin");
    let state_file = worktree.join(".grove-filter-state.json");

    // Clean up any stale shim state from a previous run.
    let _ = fs::remove_dir_all(&shim_dir);
    if let Err(e) = fs::create_dir_all(&shim_dir) {
        tracing::warn!(error = %e, "failed to create shim directory");
        return None;
    }

    // Detect project types to determine which commands to shim.
    let project_types = project_type::detect(worktree);
    let window_size = token_count::model_window_size(model);

    // Build the list of commands to shim.
    let mut commands: Vec<&str> = ALWAYS_SHIMMED.to_vec();
    for pt in &project_types {
        match pt {
            ProjectType::Rust => commands.extend_from_slice(RUST_SHIMMED),
            ProjectType::Node => commands.extend_from_slice(NODE_SHIMMED),
            ProjectType::Python => commands.extend_from_slice(PYTHON_SHIMMED),
            ProjectType::Go => commands.extend_from_slice(GO_SHIMMED),
        }
    }

    // Create symlinks: each command name → grove-filter binary.
    for cmd in &commands {
        let link_path = shim_dir.join(cmd);
        if let Err(e) = symlink(&filter_bin, &link_path) {
            tracing::debug!(
                command = %cmd,
                error = %e,
                "failed to create shim symlink — skipping"
            );
        }
    }

    // Write initial filter state with config limits if available.
    let state = match config {
        Some(cfg) => FilterState::with_config(run_id.to_string(), project_types, window_size, cfg),
        None => FilterState::new(run_id.to_string(), project_types, window_size),
    };
    state.save(&state_file);

    Some(ShimSetup {
        shim_dir,
        state_file,
    })
}

/// Locate the grove-filter binary.
///
/// Search order:
/// 1. `GROVE_FILTER_BIN` env var (explicit override)
/// 2. Adjacent to the current executable
/// 3. Cargo target directory (debug/release builds)
fn resolve_grove_filter_binary() -> Option<PathBuf> {
    // 1. Explicit env var
    if let Ok(path) = std::env::var("GROVE_FILTER_BIN") {
        let p = PathBuf::from(&path);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. Adjacent to current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let adjacent = dir.join("grove-filter");
            if adjacent.exists() {
                return Some(adjacent);
            }
        }
    }

    // 3. Cargo target dirs (for development builds)
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let workspace = PathBuf::from(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf());

        if let Some(ws) = workspace {
            for profile in &["debug", "release"] {
                let candidate = ws.join("target").join(profile).join("grove-filter");
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// Remove shim directory and state file after a run completes.
pub fn teardown(worktree: &Path) {
    let shim_dir = worktree.join(".grove-filter-bin");
    let state_file = worktree.join(".grove-filter-state.json");

    if shim_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&shim_dir) {
            tracing::debug!(error = %e, "failed to remove shim directory");
        }
    }
    if state_file.exists() {
        if let Err(e) = fs::remove_file(&state_file) {
            tracing::debug!(error = %e, "failed to remove filter state file");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teardown_nonexistent_is_safe() {
        let tmp = tempfile::tempdir().unwrap();
        teardown(tmp.path()); // Should not panic
    }

    #[test]
    fn teardown_cleans_up() {
        let tmp = tempfile::tempdir().unwrap();
        let shim_dir = tmp.path().join(".grove-filter-bin");
        let state_file = tmp.path().join(".grove-filter-state.json");

        fs::create_dir_all(&shim_dir).unwrap();
        fs::write(&state_file, "{}").unwrap();

        teardown(tmp.path());

        assert!(!shim_dir.exists());
        assert!(!state_file.exists());
    }
}
