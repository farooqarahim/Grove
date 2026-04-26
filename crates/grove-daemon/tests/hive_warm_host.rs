//! Hive warm-host integration tests.
//!
//! Verifies that hive_loom's worker and orchestrator dispatch keep
//! per-phase / per-graph persistent Claude subprocesses keyed by the
//! conversation ids defined in `worker_dispatch::phase_worker_conversation_id`
//! and `orchestrator_dispatch::orchestrator_conversation_id`.
//!
//! These are seam-level tests: we exercise the provider warm path the same
//! way the hive loop does — by issuing `ProviderRequest`s with the right
//! `conversation_id` — without driving the full graph loop. The full loop
//! is covered by `loop_orchestrator`'s in-tree tests with `MockProvider`.
//!
//! Hermetic: `claude` is stubbed by a shell script via `GROVE_CLAUDE_BIN`.

// Each test holds the env mutex for its full body to serialize
// `GROVE_CLAUDE_BIN` mutation. The std::Mutex guard outliving await
// points is the desired behaviour — there's only one runtime worker
// thread that needs the env value at any given time, and we want strict
// serial execution of these tests, not interleaving via async lock
// passing. Suppress the lint at the file level rather than per test.
#![allow(clippy::await_holding_lock)]

use grove_core::config::{GroveConfig, PermissionMode};
use grove_core::grove_graph::orchestrator_dispatch;
use grove_core::grove_graph::worker_dispatch;
use grove_core::orchestrator;
use grove_core::providers::ProviderRequest;
use grove_daemon::session_host::build_registry;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use tempfile::TempPath;

fn fake_claude_script() -> TempPath {
    let mut f = tempfile::Builder::new()
        .prefix("fake-claude-hive-")
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
  printf '%s\n' '{"type":"system","session_id":"HIVE","model":"fake"}'
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"hive-ack"}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","session_id":"HIVE","cost_usd":0.0,"is_error":false}'
done
"#
}

#[cfg(windows)]
fn fake_claude_script_body() -> &'static str {
    r#"@echo off
:loop
set /p line=
if errorlevel 1 exit /b 0
echo {"type":"system","session_id":"HIVE","model":"fake"}
echo {"type":"assistant","message":{"content":[{"type":"text","text":"hive-ack"}]}}
echo {"type":"result","subtype":"success","session_id":"HIVE","cost_usd":0.0,"is_error":false}
goto loop
"#
}

fn test_config(project_root: &Path) -> GroveConfig {
    let mut cfg = GroveConfig::load_or_create(project_root).expect("grove config");
    cfg.providers.claude_code.command = "missing-claude-for-hive-warm-host-test".to_string();
    cfg
}

fn make_request(worktree: &str, conv_id: &str, role: &str) -> ProviderRequest {
    ProviderRequest {
        objective: "hive turn".into(),
        role: role.into(),
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

/// SAFETY: tests in this file mutate `GROVE_CLAUDE_BIN`. They MUST run
/// serialized so a script-rotation between tests does not race with another
/// in-flight test reading the env. The mutex is `'static` so it survives
/// across all #[tokio::test] invocations in this binary.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_env() -> MutexGuard<'static, ()> {
    env_lock()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

// ── Worker keying ─────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn worker_steps_in_same_phase_reuse_one_host() {
    let _guard = lock_env();
    let tmp = tempfile::tempdir().unwrap();
    let script = fake_claude_script();
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
    let conv = worker_dispatch::phase_worker_conversation_id("graph-1", "phase-1");
    let req1 = make_request(&worktree, &conv, "phase_worker");
    let req2 = make_request(&worktree, &conv, "phase_worker");

    let p1 = Arc::clone(&provider);
    tokio::task::spawn_blocking(move || p1.execute(&req1))
        .await
        .unwrap()
        .expect("first chunk");
    assert_eq!(
        registry.len().await,
        1,
        "first chunk must register exactly one host"
    );

    let p2 = Arc::clone(&provider);
    tokio::task::spawn_blocking(move || p2.execute(&req2))
        .await
        .unwrap()
        .expect("second chunk");
    assert_eq!(
        registry.len().await,
        1,
        "second chunk on the same phase must reuse the existing host"
    );

    unsafe {
        std::env::remove_var("GROVE_CLAUDE_BIN");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workers_in_different_phases_get_distinct_hosts() {
    let _guard = lock_env();
    let tmp = tempfile::tempdir().unwrap();
    let script = fake_claude_script();
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
    let conv1 = worker_dispatch::phase_worker_conversation_id("graph-1", "phase-1");
    let conv2 = worker_dispatch::phase_worker_conversation_id("graph-1", "phase-2");

    let p = Arc::clone(&provider);
    let req1 = make_request(&worktree, &conv1, "phase_worker");
    tokio::task::spawn_blocking(move || p.execute(&req1))
        .await
        .unwrap()
        .expect("phase 1 turn");

    let p = Arc::clone(&provider);
    let req2 = make_request(&worktree, &conv2, "phase_worker");
    tokio::task::spawn_blocking(move || p.execute(&req2))
        .await
        .unwrap()
        .expect("phase 2 turn");

    assert_eq!(
        registry.len().await,
        2,
        "two phases must register two distinct hosts (per-phase context isolation)"
    );

    unsafe {
        std::env::remove_var("GROVE_CLAUDE_BIN");
    }
}

// ── Orchestrator keying ────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrator_decisions_in_same_graph_reuse_one_host() {
    let _guard = lock_env();
    let tmp = tempfile::tempdir().unwrap();
    let script = fake_claude_script();
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
    let conv = orchestrator_dispatch::orchestrator_conversation_id("graph-1");
    let req1 = make_request(&worktree, &conv, "orchestrator");
    let req2 = make_request(&worktree, &conv, "orchestrator");

    let p = Arc::clone(&provider);
    tokio::task::spawn_blocking(move || p.execute(&req1))
        .await
        .unwrap()
        .expect("orchestrator decision 1");

    let p = Arc::clone(&provider);
    tokio::task::spawn_blocking(move || p.execute(&req2))
        .await
        .unwrap()
        .expect("orchestrator decision 2");

    assert_eq!(
        registry.len().await,
        1,
        "two orchestrator decisions on the same graph must reuse one host"
    );

    unsafe {
        std::env::remove_var("GROVE_CLAUDE_BIN");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn orchestrator_and_worker_get_separate_hosts() {
    let _guard = lock_env();
    let tmp = tempfile::tempdir().unwrap();
    let script = fake_claude_script();
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
    let worker_conv = worker_dispatch::phase_worker_conversation_id("graph-1", "phase-1");
    let orch_conv = orchestrator_dispatch::orchestrator_conversation_id("graph-1");

    let p = Arc::clone(&provider);
    let req = make_request(&worktree, &worker_conv, "phase_worker");
    tokio::task::spawn_blocking(move || p.execute(&req))
        .await
        .unwrap()
        .expect("worker turn");

    let p = Arc::clone(&provider);
    let req = make_request(&worktree, &orch_conv, "orchestrator");
    tokio::task::spawn_blocking(move || p.execute(&req))
        .await
        .unwrap()
        .expect("orchestrator turn");

    assert_eq!(
        registry.len().await,
        2,
        "worker and orchestrator must not share a host even within the same graph"
    );

    unsafe {
        std::env::remove_var("GROVE_CLAUDE_BIN");
    }
}

// ── Phase-boundary eviction ────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn evict_warm_session_releases_phase_worker_host() {
    let _guard = lock_env();
    let tmp = tempfile::tempdir().unwrap();
    let script = fake_claude_script();
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
    let conv = worker_dispatch::phase_worker_conversation_id("graph-1", "phase-1");

    let p = Arc::clone(&provider);
    let req = make_request(&worktree, &conv, "phase_worker");
    tokio::task::spawn_blocking(move || p.execute(&req))
        .await
        .unwrap()
        .expect("first turn");
    assert_eq!(registry.len().await, 1, "phase host registered");

    // Simulate phase boundary: hive loop calls
    // `provider.evict_warm_session(conv_id)` after phase validation.
    let p = Arc::clone(&provider);
    let conv_for_evict = conv.clone();
    tokio::task::spawn_blocking(move || p.evict_warm_session(&conv_for_evict))
        .await
        .unwrap();

    assert_eq!(
        registry.len().await,
        0,
        "evict_warm_session must remove the phase host"
    );

    // Next turn on the same conversation should respawn cleanly.
    let p = Arc::clone(&provider);
    let req = make_request(&worktree, &conv, "phase_worker");
    tokio::task::spawn_blocking(move || p.execute(&req))
        .await
        .unwrap()
        .expect("post-evict turn");
    assert_eq!(
        registry.len().await,
        1,
        "post-evict turn must respawn exactly one host"
    );

    unsafe {
        std::env::remove_var("GROVE_CLAUDE_BIN");
    }
}
