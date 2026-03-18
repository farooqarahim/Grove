use std::path::Path;
use std::process::Command;

use crate::config::{GroveConfig, config_path, db_path};
use crate::db::{self, repositories::meta_repo};

/// Minimum schema version that a healthy Grove installation must have.
const MIN_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl CheckStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckStatus::Pass => "pass",
            CheckStatus::Warn => "warn",
            CheckStatus::Fail => "fail",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: &'static str,
    pub status: CheckStatus,
    pub message: String,
    pub fix_hint: Option<String>,
}

impl CheckResult {
    pub fn pass(name: &'static str, message: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Pass,
            message: message.into(),
            fix_hint: None,
        }
    }

    pub fn warn(name: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Warn,
            message: message.into(),
            fix_hint: Some(hint.into()),
        }
    }

    pub fn fail(name: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            message: message.into(),
            fix_hint: Some(hint.into()),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.status != CheckStatus::Fail
    }
}

/// Check: `git` binary is available on PATH.
pub fn check_git_available() -> CheckResult {
    let ok = Command::new("git")
        .arg("--version")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        CheckResult::pass("git_available", "git is available on PATH")
    } else {
        CheckResult::fail(
            "git_available",
            "git binary not found on PATH",
            "Install git: https://git-scm.com/downloads",
        )
    }
}

/// Check: `.grove/grove.yaml` exists and passes validation.
pub fn check_config_valid(project_root: &Path) -> CheckResult {
    let path = config_path(project_root);
    if !path.exists() {
        return CheckResult::fail(
            "config_valid",
            format!("config file not found: {}", path.display()),
            "Run `grove init` or `grove doctor --fix` to create a default config.",
        );
    }
    match GroveConfig::load_or_create(project_root) {
        Ok(cfg) => match cfg.validate() {
            Ok(()) => CheckResult::pass("config_valid", "config is present and valid"),
            Err(e) => CheckResult::fail(
                "config_valid",
                format!("config validation error: {e}"),
                "Edit .grove/grove.yaml and correct the reported field.",
            ),
        },
        Err(e) => CheckResult::fail(
            "config_valid",
            format!("failed to load config: {e}"),
            "Check .grove/grove.yaml for YAML syntax errors.",
        ),
    }
}

/// Check: DB file exists and a connection can be opened.
pub fn check_db_accessible(project_root: &Path) -> CheckResult {
    let path = db_path(project_root);
    if !path.exists() {
        return CheckResult::fail(
            "db_accessible",
            format!("DB not found: {}", path.display()),
            "Run `grove init` or `grove doctor --fix` to initialise the database.",
        );
    }
    let handle = db::DbHandle::new(project_root);
    match handle.connect() {
        Ok(_) => CheckResult::pass("db_accessible", "database is accessible"),
        Err(e) => CheckResult::fail(
            "db_accessible",
            format!("failed to open database: {e}"),
            "Delete .grove/grove.db and re-run `grove init`.",
        ),
    }
}

/// Check: schema_version in the DB is at least `MIN_SCHEMA_VERSION`.
pub fn check_schema_version_current(project_root: &Path) -> CheckResult {
    let handle = db::DbHandle::new(project_root);
    let conn = match handle.connect() {
        Ok(c) => c,
        Err(_) => {
            return CheckResult::fail(
                "schema_version_current",
                "cannot open DB to check schema version",
                "Fix the db_accessible check first.",
            );
        }
    };
    match meta_repo::get_schema_version(&conn) {
        Ok(v) if v >= MIN_SCHEMA_VERSION => CheckResult::pass(
            "schema_version_current",
            format!("schema version is {v} (minimum {MIN_SCHEMA_VERSION})"),
        ),
        Ok(v) => CheckResult::fail(
            "schema_version_current",
            format!("schema version {v} is below minimum {MIN_SCHEMA_VERSION}"),
            "Run `grove doctor --fix` to apply pending migrations.",
        ),
        Err(e) => CheckResult::fail(
            "schema_version_current",
            format!("could not read schema version: {e}"),
            "Run `grove doctor --fix` to re-initialise the database.",
        ),
    }
}

/// Check: the configured provider binary is reachable.
/// For the `mock` provider this always passes; for `claude_code` it probes the command.
pub fn check_provider_binary_present(project_root: &Path) -> CheckResult {
    let cfg = match GroveConfig::load_or_create(project_root) {
        Ok(c) => c,
        Err(_) => {
            return CheckResult::warn(
                "provider_binary_present",
                "could not load config; skipping provider binary check",
                "Fix the config_valid check first.",
            );
        }
    };

    if cfg.providers.default != "claude_code" {
        return CheckResult::pass(
            "provider_binary_present",
            format!(
                "provider '{}' does not require an external binary",
                cfg.providers.default
            ),
        );
    }

    let cmd = &cfg.providers.claude_code.command;
    let ok = Command::new(cmd)
        .arg("--version")
        .env("PATH", crate::capability::shell_path())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if ok {
        CheckResult::pass(
            "provider_binary_present",
            format!("provider binary '{}' is available", cmd),
        )
    } else {
        CheckResult::fail(
            "provider_binary_present",
            format!("provider binary '{}' not found or returned an error", cmd),
            "Install the claude CLI and ensure it is on your PATH.",
        )
    }
}

/// Check: `ANTHROPIC_API_KEY` env var is set when `claude_code` is the active provider.
pub fn check_api_key_set(project_root: &Path) -> CheckResult {
    let cfg = match GroveConfig::load_or_create(project_root) {
        Ok(c) => c,
        Err(_) => {
            return CheckResult::warn(
                "api_key_set",
                "could not load config; skipping API key check",
                "Fix the config_valid check first.",
            );
        }
    };

    if cfg.providers.default != "claude_code" {
        return CheckResult::pass(
            "api_key_set",
            format!(
                "provider '{}' does not require ANTHROPIC_API_KEY",
                cfg.providers.default
            ),
        );
    }

    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(v) if !v.trim().is_empty() => {
            CheckResult::pass("api_key_set", "ANTHROPIC_API_KEY is set")
        }
        _ => CheckResult::warn(
            "api_key_set",
            "ANTHROPIC_API_KEY is not set; claude CLI will use its own stored credentials",
            "Set ANTHROPIC_API_KEY if you prefer explicit key auth.",
        ),
    }
}

/// Run all checks in dependency order and return the full list of results.
pub fn run_all(project_root: &Path) -> Vec<CheckResult> {
    vec![
        check_git_available(),
        check_config_valid(project_root),
        check_db_accessible(project_root),
        check_schema_version_current(project_root),
        check_provider_binary_present(project_root),
        check_api_key_set(project_root),
    ]
}
