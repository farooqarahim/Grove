use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct GetLogsParams {
    run_id: String,
    #[serde(default)]
    all: bool,
}

pub async fn get_logs(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let GetLogsParams { run_id, all } =
        serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows =
        tokio::task::spawn_blocking(move || grove_core::facade::get_logs(&root, &run_id, all))
            .await
            .map_err(join_err)?
            .map_err(internal)?;
    to_value(&rows)
}

pub async fn get_report(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let report = tokio::task::spawn_blocking(move || grove_core::facade::get_report(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(report)
}

#[derive(Deserialize)]
struct RunIdOptParams {
    #[serde(default)]
    run_id: Option<String>,
}

pub async fn get_plan(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let RunIdOptParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let out = tokio::task::spawn_blocking(move || {
        grove_core::facade::get_plan(&root, run_id.as_deref())
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(out)
}

pub async fn get_subtasks(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let RunIdOptParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || {
        grove_core::facade::get_subtasks(&root, run_id.as_deref())
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct RunIdParams {
    run_id: String,
}

pub async fn get_sessions(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let RunIdParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows =
        tokio::task::spawn_blocking(move || grove_core::facade::get_sessions(&root, &run_id))
            .await
            .map_err(join_err)?
            .map_err(internal)?;
    to_value(&rows)
}
