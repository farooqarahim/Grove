use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct RunHookParams {
    event: String,
    #[serde(default)]
    agent_type: Option<String>,
    #[serde(default)]
    run_id: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
}

pub async fn run_hook(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: RunHookParams = serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || {
        grove_core::facade::run_hook(
            &root,
            &p.event,
            p.agent_type.as_deref(),
            p.run_id.as_deref(),
            p.session_id.as_deref(),
            p.tool.as_deref(),
            p.file_path.as_deref(),
        )
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(Value::Null)
}
