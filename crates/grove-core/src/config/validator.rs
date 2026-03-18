use crate::errors::{GroveError, GroveResult};

use super::GroveConfig;

/// Validate a `GroveConfig` and return a structured error with field name and
/// a human-readable fix hint on failure.
pub fn validate(cfg: &GroveConfig) -> GroveResult<()> {
    if cfg.runtime.max_agents == 0 {
        return Err(field_err(
            "runtime.max_agents",
            "must be >= 1",
            "Set runtime.max_agents to a value between 1 and 32.",
        ));
    }
    if cfg.runtime.max_agents > 32 {
        return Err(field_err(
            "runtime.max_agents",
            "must be <= 32",
            "Reduce runtime.max_agents to 32 or fewer.",
        ));
    }
    if cfg.budgets.default_run_usd <= 0.0 {
        return Err(field_err(
            "budgets.default_run_usd",
            "must be > 0",
            "Set budgets.default_run_usd to a positive value, e.g. 1.0.",
        ));
    }
    if cfg.budgets.warning_threshold_percent > 100 {
        return Err(field_err(
            "budgets.warning_threshold_percent",
            "must be 0–100",
            "Set budgets.warning_threshold_percent to a value between 0 and 100.",
        ));
    }
    if cfg.budgets.hard_stop_percent > 100 {
        return Err(field_err(
            "budgets.hard_stop_percent",
            "must be 0–100",
            "Set budgets.hard_stop_percent to a value between 0 and 100.",
        ));
    }
    if cfg.budgets.warning_threshold_percent >= cfg.budgets.hard_stop_percent {
        return Err(field_err(
            "budgets.warning_threshold_percent",
            "must be less than hard_stop_percent",
            "Ensure warning_threshold_percent < hard_stop_percent.",
        ));
    }

    if cfg.providers.default.trim().is_empty() {
        return Err(field_err(
            "providers.default",
            "must not be empty",
            "Set providers.default to a valid provider ID (e.g. 'claude_code').",
        ));
    }
    // Provider-enabled check is validated at run-start time, not at config load.
    // This allows the app to start and trackers to connect even when the default
    // coding agent isn't configured yet.
    if cfg.runtime.max_concurrent_runs == 0 {
        return Err(field_err(
            "runtime.max_concurrent_runs",
            "must be >= 1",
            "Set runtime.max_concurrent_runs to a value between 1 and 10.",
        ));
    }
    if cfg.runtime.max_concurrent_runs > 10 {
        return Err(field_err(
            "runtime.max_concurrent_runs",
            "must be <= 10",
            "Reduce runtime.max_concurrent_runs to 10 or fewer.",
        ));
    }
    if cfg.runtime.max_run_minutes == 0 {
        return Err(field_err(
            "runtime.max_run_minutes",
            "must be >= 1",
            "Set runtime.max_run_minutes to a positive value.",
        ));
    }
    if cfg.project.name.trim().is_empty() {
        return Err(field_err(
            "project.name",
            "must not be empty",
            "Set project.name to a non-empty string.",
        ));
    }
    if cfg.project.default_branch.trim().is_empty() {
        return Err(field_err(
            "project.default_branch",
            "must not be empty",
            "Set project.default_branch, e.g. 'main' or 'master'.",
        ));
    }

    validate_branch_prefix(&cfg.worktree.branch_prefix)?;

    Ok(())
}

/// Validate `branch_prefix` using `git check-ref-format` for authoritative
/// git ref rules. Falls back to basic checks when git is unavailable.
fn validate_branch_prefix(prefix: &str) -> GroveResult<()> {
    if prefix.is_empty() {
        return Err(field_err(
            "worktree.branch_prefix",
            "must not be empty",
            "Set worktree.branch_prefix to a valid git ref component, e.g. 'grove'.",
        ));
    }
    if prefix.contains(char::is_whitespace) {
        return Err(field_err(
            "worktree.branch_prefix",
            "must not contain whitespace",
            "Remove spaces or tabs from worktree.branch_prefix.",
        ));
    }

    // Use `git check-ref-format --branch <prefix>/sentinel` for authoritative
    // validation of all git ref rules (dots, backslash, @{, etc.).
    let test_ref = format!("{prefix}/sentinel");
    let git_valid = std::process::Command::new("git")
        .args(["check-ref-format", "--branch", &test_ref])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(true); // git unavailable — skip, non-fatal

    if !git_valid {
        return Err(field_err(
            "worktree.branch_prefix",
            &format!("'{prefix}' is not a valid git ref component"),
            "Use a simple identifier like 'grove', 'ai', or 'bot'. \
             Avoid leading/trailing dots, '.lock' suffix, '@{', backslash, or control chars.",
        ));
    }
    Ok(())
}

fn field_err(field: &str, problem: &str, hint: &str) -> GroveError {
    GroveError::Config(format!("field '{field}': {problem}. Hint: {hint}"))
}
