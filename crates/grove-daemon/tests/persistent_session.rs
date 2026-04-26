//! End-to-end integration test: two back-to-back provider invocations with
//! the same `conversation_id` reuse a single persistent `claude` subprocess
//! when the provider is constructed via the daemon's `build_registry` + the
//! orchestrator's `build_provider` (the same path the queue drain takes).
//!
//! This is the seam-level e2e for B1's value proposition. We don't drive the
//! full `execute_objective` orchestrator pipeline because that would require
//! a fake agent script capable of satisfying every phase (Planner, Builder,
//! Reviewer, Judge) — out of scope for a hermetic test. Instead, we
//! reconstruct the exact sequence: daemon builds registry, orchestrator's
//! `build_provider` wires it into `ClaudeCodeProvider`, two provider calls
//! flow through.
//!
//! Hermetic: `claude` is stubbed by a test script via `GROVE_CLAUDE_BIN`.

use grove_core::config::{GroveConfig, PermissionMode};
use grove_core::orchestrator;
use grove_core::providers::ProviderRequest;
use grove_daemon::session_host::build_registry;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempPath;

fn fake_claude_script() -> TempPath {
    let mut f = tempfile::Builder::new()
        .prefix("fake-claude-e2e-")
        .suffix(fake_claude_script_suffix())
        .tempfile()
        .unwrap();
    writeln!(f, "{}", fake_claude_script_body()).unwrap();
    f.flush().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut p = std::fs::metadata(f.path()).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(f.path(), p).unwrap();
    }
    f.into_temp_path()
}

#[cfg(unix)]
fn fake_claude_script_suffix() -> &'static str {
    ".sh"
}

#[cfg(windows)]
fn fake_claude_script_suffix() -> &'static str {
    ".cmd"
}

#[cfg(unix)]
fn fake_claude_script_body() -> &'static str {
    r#"#!/bin/sh
while IFS= read -r line; do
  printf '%s\n' '{"type":"system","session_id":"E2E","model":"fake"}'
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"e2e-ack"}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","session_id":"E2E","cost_usd":0.0,"is_error":false}'
done
"#
}

#[cfg(windows)]
fn fake_claude_script_body() -> &'static str {
    r#"@echo off
:loop
set /p line=
if errorlevel 1 exit /b 0
echo {"type":"system","session_id":"E2E","model":"fake"}
echo {"type":"assistant","message":{"content":[{"type":"text","text":"e2e-ack"}]}}
echo {"type":"result","subtype":"success","session_id":"E2E","cost_usd":0.0,"is_error":false}
goto loop
"#
}

fn test_config(project_root: &Path) -> GroveConfig {
    let mut cfg = GroveConfig::load_or_create(project_root).expect("grove config");
    cfg.providers.claude_code.command = "missing-claude-for-persistent-session-test".to_string();
    cfg
}

fn make_request(worktree: &str, conv_id: &str) -> ProviderRequest {
    ProviderRequest {
        objective: "two-turn e2e".into(),
        role: "builder".into(),
        worktree_path: worktree.to_string(),
        instructions: "turn prompt".into(),
        model: None,
        allowed_tools: None,
        timeout_override: None,
        provider_session_id: None,
        log_dir: None,
        grove_session_id: None,
        input_handle_callback: None,
        mcp_config_path: None,
        conversation_id: Some(conv_id.to_string()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_calls_same_conversation_reuse_one_persistent_host() {
    let tmp = tempfile::tempdir().unwrap();
    let script = fake_claude_script();

    // SAFETY: test-owned env for the duration of this test. The provider
    // reads GROVE_CLAUDE_BIN once during `build_provider` and caches it in
    // `self.command`, so concurrent tests would not race on the value.
    unsafe {
        std::env::set_var("GROVE_CLAUDE_BIN", script.as_os_str());
    }

    let cfg = test_config(tmp.path());
    let registry = build_registry(900, 8);

    let provider = orchestrator::build_provider(
        &cfg,
        tmp.path(),
        Some("claude_code"),
        Some(PermissionMode::SkipAll),
        Some(Arc::clone(&registry)),
    )
    .expect("build_provider");

    let worktree = tmp.path().to_string_lossy().to_string();
    let req1 = make_request(&worktree, "conv-e2e");
    let req2 = make_request(&worktree, "conv-e2e");

    // `Provider::execute` is sync — run it from a blocking thread so the
    // warm path's `block_in_place(|| handle.block_on(...))` can borrow this
    // multi-thread runtime's handle.
    let p1 = Arc::clone(&provider);
    let out1 = tokio::task::spawn_blocking(move || p1.execute(&req1))
        .await
        .unwrap()
        .expect("first turn");
    assert_eq!(
        out1.provider_session_id.as_deref(),
        Some("E2E"),
        "warm path must surface the fake session id on turn 1"
    );
    assert_eq!(
        registry.len().await,
        1,
        "first turn must register exactly one host"
    );

    let p2 = Arc::clone(&provider);
    let out2 = tokio::task::spawn_blocking(move || p2.execute(&req2))
        .await
        .unwrap()
        .expect("second turn");
    assert_eq!(
        out2.provider_session_id.as_deref(),
        Some("E2E"),
        "warm path must still route turn 2 through the same host"
    );
    assert_eq!(
        registry.len().await,
        1,
        "second turn on the same conversation must reuse the existing host, not add a new one"
    );

    // SAFETY: cleanup owned test env.
    unsafe {
        std::env::remove_var("GROVE_CLAUDE_BIN");
    }
}
