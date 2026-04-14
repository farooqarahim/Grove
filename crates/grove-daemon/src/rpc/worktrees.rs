use super::envelope::RpcError;
use super::{DispatchCtx, internal, invalid_params, join_err, to_value};
use serde::Deserialize;
use serde_json::Value;

pub async fn list_worktrees(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || grove_core::facade::list_worktrees(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

pub async fn clean_worktrees(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || grove_core::facade::clean_worktrees(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(out)
}

#[derive(Deserialize)]
struct IdParams {
    id: String,
}

pub async fn delete_worktree(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::delete_worktree(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn delete_all_worktrees(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || grove_core::facade::delete_all_worktrees(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(out)
}
