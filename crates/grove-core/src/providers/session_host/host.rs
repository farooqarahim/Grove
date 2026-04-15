//! Owns one persistent `claude -p` subprocess and its stdin/stdout pipes.
//!
//! Each call to [`ClaudeSessionHost::send_turn`] writes one user-turn JSON
//! line to stdin, then reads stdout lines through the first `result` event
//! and returns all collected [`StreamEvent`]s.

use super::protocol::{StreamEvent, decode_stream_event, encode_user_turn};
use crate::errors::{GroveError, GroveResult};
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;

/// A live persistent Claude subprocess.
///
/// `send_turn` serializes concurrent callers via an inner [`Mutex`]; each
/// turn writes one line to stdin then drains stdout until a `result` event
/// arrives. The process is killed on drop (`Command::kill_on_drop(true)`).
pub struct ClaudeSessionHost {
    inner: Mutex<HostInner>,
    last_used: Mutex<Instant>,
    session_id: Mutex<Option<String>>,
}

struct HostInner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

#[derive(Debug)]
pub struct TurnOutcome {
    pub events: Vec<StreamEvent>,
    pub cost_usd: f64,
    pub is_error: bool,
    pub session_id: Option<String>,
}

impl ClaudeSessionHost {
    pub async fn spawn(
        binary: &Path,
        work_dir: &Path,
        resume_session_id: Option<&str>,
    ) -> GroveResult<Arc<Self>> {
        let mut cmd = Command::new(binary);
        cmd.arg("-p")
            .arg("--input-format")
            .arg("stream-json")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose");
        if let Some(sid) = resume_session_id {
            cmd.arg("--session-id").arg(sid);
        }
        cmd.current_dir(work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| GroveError::Runtime(format!("spawn claude persistent host: {e}")))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| GroveError::Runtime("persistent claude stdin missing".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| GroveError::Runtime("persistent claude stdout missing".into()))?;
        Ok(Arc::new(Self {
            inner: Mutex::new(HostInner {
                child,
                stdin,
                stdout: BufReader::new(stdout),
            }),
            last_used: Mutex::new(Instant::now()),
            session_id: Mutex::new(resume_session_id.map(str::to_owned)),
        }))
    }

    pub async fn send_turn(&self, prompt: &str) -> GroveResult<TurnOutcome> {
        let mut inner = self.inner.lock().await;
        let line = encode_user_turn(prompt)?;
        inner
            .stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| GroveError::Runtime(format!("stdin write: {e}")))?;
        inner
            .stdin
            .write_all(b"\n")
            .await
            .map_err(|e| GroveError::Runtime(format!("stdin newline: {e}")))?;
        inner
            .stdin
            .flush()
            .await
            .map_err(|e| GroveError::Runtime(format!("stdin flush: {e}")))?;

        let mut events = Vec::new();
        let mut buf = String::new();
        // `break` carries the final `Result` event fields out of the loop.
        let (cost_usd, is_error, sid_from_result) = loop {
            buf.clear();
            let n = inner
                .stdout
                .read_line(&mut buf)
                .await
                .map_err(|e| GroveError::Runtime(format!("stdout read: {e}")))?;
            if n == 0 {
                return Err(GroveError::Runtime(
                    "persistent claude closed stdout mid-turn".into(),
                ));
            }
            let Some(ev) = decode_stream_event(&buf)? else {
                continue;
            };
            if let StreamEvent::System {
                session_id: Some(sid),
                ..
            } = &ev
            {
                let mut slot = self.session_id.lock().await;
                if slot.is_none() {
                    *slot = Some(sid.clone());
                }
            }
            if let StreamEvent::Result {
                session_id,
                cost_usd: c,
                is_error: ie,
            } = &ev
            {
                let result = (*c, *ie, session_id.clone());
                events.push(ev);
                break result;
            }
            events.push(ev);
        };
        *self.last_used.lock().await = Instant::now();
        let session_id = match sid_from_result {
            Some(s) => Some(s),
            None => self.session_id.lock().await.clone(),
        };
        Ok(TurnOutcome {
            events,
            cost_usd,
            is_error,
            session_id,
        })
    }

    pub async fn last_used(&self) -> Instant {
        *self.last_used.lock().await
    }

    pub async fn session_id(&self) -> Option<String> {
        self.session_id.lock().await.clone()
    }

    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        let _ = inner.stdin.shutdown().await;
        let _ = inner.child.start_kill();
        let _ = inner.child.wait().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn fake_claude_script() -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .prefix("fake-claude-")
            .suffix(".sh")
            .tempfile()
            .unwrap();
        writeln!(f, r#"#!/bin/sh
while IFS= read -r line; do
  echo '{{"type":"system","session_id":"FAKE-SID","model":"fake"}}'
  echo '{{"type":"assistant","message":{{"content":[{{"type":"text","text":"ack"}}]}}}}'
  echo '{{"type":"result","subtype":"success","session_id":"FAKE-SID","cost_usd":0.001,"is_error":false}}'
done
"#).unwrap();
        f.flush().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(f.path()).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(f.path(), perms).unwrap();
        }
        f
    }

    #[tokio::test]
    async fn spawn_and_send_turn_round_trip() {
        let script = fake_claude_script();
        let tmp = tempfile::tempdir().unwrap();
        let host = ClaudeSessionHost::spawn(script.path(), tmp.path(), None)
            .await
            .expect("spawn");
        let out = host.send_turn("hello").await.expect("turn");
        assert_eq!(out.session_id.as_deref(), Some("FAKE-SID"));
        assert!(!out.is_error);
        assert!(
            out.events
                .iter()
                .any(|e| matches!(e, StreamEvent::AssistantText(t) if t == "ack"))
        );
    }

    #[tokio::test]
    async fn two_sequential_turns_reuse_same_process() {
        let script = fake_claude_script();
        let tmp = tempfile::tempdir().unwrap();
        let host = ClaudeSessionHost::spawn(script.path(), tmp.path(), None)
            .await
            .expect("spawn");
        let _a = host.send_turn("turn 1").await.expect("turn 1");
        let _b = host.send_turn("turn 2").await.expect("turn 2");
        assert!(host.last_used().await <= Instant::now());
    }

    #[tokio::test]
    async fn shutdown_kills_child() {
        let script = fake_claude_script();
        let tmp = tempfile::tempdir().unwrap();
        let host = ClaudeSessionHost::spawn(script.path(), tmp.path(), None)
            .await
            .expect("spawn");
        host.shutdown().await;
        assert!(host.send_turn("after shutdown").await.is_err());
    }
}
