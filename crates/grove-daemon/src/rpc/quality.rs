use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct LintParams {
    #[serde(default)]
    fix: bool,
    #[serde(default)]
    model: Option<String>,
}

pub async fn run_lint(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let LintParams { fix, model } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || {
        grove_core::facade::run_lint(&root, fix, model.as_deref())
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(out)
}

#[derive(Deserialize)]
struct CiParams {
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    wait: bool,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default)]
    fix: bool,
    #[serde(default)]
    model: Option<String>,
}

pub async fn run_ci(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: CiParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || {
        grove_core::facade::run_ci(
            &root,
            p.branch.as_deref(),
            p.wait,
            p.timeout,
            p.fix,
            p.model.as_deref(),
        )
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(out)
}
