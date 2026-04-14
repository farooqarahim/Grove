use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

pub async fn list_tasks(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let root = ctx.cfg.project_root.clone();
    let rows = tokio::task::spawn_blocking(move || grove_core::facade::list_tasks(&root))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct QueueTaskParams {
    objective: String,
    #[serde(default)]
    priority: i64,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    conversation_id: Option<String>,
    #[serde(default)]
    pipeline: Option<String>,
    #[serde(default)]
    permission_mode: Option<String>,
}

pub async fn queue_task(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: QueueTaskParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    let task = tokio::task::spawn_blocking(move || {
        grove_core::facade::queue_task(
            &root,
            &p.objective,
            p.priority,
            p.model.as_deref(),
            p.conversation_id.as_deref(),
            p.pipeline.as_deref(),
            p.permission_mode.as_deref(),
        )
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    to_value(&task)
}

#[derive(Deserialize)]
struct CancelTaskParams {
    task_id: String,
}

pub async fn cancel_task(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let CancelTaskParams { task_id } = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || grove_core::facade::cancel_task(&root, &task_id))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}
