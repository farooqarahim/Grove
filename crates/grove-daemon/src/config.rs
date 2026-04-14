use anyhow::Result;
use std::path::{Path, PathBuf};

/// Default drain-loop concurrency. 2 is a safe starting point: big enough to
/// hide per-task startup cost, small enough that two long-running Claude Code
/// subprocesses don't both pin CPU on a laptop. Override via
/// `GROVE_DAEMON_MAX_CONCURRENT_TASKS`.
const DEFAULT_MAX_CONCURRENT_TASKS: usize = 2;

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
        Ok(Self {
            project_root,
            socket_path,
            pid_path,
            log_path,
            max_concurrent_tasks,
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
    fn max_concurrent_tasks_env_parsing() {
        let tmp = tempdir().unwrap();
        let var = "GROVE_DAEMON_MAX_CONCURRENT_TASKS";
        // SAFETY: inside #[test] the testing thread owns the env for the
        // duration of the test; `unsafe` is required on the 2024 edition.
        unsafe { std::env::remove_var(var) };
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(
            cfg.max_concurrent_tasks, DEFAULT_MAX_CONCURRENT_TASKS,
            "unset env must use the default"
        );

        unsafe { std::env::set_var(var, "5") };
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(cfg.max_concurrent_tasks, 5, "valid override must take");

        unsafe { std::env::set_var(var, "0") };
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(
            cfg.max_concurrent_tasks, DEFAULT_MAX_CONCURRENT_TASKS,
            "zero concurrency would wedge the drain — reject and use default"
        );

        unsafe { std::env::set_var(var, "not-a-number") };
        let cfg = DaemonConfig::from_project_root(tmp.path()).unwrap();
        assert_eq!(
            cfg.max_concurrent_tasks, DEFAULT_MAX_CONCURRENT_TASKS,
            "garbage env must fall back to default, not panic"
        );
        unsafe { std::env::remove_var(var) };
    }
}
