use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shared abort state between the run thread and the abort handler.
///
/// Holds an atomic flag and a PID registry. When `abort()` is called, all
/// registered subprocess PIDs are killed with SIGKILL and the flag is set
/// so the engine can bail out at the next check point.
#[derive(Clone, Debug, Default)]
pub struct AbortHandle {
    aborted: Arc<AtomicBool>,
    pids: Arc<Mutex<Vec<u32>>>,
}

impl AbortHandle {
    pub fn new() -> Self {
        Self {
            aborted: Arc::new(AtomicBool::new(false)),
            pids: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check whether abort has been requested.
    pub fn is_aborted(&self) -> bool {
        self.aborted.load(Ordering::SeqCst)
    }

    /// Signal abort: SIGTERM all registered PIDs, wait 5 s for graceful
    /// shutdown, then SIGKILL any that are still alive.
    pub fn abort(&self) {
        self.aborted.store(true, Ordering::SeqCst);

        // Snapshot PIDs and release the lock before sleeping.
        let pids: Vec<u32> = self.pids.lock().unwrap().clone();

        // Phase 1: request graceful shutdown.
        for &pid in &pids {
            let _ = Command::new("kill")
                .args(["-15", &pid.to_string()])
                .status();
        }

        // Phase 2: grace period — allow agents to flush and checkpoint.
        std::thread::sleep(Duration::from_secs(5));

        // Phase 3: SIGKILL any process that survived the grace period.
        for &pid in &pids {
            let still_alive = Command::new("kill")
                .args(["-0", &pid.to_string()])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if still_alive {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            }
        }
    }

    /// Register a subprocess PID. Returns a [`PidGuard`] that automatically
    /// unregisters the PID when dropped (RAII).
    pub fn register_pid(&self, pid: u32) -> PidGuard {
        self.pids.lock().unwrap().push(pid);
        PidGuard {
            pids: Arc::clone(&self.pids),
            pid,
        }
    }
}

/// RAII guard that removes a PID from the abort handle's registry on drop.
pub struct PidGuard {
    pids: Arc<Mutex<Vec<u32>>>,
    pid: u32,
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        let mut pids = self.pids.lock().unwrap();
        pids.retain(|&p| p != self.pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_sets_flag() {
        let h = AbortHandle::new();
        assert!(!h.is_aborted());
        h.abort();
        assert!(h.is_aborted());
    }

    #[test]
    fn clone_shares_state() {
        let h1 = AbortHandle::new();
        let h2 = h1.clone();
        h1.abort();
        assert!(h2.is_aborted());
    }

    #[test]
    fn pid_guard_unregisters_on_drop() {
        let h = AbortHandle::new();
        {
            let _g = h.register_pid(12345);
            assert_eq!(h.pids.lock().unwrap().len(), 1);
        }
        assert!(h.pids.lock().unwrap().is_empty());
    }

    #[test]
    fn register_multiple_pids() {
        let h = AbortHandle::new();
        let _g1 = h.register_pid(100);
        let _g2 = h.register_pid(200);
        assert_eq!(h.pids.lock().unwrap().len(), 2);
        drop(_g1);
        assert_eq!(h.pids.lock().unwrap().len(), 1);
        assert_eq!(h.pids.lock().unwrap()[0], 200);
    }
}
