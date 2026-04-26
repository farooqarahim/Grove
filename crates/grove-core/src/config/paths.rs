use std::path::{Path, PathBuf};

use uuid::Uuid;

/// UUID v5 namespace for deriving stable project IDs from filesystem paths.
///
/// Canonical definition — shared with `orchestrator::conversation::derive_project_id`.
/// These bytes spell "grove-project-ns" in ASCII. Must never change.
const GROVE_PROJECT_NS: Uuid = Uuid::from_bytes([
    0x67, 0x72, 0x6f, 0x76, 0x65, 0x2d, 0x70, 0x72, 0x6f, 0x6a, 0x65, 0x63, 0x74, 0x2d, 0x6e, 0x73,
]);

/// Derive a stable project ID from the canonical path of the project root.
///
/// Uses UUID v5 (SHA-1 based, deterministic) — the same path always produces
/// the same ID across sessions and machines.
pub fn derive_project_id(project_root: &Path) -> String {
    let canonical = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    Uuid::new_v5(&GROVE_PROJECT_NS, canonical.to_string_lossy().as_bytes())
        .simple()
        .to_string()
}

/// `.grove/` directory inside the project root.
///
/// Worktrees, config, and logs live here. The database has moved to the
/// centralized location returned by `project_db_dir` — see `db_path`.
pub fn grove_dir(project_root: &Path) -> PathBuf {
    project_root.join(".grove")
}

/// `.grove/grove.yaml` — main config file.
pub fn config_path(project_root: &Path) -> PathBuf {
    grove_dir(project_root).join("grove.yaml")
}

/// `~/.grove/workspaces/<project_uuid>/` — centralized per-project data dir.
///
/// `grove.db` lives here rather than inside the project root, so the project
/// directory contains only worktrees, config, and files that belong under VCS.
///
/// Falls back to `project_root` itself in two cases:
/// - The path is already under `~/.grove/workspaces/` (GroveApp virtual root —
///   avoids double-wrapping the machine-level workspace DB).
/// - The path is under the system temp directory (test fixtures — keeps test
///   DBs isolated and prevents residue in `~/.grove/workspaces/`).
pub fn project_db_dir(project_root: &Path) -> PathBuf {
    // GroveApp passes ~/.grove/workspaces/<machine_id>/ as its virtual root.
    // Centralising again would double-wrap it — use the path as-is.
    if let Ok(home) = std::env::var("HOME") {
        let workspaces = PathBuf::from(&home).join(".grove").join("workspaces");
        if project_root.starts_with(&workspaces) {
            return project_root.to_path_buf();
        }
    }

    // Test fixtures use the system temp dir. Keep DBs local there so each
    // TempDir gets its own DB and no residue accumulates in ~/.grove/workspaces/.
    let tmp = std::env::temp_dir();
    let canonical_root = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let canonical_tmp = tmp.canonicalize().unwrap_or(tmp);
    if canonical_root.starts_with(&canonical_tmp) {
        return project_root.to_path_buf();
    }

    // Real project root: centralize under ~/.grove/workspaces/<project_uuid>/.
    let project_id = derive_project_id(project_root);
    grove_app_dir().join("workspaces").join(project_id)
}

/// `~/.grove/workspaces/<project_uuid>/.grove/grove.db` — SQLite database.
///
/// For real project roots the database lives centrally, outside the project
/// directory. The project's `.grove/` is kept for worktrees and config only.
/// For GroveApp virtual roots and test temp dirs, falls back to the old
/// `project_root/.grove/grove.db` path.
pub fn db_path(project_root: &Path) -> PathBuf {
    project_db_dir(project_root).join(".grove").join("grove.db")
}

/// `.grove/logs/` — structured log files.
pub fn logs_dir(project_root: &Path) -> PathBuf {
    grove_dir(project_root).join("logs")
}

/// `~/.grove/workspaces/<project_uuid>/grove.sock` — daemon Unix domain socket.
pub fn daemon_socket_path(project_root: &Path) -> PathBuf {
    project_db_dir(project_root).join("grove.sock")
}

/// `~/.grove/workspaces/<project_uuid>/grove-daemon.pid` — daemon PID file.
pub fn daemon_pid_path(project_root: &Path) -> PathBuf {
    project_db_dir(project_root).join("grove-daemon.pid")
}

/// `~/.grove/workspaces/<project_uuid>/grove-daemon.log` — daemon log file.
pub fn daemon_log_path(project_root: &Path) -> PathBuf {
    project_db_dir(project_root).join("grove-daemon.log")
}

/// `.grove/log/<conversation_id>/` — markdown run memory for a conversation.
pub fn conversation_log_dir(project_root: &Path, conversation_id: &str) -> PathBuf {
    grove_dir(project_root).join("log").join(conversation_id)
}

/// `.grove/log/<conversation_id>/plan/<run_id>.md`
pub fn run_plan_log_path(project_root: &Path, conversation_id: &str, run_id: &str) -> PathBuf {
    conversation_log_dir(project_root, conversation_id)
        .join("plan")
        .join(format!("{run_id}.md"))
}

/// `.grove/log/<conversation_id>/verdict/<run_id>.md`
pub fn run_verdict_log_path(project_root: &Path, conversation_id: &str, run_id: &str) -> PathBuf {
    conversation_log_dir(project_root, conversation_id)
        .join("verdict")
        .join(format!("{run_id}.md"))
}

/// `.grove/reports/` — generated run reports.
pub fn reports_dir(project_root: &Path) -> PathBuf {
    grove_dir(project_root).join("reports")
}

/// `.grove/checkpoints/` — checkpoint JSON files.
pub fn checkpoints_dir(project_root: &Path) -> PathBuf {
    grove_dir(project_root).join("checkpoints")
}

/// `.grove/worktrees/` — git worktree directories.
pub fn worktrees_dir(project_root: &Path) -> PathBuf {
    grove_dir(project_root).join("worktrees")
}

/// `.grove/artifacts/<conversation_id>/<run_id>/` — agent artifact output directory.
///
/// Agents write their pipeline artifacts (PRD, Design, Review, Verdict docs)
/// here instead of in the worktree root, keeping the working tree clean.
pub fn run_artifacts_dir(project_root: &Path, conversation_id: &str, run_id: &str) -> PathBuf {
    grove_dir(project_root)
        .join("artifacts")
        .join(conversation_id)
        .join(run_id)
}

/// Directory for chatter thread JSONL logs: `.grove/chatter/`
pub fn chatter_dir(project_root: &Path) -> PathBuf {
    grove_dir(project_root).join("chatter")
}

/// Path for a specific chatter thread's JSONL log file.
pub fn chatter_log_path(project_root: &Path, conversation_id: &str, chatter_id: &str) -> PathBuf {
    chatter_dir(project_root)
        .join(conversation_id)
        .join(format!("{chatter_id}.jsonl"))
}

// ── Global app paths ─────────────────────────────────────────────────────────

/// `~/.grove/` — global app directory shared across all projects.
pub fn grove_app_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".grove")
}

/// `~/.grove/info.yml` — app-level metadata (active workspace, etc.).
pub fn grove_info_path() -> PathBuf {
    grove_app_dir().join("info.yml")
}

/// `~/.grove/workspaces/_global/tracker_credentials.json` — shared tracker creds.
///
/// This stays under Grove's machine-level app root rather than any project's
/// `.grove/`, so provider tokens never land inside a checked-out repository.
pub fn tracker_credentials_path() -> PathBuf {
    grove_app_dir()
        .join("workspaces")
        .join("_global")
        .join("tracker_credentials.json")
}

/// `~/.grove/workspaces/<id>/` — virtual project root for a workspace.
///
/// This directory is structured like a project root so that `DbHandle::new()`,
/// `GroveConfig::load_or_create()`, and all path helpers work unchanged.
pub fn workspace_data_root(workspace_id: &str) -> PathBuf {
    grove_app_dir().join("workspaces").join(workspace_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_project_id_is_stable() {
        let tmp = tempfile::TempDir::new().unwrap();
        let id1 = derive_project_id(tmp.path());
        let id2 = derive_project_id(tmp.path());
        assert_eq!(id1, id2, "same path must produce the same project ID");
        assert_eq!(id1.len(), 32, "UUID v5 simple string is 32 hex chars");
        assert!(id1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn derive_project_id_differs_per_path() {
        let a = tempfile::TempDir::new().unwrap();
        let b = tempfile::TempDir::new().unwrap();
        assert_ne!(
            derive_project_id(a.path()),
            derive_project_id(b.path()),
            "different paths must produce different IDs"
        );
    }

    #[test]
    fn project_db_dir_uses_tempdir_locally() {
        // Temp dirs must use local (non-centralized) path so tests stay isolated.
        let tmp = tempfile::TempDir::new().unwrap();
        let db_dir = project_db_dir(tmp.path());
        assert_eq!(
            db_dir,
            tmp.path(),
            "temp-dir project_db_dir must return the path itself, not a centralized location"
        );
    }

    #[test]
    fn project_db_dir_skips_grove_virtual_roots() {
        // GroveApp passes ~/.grove/workspaces/<id>/ — must not be double-wrapped.
        let virtual_root = grove_app_dir().join("workspaces").join("test_machine_id");
        // Even if the dir doesn't exist on disk, the path-prefix check fires.
        let db_dir = project_db_dir(&virtual_root);
        assert_eq!(
            db_dir, virtual_root,
            "virtual data root must not be re-centralized"
        );
    }

    #[test]
    fn db_path_ends_with_grove_grove_db() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = db_path(tmp.path());
        // For temp dirs: project_root/.grove/grove.db (local fallback).
        assert!(
            path.ends_with(".grove/grove.db"),
            "db_path must end with .grove/grove.db, got: {}",
            path.display()
        );
    }

    #[test]
    fn initialize_creates_grove_dir_and_db() {
        let tmp = tempfile::TempDir::new().unwrap();

        // initialize() must create .grove/ and a valid database.
        let result = crate::db::initialize(tmp.path());
        assert!(result.is_ok(), "initialize must succeed: {:?}", result);

        // .grove/ directory must exist (worktrees live here).
        assert!(
            tmp.path().join(".grove").exists(),
            ".grove/ must be created"
        );

        // The DB must be accessible and have a valid schema version.
        let handle = crate::db::DbHandle::new(tmp.path());
        let conn = handle
            .connect()
            .expect("DB must be connectable after initialize");
        let version: i64 = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("schema_version must be readable");
        assert!(
            version > 0,
            "schema_version must be positive after migrations"
        );
    }

    #[test]
    fn daemon_paths_are_under_project_db_dir() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();

        let sock = daemon_socket_path(root);
        let pid = daemon_pid_path(root);
        let log = daemon_log_path(root);

        let base = project_db_dir(root);
        assert!(sock.starts_with(&base), "sock {sock:?} not under {base:?}");
        assert!(pid.starts_with(&base), "pid {pid:?} not under {base:?}");
        assert!(log.starts_with(&base), "log {log:?} not under {base:?}");
        assert_eq!(sock.file_name().unwrap(), "grove.sock");
        assert_eq!(pid.file_name().unwrap(), "grove-daemon.pid");
        assert_eq!(log.file_name().unwrap(), "grove-daemon.log");
    }

    #[test]
    fn conversation_log_paths_use_singular_log_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let plan = run_plan_log_path(tmp.path(), "conv_123", "run_456");
        let verdict = run_verdict_log_path(tmp.path(), "conv_123", "run_456");
        assert!(plan.ends_with(".grove/log/conv_123/plan/run_456.md"));
        assert!(verdict.ends_with(".grove/log/conv_123/verdict/run_456.md"));
    }
}
