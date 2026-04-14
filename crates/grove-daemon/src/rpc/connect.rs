use super::envelope::RpcError;
use super::{internal, invalid_params, join_err, to_value, DispatchCtx};
use serde::Deserialize;
use serde_json::Value;

pub async fn connect_status(_ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let rows = tokio::task::spawn_blocking(grove_core::facade::connect_status)
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    to_value(&rows)
}

#[derive(Deserialize)]
struct ConnectParams {
    provider: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    site: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

pub async fn connect_provider(_ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let p: ConnectParams = serde_json::from_value(params).map_err(invalid_params)?;
    tokio::task::spawn_blocking(move || {
        grove_core::facade::connect_provider(
            &p.provider,
            p.token.as_deref(),
            p.site.as_deref(),
            p.email.as_deref(),
        )
    })
    .await
    .map_err(join_err)?
    .map_err(internal)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct ProviderParams {
    provider: String,
}

pub async fn disconnect_provider(_ctx: &DispatchCtx, params: Value) -> Result<Value, RpcError> {
    let ProviderParams { provider } = serde_json::from_value(params).map_err(invalid_params)?;
    tokio::task::spawn_blocking(move || grove_core::facade::disconnect_provider(&provider))
        .await
        .map_err(join_err)?
        .map_err(internal)?;
    Ok(Value::Null)
}
