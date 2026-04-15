//! Persistent Claude Code subprocess registry.
//!
//! The daemon holds a map of live subprocesses keyed by [`SessionKey`].
//! When the orchestrator asks [`ClaudeCodeProvider`] to run a turn and the
//! provider sees a registry, it reuses an existing host instead of cold-
//! spawning `claude -p`.

use crate::errors::GroveResult;
use host::ClaudeSessionHost;
use std::path::PathBuf;
use std::sync::Arc;

/// Identity of a persistent Claude session. Two turns belong to the same
/// host iff both fields match exactly. We key on `work_dir` because Grove
/// supports multiple worktrees per conversation; switching cwd on a live
/// `claude` process is not safe, so each (conv, cwd) pair gets its own host.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
pub struct SessionKey {
    pub conversation_id: String,
    pub work_dir: PathBuf,
}

impl SessionKey {
    pub fn new(conversation_id: impl Into<String>, work_dir: impl Into<PathBuf>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            work_dir: work_dir.into(),
        }
    }
}

pub mod host;
pub mod protocol;

/// Abstraction so the orchestrator can be compiled and tested without a
/// registry (Direct transport passes `None`; daemon passes `Some`).
#[allow(clippy::len_without_is_empty)]
#[async_trait::async_trait]
pub trait SessionHostRegistry: Send + Sync {
    /// Return an existing host for `key`, or spawn one via `spawn_fn` and
    /// register it. The closure receives the optional resume session id so
    /// callers can plumb `--session-id` through.
    async fn get_or_spawn(
        &self,
        key: SessionKey,
        resume_session_id: Option<String>,
        spawn_fn: Box<
            dyn for<'a> FnOnce(
                    Option<&'a str>,
                ) -> futures::future::BoxFuture<
                    'a,
                    GroveResult<Arc<ClaudeSessionHost>>,
                > + Send,
        >,
    ) -> GroveResult<Arc<ClaudeSessionHost>>;

    /// Explicitly evict and shut down the host for `key`, if present.
    async fn evict(&self, key: &SessionKey);

    /// Current size — for metrics and tests.
    async fn len(&self) -> usize;

    /// Erased self for downcast in daemon-side idle-sweep code path.
    fn as_any(&self) -> &dyn std::any::Any;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_conv_same_cwd_is_equal() {
        let a = SessionKey::new("conv-1", "/tmp/a");
        let b = SessionKey::new("conv-1", "/tmp/a");
        assert_eq!(a, b);
    }

    #[test]
    fn different_cwd_is_not_equal() {
        let a = SessionKey::new("conv-1", "/tmp/a");
        let b = SessionKey::new("conv-1", "/tmp/b");
        assert_ne!(a, b, "conv id alone must not collide across worktrees");
    }

    #[test]
    fn different_conv_is_not_equal() {
        let a = SessionKey::new("conv-1", "/tmp/a");
        let b = SessionKey::new("conv-2", "/tmp/a");
        assert_ne!(a, b);
    }
}
