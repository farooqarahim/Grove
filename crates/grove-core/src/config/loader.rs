use std::env;
use std::fs;
use std::path::Path;

use crate::errors::GroveResult;

use super::{DEFAULT_CONFIG_YAML, GroveConfig, defaults, paths, validator};

/// Load config for `project_root`.
///
/// Resolution order (later steps win):
/// 1. Built-in defaults via `defaults::default_config()`
/// 2. `.grove/grove.yaml` on disk (if present)
/// 3. `GROVE_*` environment variable overrides
///
/// The resolved config is validated before being returned.
pub fn load_config(project_root: &Path) -> GroveResult<GroveConfig> {
    let config_path = paths::config_path(project_root);

    let mut cfg = if config_path.exists() {
        let yaml = fs::read_to_string(&config_path)?;
        defaults::from_yaml(&yaml)?
    } else {
        defaults::default_config()
    };

    apply_env_overrides(&mut cfg);
    validator::validate(&cfg)?;
    Ok(cfg)
}

/// Load config or write the default file if absent, then return the config.
pub fn load_or_create(project_root: &Path) -> GroveResult<GroveConfig> {
    let config_path = paths::config_path(project_root);
    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, DEFAULT_CONFIG_YAML)?;
    }
    load_config(project_root)
}

/// Serialize `cfg` back to YAML and write it to `.grove/grove.yaml` under
/// `project_root`. Creates parent directories as needed.
pub fn save_config(project_root: &Path, cfg: &GroveConfig) -> GroveResult<()> {
    let config_path = paths::config_path(project_root);
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let yaml = serde_yaml::to_string(cfg).map_err(|e| {
        crate::errors::GroveError::Config(format!("failed to serialize config: {e}"))
    })?;
    fs::write(&config_path, yaml)?;
    Ok(())
}

/// Apply `GROVE_*` environment variable overrides to an already-parsed config.
///
/// Supported variables:
/// - `GROVE_PROVIDER`         → `providers.default`
/// - `GROVE_BUDGET_USD`       → `budgets.default_run_usd`
/// - `GROVE_MAX_AGENTS`       → `runtime.max_agents`
/// - `GROVE_LOG_LEVEL`        → `runtime.log_level`
/// - `GROVE_DEFAULT_BRANCH`   → `project.default_branch`
fn apply_env_overrides(cfg: &mut GroveConfig) {
    if let Ok(v) = env::var("GROVE_PROVIDER") {
        if !v.is_empty() {
            cfg.providers.default = v;
        }
    }
    if let Ok(v) = env::var("GROVE_BUDGET_USD") {
        if let Ok(n) = v.parse::<f64>() {
            cfg.budgets.default_run_usd = n;
        }
    }
    if let Ok(v) = env::var("GROVE_MAX_AGENTS") {
        if let Ok(n) = v.parse::<u16>() {
            cfg.runtime.max_agents = n;
        }
    }
    if let Ok(v) = env::var("GROVE_MAX_CONCURRENT_RUNS") {
        if let Ok(n) = v.parse::<u16>() {
            cfg.runtime.max_concurrent_runs = n;
        }
    }
    if let Ok(v) = env::var("GROVE_LOG_LEVEL") {
        if !v.is_empty() {
            cfg.runtime.log_level = v;
        }
    }
    if let Ok(v) = env::var("GROVE_DEFAULT_BRANCH") {
        if !v.is_empty() {
            cfg.project.default_branch = v;
        }
    }
}
