use anyhow::Result;
use std::path::{Path, PathBuf};

/// Default drain-loop concurrency. 2 is a safe starting point: big enough to
/// hide per-task startup cost, small enough that two long-running Claude Code
/// subprocesses don't both pin CPU on a laptop. Override via
/// `GROVE_DAEMON_MAX_CONCURRENT_TASKS`.
const DEFAULT_MAX_CONCURRENT_TASKS: usize = 2;
/// Default idle timeout (seconds) before a persistent session host is reaped
/// by the registry's idle sweep. Override via `GROVE_DAEMON_SESSION_IDLE_SECS`.
const DEFAULT_SESSION_IDLE_SECS: u64 = 900;
/// Default maximum number of live persistent session hosts. When the registry
/// reaches this count it evicts the least-recently-used host before spawning
/// a new one. Override via `GROVE_DAEMON_MAX_SESSIONS`.
const DEFAULT_MAX_SESSIONS: usize = 8;

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub project_root: PathBuf,
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
    pub log_path: PathBuf,
    /// Maximum number of queued tasks the drain loop will execute
    /// concurrently. Enforced by a [`tokio::sync::Semaphore`] in
    /// [`crate::queue_drain`].
    pub max_concurrent_tasks: usize,
    /// Idle timeout (seconds) before a persistent session host is reaped
    /// by the registry's idle sweep. Override via `GROVE_DAEMON_SESSION_IDLE_SECS`.
    pub session_idle_secs: u64,
    /// Maximum number of live persistent session hosts. When the registry
    /// reaches this count it evicts the least-recently-used host before
    /// spawning a new one. Override via `GROVE_DAEMON_MAX_SESSIONS`.
    pub max_sessions: usize,
}

impl DaemonConfig {
    pub fn from_project_root(project_root: &Path) -> Result<Self> {
        use grove_core::config::paths;
        let project_root = project_root.to_path_buf();
        let socket_path = paths::daemon_socket_path(&project_root);
        let pid_path = paths::daemon_pid_path(&project_root);
        let log_path = paths::daemon_log_path(&project_root);
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let max_concurrent_tasks = std::env::var("GROVE_DAEMON_MAX_CONCURRENT_TASKS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_CONCURRENT_TASKS);
        let session_idle_secs = std::env::var("GROVE_DAEMON_SESSION_IDLE_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_SESSION_IDLE_SECS);
        let max_sessions = std::env::var("GROVE_DAEMON_MAX_SESSIONS")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_SESSIONS);
        Ok(Self {
            project_root,
            socket_path,
            pid_path,
            log_path,
            max_concurrent_tasks,
            session_idle_secs,
            max_sessions,
        })
    }
}

#[cfg(test)]
mod tests {
    // A single consolidated test because std::env is process-global: parallel
    // tests mutating `GROVE_DAEMON_MAX_CONCURRENT_TASKS` would race. Keeping
    // all env manipulation in one linear test avoids flakiness without pulling
    // in serial_test.
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn env_parsing() {
        let tmp = tempdir().unwrap();
        let tasks_var = "GROVE_DAEMON_MAX_CONCURRENT_TASKS";
        let idle_var = "GROVE_DAEMON_SESSION_IDLE_SECS";
        let maxh_var = "GROVE_DAEMON_MAX_SESSIONS";

        // SAFETY: inside #[test] the testing thread owns the env for the
        // duration of the test; `unsafe` is required on the 2024 edition.

        // Test 1: unset env must use defaults
        unsafe {
            std::env::remove_var(tasks_var);
            std::env::remove_var(idle_var);
            std::env::remove_var(maxh_var);
        }
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(
            cfg.max_concurrent_tasks, DEFAULT_MAX_CONCURRENT_TASKS,
            "unset env must use the default"
        );
        assert_eq!(cfg.session_idle_secs, 900, "unset idle must use default");
        assert_eq!(cfg.max_sessions, 8, "unset capacity must use default");

        // Test 2: valid overrides must take
        unsafe {
            std::env::set_var(tasks_var, "5");
            std::env::set_var(idle_var, "60");
            std::env::set_var(maxh_var, "3");
        }
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(cfg.max_concurrent_tasks, 5, "valid override must take");
        assert_eq!(cfg.session_idle_secs, 60, "valid idle override must take");
        assert_eq!(cfg.max_sessions, 3, "valid capacity override must take");

        // Test 3: zero values must fall back to defaults
        unsafe {
            std::env::set_var(tasks_var, "0");
            std::env::set_var(idle_var, "0");
            std::env::set_var(maxh_var, "0");
        }
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(
            cfg.max_concurrent_tasks, DEFAULT_MAX_CONCURRENT_TASKS,
            "zero concurrency would wedge the drain — reject and use default"
        );
        assert_eq!(cfg.session_idle_secs, 900, "zero idle must fall back");
        assert_eq!(cfg.max_sessions, 8, "zero capacity must fall back");

        // Test 4: garbage env must fall back to defaults
        unsafe {
            std::env::set_var(tasks_var, "not-a-number");
            std::env::set_var(idle_var, "garbage");
            std::env::set_var(maxh_var, "invalid");
        }
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(
            cfg.max_concurrent_tasks, DEFAULT_MAX_CONCURRENT_TASKS,
            "garbage env must fall back to default, not panic"
        );
        assert_eq!(cfg.session_idle_secs, 900, "garbage idle must fall back");
        assert_eq!(cfg.max_sessions, 8, "garbage capacity must fall back");

        // Cleanup
        unsafe {
            std::env::remove_var(tasks_var);
            std::env::remove_var(idle_var);
            std::env::remove_var(maxh_var);
        }
    }
}
