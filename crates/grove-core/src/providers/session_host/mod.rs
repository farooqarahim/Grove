//! Persistent Claude Code subprocess registry.
//!
//! The daemon holds a map of live subprocesses keyed by [`SessionKey`].
//! When the orchestrator asks [`ClaudeCodeProvider`] to run a turn and the
//! provider sees a registry, it reuses an existing host instead of cold-
//! spawning `claude -p`.

use std::path::PathBuf;

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

pub mod protocol;

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
