use grove_daemon::config::DaemonConfig;
use grove_daemon::queue_drain::DrainSignal;
use grove_daemon::rpc::{DispatchCtx, dispatch};
use serde_json::json;
use std::time::Duration;
use tempfile::tempdir;

fn ctx() -> DispatchCtx {
    let tmp = tempdir().unwrap();
    let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
    DispatchCtx::new(
        cfg,
        DrainSignal::new(),
        grove_daemon::session_host::build_registry(900, 8),
    )
}

fn ctx_with_signal() -> (DispatchCtx, DrainSignal) {
    let tmp = tempdir().unwrap();
    let path = tmp.keep();
    // Initialize the DB schema so queue_task can actually insert a row.
    grove_core::db::initialize(&path).expect("db init");
    let cfg = DaemonConfig::from_project_root(&path).unwrap();
    let signal = DrainSignal::new();
    (
        DispatchCtx::new(
            cfg,
            signal.clone(),
            grove_daemon::session_host::build_registry(900, 8),
        ),
        signal,
    )
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
async fn queue_task_notifies_drain_signal() {
    // Queuing a task via RPC must wake the drain loop. We assert this by
    // holding a sibling clone of the DrainSignal and observing that its
    // wait() resolves promptly after dispatch returns (or fails — the notify
    // happens *before* the blocking call, so even a failed queue still wakes
    // the loop, which is the safer behavior).
    let (c, sig) = ctx_with_signal();
    let req: grove_daemon::rpc::envelope::RpcRequest = serde_json::from_str(
        r#"{"jsonrpc":"2.0","method":"grove.queue_task","params":{"objective":"test-objective"},"id":3}"#,
    )
    .unwrap();
    let _ = dispatch(&c, req).await;
    // Notify permits are buffered — if notify() was called, wait() resolves
    // immediately. If it wasn't, this times out.
    tokio::time::timeout(Duration::from_millis(100), sig.wait())
        .await
        .expect("queue_task should have notified the drain signal");
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
