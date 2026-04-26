//! Route coverage: every advertised RPC method must dispatch to a handler
//! (i.e. must NOT return -32601 / method not found).
//!
//! Handlers may still legitimately fail at runtime (missing project state,
//! invalid params, etc.) — what we assert here is that dispatch *recognised*
//! the method name. Any failure with `code != -32601` proves routing works.
//!
//! When you add a new RPC to `dispatch()`, append the method name below.

use grove_daemon::config::DaemonConfig;
use grove_daemon::queue_drain::DrainSignal;
use grove_daemon::rpc::envelope::{RpcRequest, RpcResponse};
use grove_daemon::rpc::{DispatchCtx, dispatch};
use serde_json::{Value, json};

/// Method-not-found per JSON-RPC 2.0.
const METHOD_NOT_FOUND: i32 = -32601;

/// Every RPC method registered in `crates/grove-daemon/src/rpc/mod.rs`.
/// Keep this list in lockstep with the `dispatch` match arms.
const ROUTES: &[&str] = &[
    "grove.health",
    // Runs
    "grove.list_runs",
    "grove.get_run",
    "grove.abort_run",
    "grove.resume_run",
    "grove.start_run",
    "grove.drain_queue",
    "grove.retry_publish_run",
    // Tasks
    "grove.list_tasks",
    "grove.queue_task",
    "grove.cancel_task",
    // Workspace
    "grove.get_workspace",
    "grove.set_workspace_name",
    "grove.archive_workspace",
    "grove.delete_workspace",
    // Projects
    "grove.list_projects",
    "grove.get_project",
    "grove.set_project_name",
    "grove.set_project_settings",
    "grove.archive_project",
    "grove.delete_project",
    // Conversations
    "grove.list_conversations",
    "grove.get_conversation",
    "grove.archive_conversation",
    "grove.delete_conversation",
    "grove.rebase_conversation",
    "grove.merge_conversation",
    // Issues
    "grove.list_issues",
    "grove.get_issue",
    "grove.create_issue",
    "grove.close_issue",
    "grove.search_issues",
    "grove.sync_issues",
    "grove.update_issue",
    "grove.comment_issue",
    "grove.assign_issue",
    "grove.move_issue",
    "grove.reopen_issue",
    "grove.activity_issue",
    "grove.push_issue",
    "grove.issue_ready",
    // Logs / reports
    "grove.get_logs",
    "grove.get_report",
    "grove.get_plan",
    "grove.get_subtasks",
    "grove.get_sessions",
    // Auth / LLM
    "grove.list_providers",
    "grove.set_api_key",
    "grove.remove_api_key",
    "grove.list_models",
    "grove.select_llm",
    // Connect
    "grove.connect_status",
    "grove.connect_provider",
    "grove.disconnect_provider",
    // Quality
    "grove.run_lint",
    "grove.run_ci",
    // Signals
    "grove.send_signal",
    "grove.check_signals",
    "grove.list_signals",
    // Hooks
    "grove.run_hook",
    // Worktrees
    "grove.list_worktrees",
    "grove.clean_worktrees",
    "grove.delete_worktree",
    "grove.delete_all_worktrees",
    // Maintenance
    "grove.run_cleanup",
    "grove.run_gc",
    // Locks / merge queue
    "grove.list_ownership_locks",
    "grove.list_merge_queue",
];

fn make_ctx() -> DispatchCtx {
    let tmp = tempfile::tempdir().expect("tempdir");
    let cfg = DaemonConfig::from_project_root(tmp.path()).expect("daemon config");
    // Leak the tempdir so paths stay valid for the test's lifetime;
    // OS will reclaim on process exit.
    std::mem::forget(tmp);
    DispatchCtx::new(
        cfg,
        DrainSignal::new(),
        grove_daemon::session_host::build_registry(900, 8),
    )
}

#[tokio::test]
async fn every_route_dispatches() {
    let ctx = make_ctx();
    let mut unrouted: Vec<&str> = Vec::new();
    for method in ROUTES {
        let req = RpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: (*method).to_string(),
            params: Value::Null,
        };
        let resp: RpcResponse = dispatch(&ctx, req).await;
        if let Some(err) = resp.error {
            if err.code == METHOD_NOT_FOUND {
                unrouted.push(*method);
            }
        }
    }
    assert!(
        unrouted.is_empty(),
        "the following RPC methods are not routed: {unrouted:?}"
    );
}

#[tokio::test]
async fn unknown_method_still_returns_method_not_found() {
    let ctx = make_ctx();
    let req = RpcRequest {
        jsonrpc: "2.0".to_string(),
        id: Some(json!(1)),
        method: "grove.does_not_exist".to_string(),
        params: Value::Null,
    };
    let resp = dispatch(&ctx, req).await;
    let err = resp.error.expect("expected error");
    assert_eq!(err.code, METHOD_NOT_FOUND);
}
