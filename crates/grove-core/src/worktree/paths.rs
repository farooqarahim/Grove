use std::path::{Path, PathBuf};

/// Path of a conversation worktree: `<worktrees_base>/<conversation_id>`
pub fn conv_worktree_path(worktrees_base: &Path, conversation_id: &str) -> PathBuf {
    worktrees_base.join(conversation_id)
}

/// Absolute path of the worktree directory for `session_id` inside `base_dir`.
pub fn worktree_path(base_dir: &Path, session_id: &str) -> PathBuf {
    base_dir.join(session_id)
}

/// Git branch name used for a session's isolated worktree.
/// Convention: `grove/<session_id>` (hardcoded prefix for backward compat).
pub fn branch_name_for_session(session_id: &str) -> String {
    branch_name_for_session_p("grove", session_id)
}

/// Branch name for a session with a configurable prefix.
pub fn branch_name_for_session_p(prefix: &str, session_id: &str) -> String {
    format!("{prefix}/{session_id}")
}

/// Git branch name for a conversation's long-lived branch.
/// Convention: `grove/s_<conversation_id>`
pub fn conv_branch_name(conversation_id: &str) -> String {
    conv_branch_name_p("grove", conversation_id)
}

/// Conversation branch name with a configurable prefix.
pub fn conv_branch_name_p(prefix: &str, conversation_id: &str) -> String {
    format!("{prefix}/s_{conversation_id}")
}
