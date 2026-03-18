use std::path::Path;
use std::process::Command;

/// Maximum bytes of diff output to include in handoff context.
/// Keeps agent prompts within reasonable token budgets.
const MAX_DIFF_BYTES: usize = 8192;

/// Generate a differential handoff context string for the next sequential agent.
///
/// Produces a human-readable summary of what the previous agent changed,
/// based on `git diff --stat` and a truncated unified diff between
/// `parent_sha` (the SHA the agent started from) and `checkpoint_sha`
/// (the SHA after the agent committed).
///
/// Returns `None` if either SHA is missing, the diff is empty, or git
/// commands fail (non-fatal — the run continues without handoff context).
pub fn build_handoff_context(
    worktree_path: &Path,
    parent_sha: Option<&str>,
    checkpoint_sha: Option<&str>,
) -> Option<String> {
    let parent = parent_sha?;
    let current = checkpoint_sha?;

    if parent == current {
        return None;
    }

    // --stat gives a compact summary of what changed.
    let stat = git_diff_stat(worktree_path, parent, current)?;
    if stat.trim().is_empty() {
        return None;
    }

    // Unified diff (truncated) gives the actual changes.
    let diff = git_diff_unified(worktree_path, parent, current)?;

    let truncated_diff = if diff.len() > MAX_DIFF_BYTES {
        let cut = &diff[..MAX_DIFF_BYTES];
        // Cut at a newline boundary to avoid mid-line truncation.
        let end = cut.rfind('\n').unwrap_or(MAX_DIFF_BYTES);
        format!(
            "{}\n\n... (diff truncated, {} total bytes)",
            &diff[..end],
            diff.len()
        )
    } else {
        diff
    };

    Some(format!(
        "\n--- PREVIOUS AGENT CHANGES ---\n\
         The previous agent made the following changes to the codebase.\n\
         Build on this work — do NOT revert or redo these changes.\n\n\
         Summary:\n{stat}\n\n\
         Diff:\n```\n{truncated_diff}\n```\n\
         --- END PREVIOUS AGENT CHANGES ---\n\n"
    ))
}

/// `git diff --stat parent..checkpoint`
fn git_diff_stat(worktree_path: &Path, parent: &str, head: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["diff", "--stat", &format!("{parent}..{head}")])
        .current_dir(worktree_path)
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let s = String::from_utf8_lossy(&out.stdout).to_string();
    if s.trim().is_empty() { None } else { Some(s) }
}

/// `git diff parent..checkpoint` (unified diff).
fn git_diff_unified(worktree_path: &Path, parent: &str, head: &str) -> Option<String> {
    let out = Command::new("git")
        .args(["diff", &format!("{parent}..{head}")])
        .current_dir(worktree_path)
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let s = String::from_utf8_lossy(&out.stdout).to_string();
    if s.trim().is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_when_both_shas_missing() {
        assert!(build_handoff_context(Path::new("/tmp"), None, None).is_none());
    }

    #[test]
    fn none_when_parent_missing() {
        assert!(build_handoff_context(Path::new("/tmp"), None, Some("abc123")).is_none());
    }

    #[test]
    fn none_when_checkpoint_missing() {
        assert!(build_handoff_context(Path::new("/tmp"), Some("abc123"), None).is_none());
    }

    #[test]
    fn none_when_shas_equal() {
        assert!(build_handoff_context(Path::new("/tmp"), Some("abc"), Some("abc")).is_none());
    }
}
