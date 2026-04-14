use grove_daemon::config::DaemonConfig;
use grove_daemon::rpc::{DispatchCtx, dispatch};
use serde_json::json;
use tempfile::tempdir;

fn ctx() -> DispatchCtx {
    let tmp = tempdir().unwrap();
    let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
    DispatchCtx::new(cfg)
}

#[tokio::test]
async fn health_returns_ok() {
    let c = ctx();
    let req: grove_daemon::rpc::envelope::RpcRequest =
        serde_json::from_str(r#"{"jsonrpc":"2.0","method":"grove.health","params":{},"id":1}"#)
            .unwrap();
    let resp = dispatch(&c, req).await;
    assert_eq!(resp.error.as_ref().map(|e| e.code), None);
    let result = resp.result.unwrap();
    assert_eq!(result["status"], "ok");
    assert!(result["pid"].is_u64());
    assert!(result["uptime_ms"].is_u64());
}

#[tokio::test]
async fn unknown_method_returns_minus_32601() {
    let c = ctx();
    let req: grove_daemon::rpc::envelope::RpcRequest =
        serde_json::from_str(r#"{"jsonrpc":"2.0","method":"grove.nope","params":{},"id":2}"#)
            .unwrap();
    let resp = dispatch(&c, req).await;
    assert_eq!(resp.error.as_ref().unwrap().code, -32601);
    assert_eq!(resp.id, Some(json!(2)));
}
