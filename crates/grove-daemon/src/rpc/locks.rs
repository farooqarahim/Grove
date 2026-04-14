use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct OwnershipParams {
    #[serde(default)]
    run_id: Option<String>,
}

pub async fn list_ownership_locks(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let OwnershipParams { run_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || {
        grove_core::facade::list_ownership_locks(&root, run_id.as_deref())
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct MergeQueueParams {
    conversation_id: String,
}

pub async fn list_merge_queue(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let MergeQueueParams { conversation_id } =
        serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || {
        grove_core::facade::list_merge_queue(&root, &conversation_id)
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    to_value(&rows)
}
