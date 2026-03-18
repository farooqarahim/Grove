use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::errors::{GroveError, GroveResult};

/// Result of a single file copy attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    Copied,
    Skipped,
}

/// Summary of a `preserve_files` run.
#[derive(Debug, Clone, Default)]
pub struct PreserveResult {
    pub copied: usize,
    pub skipped: usize,
    pub errors: usize,
}

/// Directories that must never be copied into a new worktree.
///
/// This is the authoritative exclusion list for worktree file-copy operations.
/// Add entries here to permanently exclude directories from `preserve_files`.
///
/// | Entry          | Reason                                                          |
/// |----------------|-----------------------------------------------------------------|
/// | `.grove`       | Grove's own metadata dir — must NEVER be shared across sessions|
/// | `.git`         | Git internals — handled by git itself, not us                   |
/// | `node_modules` | Large dependency tree, not project source                       |
/// | `vendor`       | Vendored dependencies                                           |
/// | `.cache`       | Ephemeral build/tool cache                                      |
/// | `dist`         | Build output                                                    |
/// | `build`        | Build output                                                    |
/// | `target`       | Rust build output                                               |
const WORKTREE_COPY_EXCLUDES: &[&str] = &[
    ".grove",
    ".git",
    "node_modules",
    "vendor",
    ".cache",
    "dist",
    "build",
    "target",
];

/// Enumerate candidate files from `project_root` that match any glob-style
/// pattern in `patterns` (e.g., `".env*"`, `".envrc"`).
///
/// Combines results from:
/// 1. `git ls-files --others --ignored --exclude-standard` (gitignored files)
/// 2. `git ls-files --others --exclude-standard` (untracked non-ignored files)
/// 3. A direct filesystem walk at root level (fallback / non-git repos).
pub fn get_candidate_files(project_root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    if patterns.is_empty() {
        return Vec::new();
    }

    let mut candidates: HashSet<PathBuf> = HashSet::new();

    // gitignored files matching the pathspecs
    candidates.extend(run_git_ls_files(
        project_root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
        patterns,
    ));

    // untracked (non-ignored) files matching the pathspecs
    candidates.extend(run_git_ls_files(
        project_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        patterns,
    ));

    // filesystem walk fallback (handles non-git repos and edge cases)
    candidates.extend(walk_candidates(project_root, patterns));

    candidates.into_iter().collect()
}

fn run_git_ls_files(project_root: &Path, args: &[&str], patterns: &[String]) -> Vec<PathBuf> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args).arg("--");
    for pat in patterns {
        cmd.arg(pat);
    }
    cmd.current_dir(project_root);

    let Ok(out) = cmd.output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }

    out.stdout
        .split(|&b| b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| project_root.join(String::from_utf8_lossy(p).trim()))
        .filter(|p| !should_skip_path(p))
        .collect()
}

/// Filesystem walk at root level: find files matching glob patterns.
///
/// Deliberately non-recursive — env files always live at the repo root.
fn walk_candidates(root: &Path, patterns: &[String]) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if should_skip_path(&path) {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_file() {
            let name = entry.file_name();
            let n = name.to_string_lossy();
            if patterns.iter().any(|p| glob_match(p, &n)) {
                found.push(path);
            }
        }
    }
    found
}

/// Simple glob matcher supporting `*` wildcards and exact matches.
fn glob_match(pattern: &str, name: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == name;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.is_empty() {
        return true;
    }
    let mut remaining = name;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else if i == parts.len() - 1 {
            return remaining.ends_with(part);
        } else if let Some(pos) = remaining.find(part) {
            remaining = &remaining[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// Returns `true` if any path component is in `WORKTREE_COPY_EXCLUDES`.
fn should_skip_path(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        WORKTREE_COPY_EXCLUDES.contains(&s.as_ref())
    })
}

/// Copy `src` to `dst` only if `dst` does not already exist (exclusive copy).
///
/// Returns `CopyResult::Skipped` when `dst` already exists — never overwrites.
/// Returns `CopyResult::Copied` on success, `Err` on I/O failure.
pub fn copy_file_exclusive(src: &Path, dst: &Path) -> GroveResult<CopyResult> {
    if dst.exists() {
        return Ok(CopyResult::Skipped);
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            GroveError::Runtime(format!(
                "preserve: create_dir_all {}: {e}",
                parent.display()
            ))
        })?;
    }
    std::fs::copy(src, dst).map_err(|e| {
        GroveError::Runtime(format!(
            "preserve: copy {} → {}: {e}",
            src.display(),
            dst.display()
        ))
    })?;
    Ok(CopyResult::Copied)
}

/// Copy gitignored/untracked files from `project_root` into `worktree_path`.
///
/// Only files matching `patterns` are considered. Files already present in the
/// worktree are never overwritten (`copy_file_exclusive`). Paths containing any
/// `WORKTREE_COPY_EXCLUDES` component are excluded — including `.grove`.
///
/// Never fails worktree creation — I/O errors are counted and logged at warn
/// level. Returns `PreserveResult` with copy/skip/error counts for observability.
pub fn preserve_files(
    project_root: &Path,
    worktree_path: &Path,
    patterns: &[String],
) -> PreserveResult {
    let mut result = PreserveResult::default();
    if patterns.is_empty() {
        return result;
    }

    let candidates = get_candidate_files(project_root, patterns);

    for src in candidates {
        let rel = match src.strip_prefix(project_root) {
            Ok(r) => r.to_owned(),
            Err(_) => {
                tracing::debug!(src = %src.display(), "preserve: cannot strip prefix — skipping");
                continue;
            }
        };

        let dst = worktree_path.join(&rel);

        match copy_file_exclusive(&src, &dst) {
            Ok(CopyResult::Copied) => {
                tracing::debug!(file = %rel.display(), "preserve: copied");
                result.copied += 1;
            }
            Ok(CopyResult::Skipped) => {
                tracing::debug!(file = %rel.display(), "preserve: already exists, skipped");
                result.skipped += 1;
            }
            Err(e) => {
                tracing::warn!(
                    file = %rel.display(), error = %e,
                    "preserve: copy failed — skipping"
                );
                result.errors += 1;
            }
        }
    }

    tracing::debug!(
        copied = result.copied,
        skipped = result.skipped,
        errors = result.errors,
        "preserve_files complete"
    );
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // [11]-A: full unit test suite for preserve_files

    #[test]
    fn env_file_is_copied_to_worktree() {
        let project = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        std::fs::write(project.path().join(".env"), "SECRET=hello").unwrap();

        let patterns = vec![".env".to_string(), ".env.*".to_string()];
        let r = preserve_files(project.path(), worktree.path(), &patterns);

        assert_eq!(r.copied, 1, "expected 1 file copied");
        assert_eq!(r.skipped, 0);
        assert!(worktree.path().join(".env").exists());
        assert_eq!(
            std::fs::read_to_string(worktree.path().join(".env")).unwrap(),
            "SECRET=hello"
        );
    }

    #[test]
    fn empty_patterns_copies_nothing() {
        let project = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        std::fs::write(project.path().join(".env"), "SECRET=hello").unwrap();

        let r = preserve_files(project.path(), worktree.path(), &[]);

        assert_eq!(r.copied, 0);
        assert!(!worktree.path().join(".env").exists());
    }

    #[test]
    fn existing_dst_file_is_not_overwritten() {
        let project = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        std::fs::write(project.path().join(".env"), "FROM_PROJECT").unwrap();
        std::fs::write(worktree.path().join(".env"), "FROM_WORKTREE").unwrap();

        let patterns = vec![".env".to_string()];
        let r = preserve_files(project.path(), worktree.path(), &patterns);

        assert_eq!(r.copied, 0);
        assert_eq!(r.skipped, 1);
        assert_eq!(
            std::fs::read_to_string(worktree.path().join(".env")).unwrap(),
            "FROM_WORKTREE",
            "worktree file must not be overwritten"
        );
    }

    #[test]
    fn node_modules_paths_are_skipped() {
        let project = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        let nm = project.path().join("node_modules");
        std::fs::create_dir_all(&nm).unwrap();
        std::fs::write(nm.join(".env"), "SECRET=skip").unwrap();

        let patterns = vec![".env".to_string()];
        let r = preserve_files(project.path(), worktree.path(), &patterns);

        assert_eq!(r.copied, 0, "node_modules/.env must be skipped");
    }

    /// `.grove` must NEVER be copied into a new worktree under any circumstances.
    /// This is a hard requirement — Grove's metadata directory is session-specific.
    #[test]
    fn grove_dir_is_never_copied_into_worktree() {
        let project = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        // Simulate a .grove directory with a file that matches a copy pattern.
        let grove_dir = project.path().join(".grove");
        std::fs::create_dir_all(&grove_dir).unwrap();
        std::fs::write(grove_dir.join("db.sqlite"), "grove-data").unwrap();
        std::fs::write(grove_dir.join(".env"), "GROVE_SECRET=do-not-copy").unwrap();

        // Use a broad pattern that would normally match .env files.
        let patterns = vec![".env".to_string(), ".env.*".to_string()];
        let r = preserve_files(project.path(), worktree.path(), &patterns);

        assert_eq!(
            r.copied, 0,
            ".grove contents must never be copied to worktree"
        );
        assert!(
            !worktree.path().join(".grove").exists(),
            ".grove directory must not exist in worktree"
        );
    }

    #[test]
    fn no_error_when_project_has_no_matching_files() {
        let project = TempDir::new().unwrap();
        let worktree = TempDir::new().unwrap();

        let patterns = vec![".env".to_string(), ".envrc".to_string()];
        let r = preserve_files(project.path(), worktree.path(), &patterns);

        assert_eq!(r.copied, 0);
        assert_eq!(r.errors, 0);
    }

    #[test]
    fn glob_match_star_prefix() {
        assert!(glob_match(".env.*", ".env.local"));
        assert!(glob_match(".env.*", ".env.production"));
        assert!(!glob_match(".env.*", ".envrc"));
    }

    #[test]
    fn glob_match_exact() {
        assert!(glob_match(".envrc", ".envrc"));
        assert!(!glob_match(".envrc", ".env"));
    }

    #[test]
    fn glob_match_star_suffix() {
        assert!(glob_match(".env*", ".env"));
        assert!(glob_match(".env*", ".envrc"));
        assert!(glob_match(".env*", ".env.local"));
    }
}
