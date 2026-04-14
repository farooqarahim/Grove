use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct SendParams {
    run_id: String,
    from: String,
    to: String,
    signal_type: String,
    #[serde(default)]
    payload: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
}

pub async fn send_signal(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: SendParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || {
        grove_core::facade::send_signal(
            &root,
            &p.run_id,
            &p.from,
            &p.to,
            &p.signal_type,
            p.payload.as_deref(),
            p.priority,
        )
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct CheckParams {
    run_id: String,
    agent: String,
}

pub async fn check_signals(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let CheckParams { run_id, agent } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || {
        grove_core::facade::check_signals(&root, &run_id, &agent)
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct ListParams {
    run_id: String,
}

pub async fn list_signals(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let ListParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows =
        tokio::task::spawn_blocking(move || grove_core::facade::list_signals(&root, &run_id))
            .await
            .map_err(join_err)?
            .map_err(internal)?;
    to_value(&rows)
}
