//! Verifies ClaudeCodeProvider takes the warm path when a SessionHostRegistry
//! is injected AND the request carries a conversation id. Uses a fake claude
//! shell script so no network calls or real Anthropic API are involved.

use grove_core::config::PermissionMode;
use grove_core::providers::claude_code::ClaudeCodeProvider;
use grove_core::providers::session_host::SessionHostRegistry;
use grove_core::providers::session_host::registry::{InMemorySessionHostRegistry, RegistryConfig};
use grove_core::providers::{Provider, ProviderRequest, StreamOutputEvent, StreamSink};
use std::io::Write;
use std::sync::{Arc, Mutex};

struct CaptureSink(Arc<Mutex<Vec<StreamOutputEvent>>>);
impl StreamSink for CaptureSink {
    fn on_event(&self, event: StreamOutputEvent) {
        self.0.lock().unwrap().push(event);
    }
}

fn fake_script() -> tempfile::TempPath {
    let mut f = tempfile::Builder::new()
        .prefix("fake-claude-")
        .suffix(".sh")
        .tempfile()
        .unwrap();
    writeln!(
        f,
        r#"#!/bin/sh
while IFS= read -r line; do
  printf '%s\n' '{{"type":"system","session_id":"WARM","model":"fake"}}'
  printf '%s\n' '{{"type":"assistant","message":{{"content":[{{"type":"text","text":"warm-ack"}}]}}}}'
  printf '%s\n' '{{"type":"result","subtype":"success","session_id":"WARM","cost_usd":0.0,"is_error":false}}'
done
"#
    )
    .unwrap();
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

fn make_request(prompt: &str, conv_id: &str, worktree: &str) -> ProviderRequest {
    ProviderRequest {
        objective: "test objective".into(),
        role: "test".into(),
        worktree_path: worktree.to_string(),
        instructions: prompt.to_string(),
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
async fn warm_path_emits_assistant_text_from_registry() {
    let script = fake_script();
    let tmp = tempfile::tempdir().unwrap();
    let reg: Arc<dyn SessionHostRegistry> =
        Arc::new(InMemorySessionHostRegistry::new(RegistryConfig::default()));
    let provider = ClaudeCodeProvider::new(
        script.to_string_lossy().to_string(),
        60,
        PermissionMode::SkipAll,
        Vec::new(),
        None,
    )
    .with_session_registry(Some(Arc::clone(&reg)));

    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = CaptureSink(Arc::clone(&events));
    let req = make_request("hello", "conv-1", tmp.path().to_str().unwrap());

    // execute_streaming is sync; run it on a blocking thread so the warm path's
    // block_in_place + Handle::current() can use this multi-thread runtime.
    let provider_arc = Arc::new(provider);
    let p_for_task = Arc::clone(&provider_arc);
    let resp = tokio::task::spawn_blocking(move || p_for_task.execute_streaming(&req, &sink))
        .await
        .unwrap()
        .expect("warm turn");

    assert_eq!(resp.provider_session_id.as_deref(), Some("WARM"));
    assert!(
        resp.summary.contains("warm-ack"),
        "summary should contain assistant text, got: {:?}",
        resp.summary
    );
    {
        let evs = events.lock().unwrap();
        assert!(
            evs.iter().any(|e| matches!(
                e,
                StreamOutputEvent::AssistantText { text } if text == "warm-ack"
            )),
            "expected an AssistantText event with 'warm-ack', got: {:#?}",
            *evs
        );
    } // drop MutexGuard before await
    assert_eq!(
        reg.len().await,
        1,
        "registry must retain the host after the first turn"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_turns_reuse_one_host() {
    let script = fake_script();
    let tmp = tempfile::tempdir().unwrap();
    let reg: Arc<dyn SessionHostRegistry> =
        Arc::new(InMemorySessionHostRegistry::new(RegistryConfig::default()));
    let provider = Arc::new(
        ClaudeCodeProvider::new(
            script.to_string_lossy().to_string(),
            60,
            PermissionMode::SkipAll,
            Vec::new(),
            None,
        )
        .with_session_registry(Some(Arc::clone(&reg))),
    );

    let sink_events = Arc::new(Mutex::new(Vec::new()));
    let sink = CaptureSink(Arc::clone(&sink_events));
    let worktree = tmp.path().to_str().unwrap().to_string();

    let p1 = Arc::clone(&provider);
    let req1 = make_request("turn-1", "conv-1", &worktree);
    tokio::task::spawn_blocking(move || {
        let s = CaptureSink(Arc::new(Mutex::new(Vec::new())));
        p1.execute_streaming(&req1, &s)
    })
    .await
    .unwrap()
    .expect("turn 1");

    assert_eq!(reg.len().await, 1, "first turn must register one host");

    let p2 = Arc::clone(&provider);
    let req2 = make_request("turn-2", "conv-1", &worktree);
    tokio::task::spawn_blocking(move || p2.execute_streaming(&req2, &sink))
        .await
        .unwrap()
        .expect("turn 2");

    assert_eq!(
        reg.len().await,
        1,
        "second turn on same conversation must reuse the existing host"
    );
    {
        let evs = sink_events.lock().unwrap();
        assert!(
            evs.iter().any(
                |e| matches!(e, StreamOutputEvent::AssistantText { text } if text == "warm-ack")
            ),
            "second turn should still emit AssistantText"
        );
    }
}
