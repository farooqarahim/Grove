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
    pub fn from_project_root(_project_root: &Path) -> Result<Self> {
        anyhow::bail!("DaemonConfig::from_project_root not implemented — see Task 2")
    }
}
