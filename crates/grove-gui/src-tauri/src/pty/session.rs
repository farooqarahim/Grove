use std::io::Write as _;
use std::os::fd::AsFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use std::time::Duration;

use tauri::Emitter as _;
use tokio::io::AsyncReadExt;
use tokio::task::JoinHandle;

use super::types::{PtyExitPayload, PtyId, PtyOpenConfig, PtyOutputPayload};

/// A live PTY session backed by `pty-process`.
///
/// The read half is driven by a tokio task that emits `pty:output:{pty_id}` events.
/// The write half is a raw `std::fs::File` (dup'd from the PTY master fd) so that
/// sync Tauri commands can write without an async runtime.
pub struct PtySession {
    /// A dup'd File descriptor to the PTY master — used for sync writes.
    write_file: std::fs::File,
    /// The `OwnedWritePty` — kept alive to hold the Arc on the PTY master.
    /// Also used for resize (which is sync on the inner fd).
    write_half: pty_process::OwnedWritePty,
    /// `true` while the spawned process is running.
    pub alive: Arc<AtomicBool>,
    /// Exit code set by the reader task when the process terminates.
    pub exit_code: Arc<Mutex<Option<i32>>>,
    /// Handle to the tokio reader task (for cleanup on close).
    reader_handle: JoinHandle<()>,
}

impl PtySession {
    /// Spawn a new PTY session.
    ///
    /// - Opens a PTY pair via `pty_process::open()`.
    /// - Spawns the requested command (or user's login shell) on the slave side.
    /// - Splits into read/write halves; read half drives a tokio reader task.
    /// - Returns the PtySession (owns write half + metadata).
    pub fn spawn(
        pty_id: PtyId,
        config: PtyOpenConfig,
        app_handle: tauri::AppHandle,
    ) -> Result<Self, String> {
        let (pty, pts) = pty_process::open().map_err(|e| format!("failed to allocate PTY: {e}"))?;
        pty.resize(pty_process::Size::new(config.rows, config.cols))
            .map_err(|e| format!("failed to set initial PTY size: {e}"))?;

        // Build the command.
        let (binary, args) = match &config.command {
            Some((bin, a)) => (bin.clone(), a.clone()),
            None => {
                let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
                (shell, vec!["-l".to_string()])
            }
        };

        let cwd = resolve_cwd(&config.cwd);

        // pty_process::Command uses a builder pattern where methods consume self.
        let mut cmd = pty_process::Command::new(&binary);
        for arg in &args {
            cmd = cmd.arg(arg);
        }
        cmd = cmd.current_dir(&cwd);

        // Set environment variables.
        cmd = cmd.env("TERM", "xterm-256color");
        cmd = cmd.env("COLORTERM", "truecolor");
        if let Ok(path) = std::env::var("PATH") {
            cmd = cmd.env("PATH", &path);
        }
        for (key, val) in &config.env {
            cmd = cmd.env(key, val);
        }

        let mut child = cmd
            .spawn(pts)
            .map_err(|e| format!("failed to spawn '{binary}': {e}"))?;

        // Dup the raw fd for synchronous writes before splitting (which moves
        // ownership of the Pty into Arc-wrapped halves).
        let dup_fd = pty
            .as_fd()
            .try_clone_to_owned()
            .map_err(|e| format!("failed to dup PTY fd: {e}"))?;
        let write_file = std::fs::File::from(dup_fd);

        let (read_half, write_half) = pty.into_split();

        let alive = Arc::new(AtomicBool::new(true));
        let exit_code: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));

        let reader_handle = tokio::spawn(reader_loop(
            read_half,
            pty_id.clone(),
            app_handle.clone(),
            Arc::clone(&alive),
            Arc::clone(&exit_code),
        ));

        // Spawn a background task to wait for the child process exit code.
        let alive_for_wait = Arc::clone(&alive);
        let exit_code_for_wait = Arc::clone(&exit_code);
        tokio::spawn(async move {
            let status = child.wait().await;
            let code = status.ok().and_then(|s| s.code());
            *exit_code_for_wait.lock() = code;
            alive_for_wait.store(false, Ordering::SeqCst);
        });

        Ok(Self {
            write_file,
            write_half,
            alive,
            exit_code,
            reader_handle,
        })
    }

    /// Write data to the PTY's stdin (synchronous, via dup'd fd).
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        self.write_file
            .write_all(data)
            .map_err(|e| format!("PTY write failed: {e}"))?;
        self.write_file
            .flush()
            .map_err(|e| format!("PTY flush failed: {e}"))
    }

    /// Resize the PTY terminal window.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), String> {
        self.write_half
            .resize(pty_process::Size::new(rows, cols))
            .map_err(|e| format!("PTY resize failed: {e}"))
    }

    /// Check if the process is still running.
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Get the process exit code (None if still running or unknown).
    #[allow(dead_code)]
    pub fn get_exit_code(&self) -> Option<i32> {
        *self.exit_code.lock()
    }

    /// Abort the reader task and clean up.
    pub fn close(self) {
        self.reader_handle.abort();
        // write_half and write_file are dropped here, which closes the master PTY.
        // The child process receives SIGHUP and should exit.
    }
}

/// Async reader loop: reads from the PTY read half, buffers output, and
/// flushes to the frontend via Tauri events every 16ms.
async fn reader_loop(
    mut read_half: pty_process::OwnedReadPty,
    pty_id: PtyId,
    app_handle: tauri::AppHandle,
    alive: Arc<AtomicBool>,
    exit_code: Arc<Mutex<Option<i32>>>,
) {
    let mut buf = [0u8; 8192];
    let mut pending = Vec::with_capacity(8192);
    let mut flush_interval = tokio::time::interval(Duration::from_millis(16));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let output_event = format!("pty:output:{}", pty_id);
    let exit_event = format!("pty:exit:{}", pty_id);

    loop {
        tokio::select! {
            biased;

            result = read_half.read(&mut buf) => {
                match result {
                    Ok(0) | Err(_) => {
                        // EOF or read error — flush remaining bytes and exit.
                        if !pending.is_empty() {
                            let data = String::from_utf8_lossy(&pending).into_owned();
                            let _ = app_handle.emit(&output_event, PtyOutputPayload { data });
                            pending.clear();
                        }
                        alive.store(false, Ordering::SeqCst);
                        let code = *exit_code.lock();
                        let _ = app_handle.emit(&exit_event, PtyExitPayload { code });
                        break;
                    }
                    Ok(n) => {
                        pending.extend_from_slice(&buf[..n]);
                    }
                }
            }

            _ = flush_interval.tick() => {
                if !pending.is_empty() {
                    let data = String::from_utf8_lossy(&pending).into_owned();
                    let _ = app_handle.emit(&output_event, PtyOutputPayload { data });
                    pending.clear();
                }
            }
        }
    }
}

/// Resolve `cwd` to an absolute path, expanding empty/"~" to $HOME.
fn resolve_cwd(cwd: &str) -> std::path::PathBuf {
    if cwd.is_empty() || cwd == "~" {
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
    } else {
        std::path::PathBuf::from(cwd)
    }
}
