use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct ListParams {
    #[serde(default = "default_limit")]
    limit: i64,
}
fn default_limit() -> i64 {
    50
}

pub async fn list_conversations(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let ListParams { limit } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows =
        tokio::task::spawn_blocking(move || grove_core::facade::list_conversations(&root, limit))
            .await
            .map_err(join_err)?
            .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct IdParams {
    id: String,
}

pub async fn get_conversation(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let row = tokio::task::spawn_blocking(move || grove_core::facade::get_conversation(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&row)
}

pub async fn archive_conversation(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::archive_conversation(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn delete_conversation(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::delete_conversation(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn rebase_conversation(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::rebase_conversation(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn merge_conversation(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let IdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::merge_conversation(&root, &id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}
