use std::fs;
use std::path::Path;

use crate::config::{self, GroveConfig};
use crate::db;
use crate::errors::GroveResult;

use super::checks::{CheckResult, CheckStatus};

#[derive(Debug, Clone)]
pub enum FixOutcome {
    /// Fix was applied; description of what was done.
    Applied(String),
    /// This check name has no automated fix, or the check already passed.
    NotApplicable,
    /// Fix was attempted but failed; reason provided.
    Failed(String),
}

/// Attempt to automatically fix the issue described by `check`.
///
/// Safe fixes only: creates missing files/directories and re-initialises the
/// database.  Fixes that require user interaction (installing binaries, setting
/// env vars) return `FixOutcome::NotApplicable` with a hint already embedded in
/// the `CheckResult`.
pub fn auto_fix(check: &CheckResult, project_root: &Path) -> GroveResult<FixOutcome> {
    if check.status == CheckStatus::Pass {
        return Ok(FixOutcome::NotApplicable);
    }

    match check.name {
        "config_valid" => fix_config(project_root),
        "db_accessible" => fix_db(project_root),
        "schema_version_current" => fix_schema(project_root),
        _ => Ok(FixOutcome::NotApplicable),
    }
}

/// Apply every safe automated fix for all failing checks in one pass.
///
/// Returns a list of `(check_name, FixOutcome)` pairs for callers that want
/// a summary.
pub fn apply_all_fixes(
    checks: &[CheckResult],
    project_root: &Path,
) -> GroveResult<Vec<(&'static str, FixOutcome)>> {
    let mut results = Vec::new();
    for check in checks {
        if check.status != CheckStatus::Pass {
            let outcome = auto_fix(check, project_root)?;
            results.push((check.name, outcome));
        }
    }
    Ok(results)
}

// ── Internal fix implementations ────────────────────────────────────────────

fn fix_config(project_root: &Path) -> GroveResult<FixOutcome> {
    match GroveConfig::write_default(project_root) {
        Ok(p) => Ok(FixOutcome::Applied(format!(
            "Created default config at {}",
            p.display()
        ))),
        Err(e) => Ok(FixOutcome::Failed(format!(
            "Failed to write default config: {e}"
        ))),
    }
}

fn fix_db(project_root: &Path) -> GroveResult<FixOutcome> {
    // Ensure all standard .grove/ subdirectories exist first.
    for dir in [
        config::logs_dir(project_root),
        config::reports_dir(project_root),
        config::checkpoints_dir(project_root),
        config::worktrees_dir(project_root),
    ] {
        fs::create_dir_all(&dir)?;
    }

    match db::initialize(project_root) {
        Ok(result) => Ok(FixOutcome::Applied(format!(
            "Initialised database at {} (schema v{})",
            result.db_path.display(),
            result.schema_version
        ))),
        Err(e) => Ok(FixOutcome::Failed(format!(
            "Failed to initialise database: {e}"
        ))),
    }
}

fn fix_schema(project_root: &Path) -> GroveResult<FixOutcome> {
    // Re-running initialize is idempotent for the base schema and brings a
    // missing DB up to the minimum version.  Applying higher-version migrations
    // (0002, 0003) requires the migrations directory to be present on disk.
    match db::initialize(project_root) {
        Ok(result) => Ok(FixOutcome::Applied(format!(
            "Re-initialised schema; version now {}",
            result.schema_version
        ))),
        Err(e) => Ok(FixOutcome::Failed(format!(
            "Failed to apply schema fix: {e}"
        ))),
    }
}
