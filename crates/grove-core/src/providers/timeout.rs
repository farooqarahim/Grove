use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::errors::{GroveError, GroveResult};

/// Run `f` on a background thread and block until it completes or `duration`
/// elapses. Returns `Err(GroveError::Runtime("timed out"))` on timeout.
///
/// `F` must be `Send + 'static` and its return type `R` must be `Send + 'static`
/// because it runs on a separate thread.
pub fn with_timeout<F, R>(duration: Duration, f: F) -> GroveResult<R>
where
    F: FnOnce() -> GroveResult<R> + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = mpsc::channel::<GroveResult<R>>();

    thread::spawn(move || {
        let result = f();
        // Ignore send error — receiver may have timed out and dropped already.
        let _ = tx.send(result);
    });

    match rx.recv_timeout(duration) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(GroveError::Runtime(format!(
            "operation timed out after {}s",
            duration.as_secs()
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(GroveError::Runtime(
            "provider thread panicked or disconnected".to_string(),
        )),
    }
}

/// Run `f` on a background thread. On timeout or disconnect, SIGKILL the
/// child process (if any) registered in `child_pid` before returning the error.
///
/// The caller should share the same `child_pid` Arc with the closure so
/// the PID is set once the child is spawned.
pub fn with_timeout_and_pid<F, R>(
    duration: Duration,
    child_pid: Arc<Mutex<Option<u32>>>,
    f: F,
) -> GroveResult<R>
where
    F: FnOnce() -> GroveResult<R> + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = mpsc::channel::<GroveResult<R>>();

    thread::spawn(move || {
        let result = f();
        let _ = tx.send(result);
    });

    match rx.recv_timeout(duration) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => {
            if let Some(pid) = *child_pid.lock().unwrap() {
                tracing::warn!(pid, "timeout fired — killing child process");
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            }
            Err(GroveError::Runtime(format!(
                "operation timed out after {}s",
                duration.as_secs()
            )))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            if let Some(pid) = *child_pid.lock().unwrap() {
                tracing::warn!(pid, "provider thread disconnected — killing child process");
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            }
            Err(GroveError::Runtime(
                "provider thread panicked or disconnected".to_string(),
            ))
        }
    }
}
