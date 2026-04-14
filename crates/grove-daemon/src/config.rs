use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub project_root: PathBuf,
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
    pub log_path: PathBuf,
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
        Ok(Self {
            project_root,
            socket_path,
            pid_path,
            log_path,
        })
    }
}
