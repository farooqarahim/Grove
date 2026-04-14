use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct CleanupParams {
    #[serde(default)]
    project: bool,
    #[serde(default)]
    conversation: bool,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    yes: bool,
    #[serde(default)]
    force: bool,
}

pub async fn run_cleanup(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: CleanupParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || {
        grove_core::facade::run_cleanup(&root, p.project, p.conversation, p.dry_run, p.yes, p.force)
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(out)
}

#[derive(Deserialize)]
struct GcParams {
    #[serde(default)]
    dry_run: bool,
}

pub async fn run_gc(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let GcParams { dry_run } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || {
        grove_core::facade::run_gc(&root, &root, dry_run)
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(out)
}
