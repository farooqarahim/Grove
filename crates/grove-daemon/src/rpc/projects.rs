use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

pub async fn list_projects(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || grove_core::facade::list_projects(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

pub async fn get_project(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let row = tokio::task::spawn_blocking(move || grove_core::facade::get_project(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&row)
}

#[derive(Deserialize)]
struct SetNameParams {
    name: String,
}

pub async fn set_project_name(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let SetNameParams { name } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::set_project_name(&root, &name))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct SetSettingsParams {
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    parallel: Option<i64>,
    #[serde(default)]
    pipeline: Option<String>,
    #[serde(default)]
    permission_mode: Option<String>,
}

pub async fn set_project_settings(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: SetSettingsParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || {
        grove_core::facade::set_project_settings(
            &root,
            p.provider.as_deref(),
            p.parallel,
            p.pipeline.as_deref(),
            p.permission_mode.as_deref(),
        )
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct OptIdParams {
    #[serde(default)]
    id: Option<String>,
}

pub async fn archive_project(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let OptIdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::archive_project(&root, id.as_deref()))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn delete_project(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let OptIdParams { id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::delete_project(&root, id.as_deref()))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}
