use super::envelope::RpcError;
use super::{DispatchCtx, internal, invalid_params, join_err, to_value};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct ListRunsParams {
    #[serde(default = "default_limit")]
    limit: i64,
}
fn default_limit() -> i64 {
    50
}

pub async fn list_runs(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let ListRunsParams { limit } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || grove_core::facade::list_runs(&root, limit))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct RunIdParams {
    run_id: String,
}

pub async fn get_run(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let RunIdParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let row = tokio::task::spawn_blocking(move || grove_core::facade::get_run(&root, &run_id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&row)
}

pub async fn abort_run(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let RunIdParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::abort_run(&root, &run_id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn resume_run(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let RunIdParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::resume_run(&root, &run_id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn retry_publish_run(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let RunIdParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::retry_publish_run(&root, &run_id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct StartRunParams {
    objective: String,
    #[serde(default)]
    pipeline: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    permission_mode: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
}

pub async fn start_run(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: StartRunParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let input = grove_core::facade::StartRunInput {
        objective: p.objective,
        pipeline: p.pipeline,
        model: p.model,
        permission_mode: p.permission_mode,
        conversation_id: p.conversation_id,
    };
    let out = tokio::task::spawn_blocking(move || grove_core::facade::start_run(&root, input))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    ctx.drain_signal.notify();
    to_value(&out)
}

pub async fn drain_queue(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    ctx.drain_signal.notify();
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::drain_queue(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}
