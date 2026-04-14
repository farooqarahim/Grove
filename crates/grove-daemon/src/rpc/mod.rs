pub mod envelope;
pub mod health;

use crate::config::DaemonConfig;
use envelope::{RpcError, RpcRequest, RpcResponse};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct DispatchCtx {
    pub cfg: Arc<DaemonConfig>,
    pub started_at: Instant,
}

impl DispatchCtx {
    pub fn new(cfg: DaemonConfig) -> Self {
        Self {
            cfg: Arc::new(cfg),
            started_at: Instant::now(),
        }
    }
}

pub async fn dispatch(ctx: &DispatchCtx, req: RpcRequest) -> RpcResponse {
    if req.jsonrpc != "2.0" {
        return RpcResponse::err(
            req.id,
            RpcError::invalid_request("jsonrpc must be \"2.0\""),
        );
    }
    let id = req.id.clone();
    let result = match req.method.as_str() {
        "grove.health" => health::handle(ctx, req.params).await,
        other => Err(RpcError::method_not_found(other)),
    };
    match result {
        Ok(v) => RpcResponse::ok(id, v),
        Err(e) => RpcResponse::err(id, e),
    }
}
