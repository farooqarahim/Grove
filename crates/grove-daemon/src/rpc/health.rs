use super::envelope::RpcError;
use super::DispatchCtx;
use serde_json::{json, Value};
use std::time::Instant;

pub async fn handle(ctx: &DispatchCtx, _params: Value) -> Result<Value, RpcError> {
    let uptime_ms = Instant::now()
        .saturating_duration_since(ctx.started_at)
        .as_millis() as u64;
    Ok(json!({
        "status": "ok",
        "pid": std::process::id(),
        "uptime_ms": uptime_ms,
        "project_root": ctx.cfg.project_root,
    }))
}
