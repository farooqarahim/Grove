use super::envelope::RpcError;
use super::{DispatchCtx, internal, invalid_params, join_err, to_value};
use serde::Deserialize;
use serde_json::Value;

pub async fn list_providers(_ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let rows = tokio::task::spawn_blocking(grove_core::facade::list_providers)
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct SetApiKeyParams {
    provider: String,
    key: String,
}

pub async fn set_api_key(_ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let SetApiKeyParams { provider, key } =
        serde_json::from_value(params).map_err(invalid_params)?;
    tokio::task::spawn_blocking(move || grove_core::facade::set_api_key(&provider, &key))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct ProviderParams {
    provider: String,
}

pub async fn remove_api_key(_ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let ProviderParams { provider } = serde_json::from_value(params).map_err(invalid_params)?;
    tokio::task::spawn_blocking(move || grove_core::facade::remove_api_key(&provider))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}

pub async fn list_models(_ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let ProviderParams { provider } = serde_json::from_value(params).map_err(invalid_params)?;
    let rows = tokio::task::spawn_blocking(move || grove_core::facade::list_models(&provider))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct SelectParams {
    provider: String,
    #[serde(default)]
    model: Option<String>,
}

pub async fn select_llm(ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let SelectParams { provider, model } =
        serde_json::from_value(params).map_err(invalid_params)?;
    let root = ctx.cfg.project_root.clone();
    tokio::task::spawn_blocking(move || {
        grove_core::facade::select_llm(&root, &provider, model.as_deref())
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(Value::Null)
}
