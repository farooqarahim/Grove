use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// Returns `true` for Grove's internal agent-handoff files.
/// These coordinate agents inside a worktree but must never be promoted to
/// `project_root` and must not affect the deletion diff.
pub fn is_grove_internal_file(name: &str) -> bool {
    name.starts_with("PLAN_")
        || name.starts_with("GROVE_PLAN_")
        || name.starts_with("GROVE_SPAWN")
        || name.starts_with("TASKS_")
        || name.starts_with("TEST_RESULTS_")
        || name.starts_with("REVIEW_")
        || name.starts_with("BUGS_FIXED_")
        || name.starts_with("SECURITY_REPORT_")
        || name.starts_with("VALIDATION_")
        || name.starts_with("RESEARCH_")
        || name.starts_with("PERF_REPORT_")
        || name.starts_with("MIGRATION_NOTES_")
        || name.starts_with("API_DESIGN_")
        || name.starts_with("COORDINATION_SUMMARY_")
        || name.starts_with("MONITOR_REPORT_")
        || name.starts_with("DEPLOY_NOTES_")
        || name.starts_with("INTEGRATION_NOTES_")
        || name.starts_with(".grove-filter-")
}

/// Gitignore-aware file filter backed by the `ignore` crate (from ripgrep).
///
/// Supports the full `.gitignore` spec: `**` globs, negation (`!`), path
/// separators (`build/output/*.js`), and `.git/info/exclude`.
///
/// `.git` and `.grove` are **not** handled here — they are hardcoded exclusions
/// in the copy/walk functions themselves.
#[derive(Debug, Clone)]
pub struct GitignoreFilter {
    inner: Gitignore,
}

impl GitignoreFilter {
    /// Create a filter with no patterns (matches nothing).
    pub fn empty() -> Self {
        Self {
            inner: Gitignore::empty(),
        }
    }

    /// Read `.gitignore` from `root` and `.git/info/exclude`.
    ///
    /// Returns an empty filter if neither file exists or both are unreadable —
    /// callers fall back to copying everything except the hardcoded exclusions.
    pub fn load(root: &Path) -> Self {
        let mut builder = GitignoreBuilder::new(root);

        // Add the root .gitignore
        let gitignore_path = root.join(".gitignore");
        if gitignore_path.exists() {
            let _ = builder.add(&gitignore_path);
        }

        // Add .git/info/exclude if present (repo-local excludes)
        let exclude_path = root.join(".git").join("info").join("exclude");
        if exclude_path.exists() {
            let _ = builder.add(&exclude_path);
        }

        match builder.build() {
            Ok(inner) => Self { inner },
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse gitignore — using empty filter");
                Self::empty()
            }
        }
    }

    /// Returns `true` if `rel_path` (relative to repo root) should be excluded.
    ///
    /// `is_dir` indicates whether the path is a directory (affects trailing-slash
    /// patterns like `build/`).
    pub fn is_ignored(&self, rel_path: &Path, is_dir: bool) -> bool {
        self.inner
            .matched_path_or_any_parents(rel_path, is_dir)
            .is_ignore()
    }
}

/// The default `.gitignore` content written by `grove init` when none exists.
///
/// `.grove/` is listed here so `git add -A` never commits Grove's internal
/// database, worktrees, or logs to the project's git history.
/// `.git` is intentionally absent — git always ignores its own directory.
pub const DEFAULT_GITIGNORE: &str = r#"# Grove internals (database, worktrees, logs)
.grove/

# Dependencies
node_modules/
vendor/

# Build output
dist/
build/
out/
.next/
.nuxt/
.output/

# Rust
target/

# Python
__pycache__/
*.pyc
*.pyo
.venv/
venv/
env/
*.egg-info/

# Logs & temp files
*.log
*.tmp
*.temp
.cache/
.parcel-cache/

# Environment / secrets
.env
.env.local
.env.*.local

# OS
.DS_Store
Thumbs.db

# Editor
.idea/
.vscode/
*.swp
*.swo
"#;

/// Ensure `.grove/` appears in both `.gitignore` and `.git/info/exclude`
/// so Grove's internal directory is never accidentally committed.
///
/// Rules:
/// - If `.gitignore` exists: scan every line; only append if the entry is absent.
/// - If `.gitignore` does not exist: create it with `.grove/` as the only entry.
/// - `.git/info/exclude`: same scan-then-append logic; skip creation if the file
///   is absent (the `.git` directory may not exist yet for non-git projects).
///
/// The function is idempotent and best-effort — errors are logged but do not
/// propagate so they never block a run from starting.
pub fn ensure_grove_gitignored(project_root: &Path) {
    const ENTRY: &str = ".grove/";

    ensure_entry_in_file(&project_root.join(".gitignore"), ENTRY, true);

    let exclude_path = project_root.join(".git").join("info").join("exclude");
    // Only touch .git/info/exclude if the file already exists (no .git = non-git repo).
    if exclude_path.exists() {
        ensure_entry_in_file(&exclude_path, ENTRY, false);
    }
}

/// Append `entry` to `file_path` if it is not already present.
/// When `create_if_missing` is true the file will be created if absent.
fn ensure_entry_in_file(file_path: &std::path::Path, entry: &str, create_if_missing: bool) {
    let entry_bare = entry.trim_end_matches('/'); // match both ".grove/" and ".grove"

    if file_path.exists() {
        let contents = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %file_path.display(), "could not read file: {e}");
                return;
            }
        };
        let already_present = contents.lines().any(|line| {
            let t = line.trim();
            t == entry || t == entry_bare
        });
        if already_present {
            return;
        }
        // Ensure we start on a new line before appending.
        let append = if contents.ends_with('\n') || contents.is_empty() {
            format!("{entry}\n")
        } else {
            format!("\n{entry}\n")
        };
        if let Err(e) = std::fs::write(file_path, format!("{contents}{append}")) {
            tracing::warn!(path = %file_path.display(), "could not update file: {e}");
        }
    } else if create_if_missing {
        if let Err(e) = std::fs::write(file_path, format!("{entry}\n")) {
            tracing::warn!(path = %file_path.display(), "could not create file: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn empty_filter_matches_nothing() {
        let f = GitignoreFilter::empty();
        assert!(!f.is_ignored(Path::new("node_modules"), false));
        assert!(!f.is_ignored(Path::new("src/main.rs"), false));
    }

    #[test]
    fn load_from_gitignore_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join(".gitignore"),
            "node_modules/\n*.log\nbuild/output/*.js\n",
        )
        .unwrap();

        let f = GitignoreFilter::load(tmp.path());

        // Exact directory match
        assert!(f.is_ignored(Path::new("node_modules"), true));
        // Files inside ignored dir
        assert!(f.is_ignored(Path::new("node_modules/package.json"), false));
        // Suffix glob
        assert!(f.is_ignored(Path::new("app.log"), false));
        assert!(f.is_ignored(Path::new("deep/nested/error.log"), false));
        // Path-separator pattern
        assert!(f.is_ignored(Path::new("build/output/app.js"), false));
        assert!(!f.is_ignored(Path::new("src/app.js"), false));
        // Non-ignored
        assert!(!f.is_ignored(Path::new("src/main.rs"), false));
    }

    #[test]
    fn double_star_glob() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "**/node_modules\n").unwrap();

        let f = GitignoreFilter::load(tmp.path());
        assert!(f.is_ignored(Path::new("node_modules"), true));
        assert!(f.is_ignored(Path::new("a/b/node_modules"), true));
        assert!(!f.is_ignored(Path::new("src"), true));
    }

    #[test]
    fn negation_pattern() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "*.log\n!keep.log\n").unwrap();

        let f = GitignoreFilter::load(tmp.path());
        assert!(f.is_ignored(Path::new("error.log"), false));
        assert!(!f.is_ignored(Path::new("keep.log"), false));
    }

    #[test]
    fn git_info_exclude() {
        let tmp = tempfile::TempDir::new().unwrap();
        let exclude_dir = tmp.path().join(".git").join("info");
        std::fs::create_dir_all(&exclude_dir).unwrap();
        std::fs::write(exclude_dir.join("exclude"), "secret.txt\n").unwrap();

        let f = GitignoreFilter::load(tmp.path());
        assert!(f.is_ignored(Path::new("secret.txt"), false));
        assert!(!f.is_ignored(Path::new("public.txt"), false));
    }

    #[test]
    fn missing_gitignore_returns_empty_filter() {
        let tmp = tempfile::TempDir::new().unwrap();
        let f = GitignoreFilter::load(tmp.path());
        assert!(!f.is_ignored(Path::new("anything"), false));
    }

    #[test]
    fn ensure_grove_gitignored_creates_gitignore_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        ensure_grove_gitignored(tmp.path());
        let contents = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(contents.contains(".grove/"));
    }

    #[test]
    fn ensure_grove_gitignored_injects_into_existing_gitignore() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), "node_modules/\n*.log\n").unwrap();
        ensure_grove_gitignored(tmp.path());
        let contents = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(contents.contains(".grove/"));
        assert!(contents.contains("node_modules/"));
    }

    #[test]
    fn ensure_grove_gitignored_does_not_duplicate() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), ".grove/\nnode_modules/\n").unwrap();
        ensure_grove_gitignored(tmp.path());
        let contents = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(contents.matches(".grove/").count(), 1);
    }

    #[test]
    fn ensure_grove_gitignored_matches_without_trailing_slash() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join(".gitignore"), ".grove\n").unwrap();
        ensure_grove_gitignored(tmp.path());
        // ".grove" (no slash) should count as already present — no duplicate
        let contents = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(contents.matches(".grove").count(), 1);
    }

    #[test]
    fn ensure_grove_gitignored_updates_git_info_exclude() {
        let tmp = tempfile::TempDir::new().unwrap();
        let exclude_dir = tmp.path().join(".git").join("info");
        std::fs::create_dir_all(&exclude_dir).unwrap();
        std::fs::write(exclude_dir.join("exclude"), "# git exclude\n").unwrap();
        ensure_grove_gitignored(tmp.path());
        let contents = std::fs::read_to_string(exclude_dir.join("exclude")).unwrap();
        assert!(contents.contains(".grove/"));
    }

    #[test]
    fn grove_internal_file_detection() {
        assert!(is_grove_internal_file("PLAN_design"));
        assert!(is_grove_internal_file("GROVE_SPAWN.json"));
        assert!(is_grove_internal_file("TASKS_agent1"));
        assert!(!is_grove_internal_file("README.md"));
        assert!(!is_grove_internal_file("src/main.rs"));
    }
}
