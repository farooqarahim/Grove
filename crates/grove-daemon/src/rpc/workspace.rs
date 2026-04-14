use super::envelope::RpcError;
use super::{DispatchCtx, internal, invalid_params, join_err, to_value};
use serde::Deserialize;
use serde_json::Value;

pub async fn get_workspace(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let row = tokio::task::spawn_blocking(move || grove_core::facade::get_workspace(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&row)
}

#[derive(Deserialize)]
struct SetNameParams {
    name: String,
}

pub async fn set_workspace_name(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let SetNameParams { name } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::set_workspace_name(&root, &name))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct IdParams {
    id: String,
}

pub async fn archive_workspace(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::archive_workspace(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn delete_workspace(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::delete_workspace(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}
