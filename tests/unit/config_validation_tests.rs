use grove_core::config::defaults::default_config;

#[test]
fn valid_default_config_passes() {
    let cfg = default_config();
    assert!(cfg.validate().is_ok(), "default config should be valid");
}

#[test]
fn empty_provider_name_returns_error() {
    let mut cfg = default_config();
    cfg.providers.default = String::new();
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("providers.default") || err.contains("provider"),
        "expected provider field in error, got: {err}"
    );
}

#[test]
fn budget_zero_returns_error() {
    let mut cfg = default_config();
    cfg.budgets.default_run_usd = 0.0;
    let err = cfg.validate().unwrap_err().to_string();
    assert!(
        err.contains("budget"),
        "expected budget in error, got: {err}"
    );
}

#[test]
fn negative_budget_returns_error() {
    let mut cfg = default_config();
    cfg.budgets.default_run_usd = -5.0;
    assert!(cfg.validate().is_err());
}

#[test]
fn max_agents_zero_returns_error() {
    let mut cfg = default_config();
    cfg.runtime.max_agents = 0;
    assert!(cfg.validate().is_err());
}

#[test]
fn max_agents_above_32_returns_error() {
    let mut cfg = default_config();
    cfg.runtime.max_agents = 33;
    assert!(cfg.validate().is_err());
}

#[test]
fn max_agents_boundary_32_is_valid() {
    let mut cfg = default_config();
    cfg.runtime.max_agents = 32;
    assert!(cfg.validate().is_ok());
}

#[test]
fn max_agents_boundary_1_is_valid() {
    let mut cfg = default_config();
    cfg.runtime.max_agents = 1;
    assert!(cfg.validate().is_ok());
}

#[test]
fn max_concurrent_runs_1_is_valid() {
    let mut cfg = default_config();
    cfg.runtime.max_concurrent_runs = 1;
    assert!(cfg.validate().is_ok());
}

#[test]
fn max_concurrent_runs_10_is_valid() {
    let mut cfg = default_config();
    cfg.runtime.max_concurrent_runs = 10;
    assert!(cfg.validate().is_ok());
}
