use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug)]
pub struct PidGuard {
    path: std::path::PathBuf,
}

impl PidGuard {
    pub fn acquire(path: &Path) -> Result<Self> {
        if let Some(existing) = read_pid(path) {
            if process_is_live(existing) {
                anyhow::bail!("daemon already running with pid {existing}");
            }
            let _ = std::fs::remove_file(path);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, std::process::id().to_string())
            .with_context(|| format!("write pid file {path:?}"))?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub fn process_is_live(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal;
        use nix::unistd::Pid;
        signal::kill(Pid::from_raw(pid as i32), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn acquire_writes_pid_and_drop_removes_it() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("pid");
        {
            let _g = PidGuard::acquire(&p).unwrap();
            assert_eq!(read_pid(&p), Some(std::process::id()));
        }
        assert!(!p.exists(), "pid file should be removed on drop");
    }

    #[cfg(unix)]
    #[test]
    fn second_acquire_with_live_pid_fails() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("pid");
        let _g = PidGuard::acquire(&p).unwrap();
        let err = PidGuard::acquire(&p).unwrap_err();
        assert!(err.to_string().contains("already running"));
    }

    #[test]
    fn stale_pid_is_replaced() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("pid");
        std::fs::write(&p, "4000000").unwrap();
        let _g = PidGuard::acquire(&p).expect("should clear stale pid");
    }
}
