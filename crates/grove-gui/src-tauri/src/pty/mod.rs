pub mod launch;
pub mod session;
pub mod types;

use parking_lot::Mutex;
use std::collections::HashMap;

use tauri::State;

use session::PtySession;
use types::{PtyId, PtyOpenConfig, PtyOpenResult};

use crate::state::AppState;

/// Manages all live PTY sessions.
///
/// Stored as a Tauri managed state alongside AppState.
pub struct PtyManager {
    sessions: Mutex<HashMap<String, PtySession>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }

    /// Open a new PTY session or return an existing alive one.
    ///
    /// Returns `Ok(true)` if a new process was spawned, `Ok(false)` if reusing.
    pub fn open(
        &self,
        pty_id: &PtyId,
        config: PtyOpenConfig,
        app_handle: tauri::AppHandle,
    ) -> Result<bool, String> {
        let key = pty_id.as_str().to_string();

        // Hold a single lock across check-remove-spawn-insert to avoid TOCTOU races.
        let mut sessions = self.sessions.lock();

        // Fast path: session exists and process is alive.
        if let Some(session) = sessions.get(&key) {
            if session.is_alive() {
                return Ok(false);
            }
        }

        // Remove any stale session.
        if let Some(old) = sessions.remove(&key) {
            old.close();
        }

        // Spawn new session (fork+openpty is fast, lock contention is minimal).
        let session = PtySession::spawn(pty_id.clone(), config, app_handle)?;
        sessions.insert(key, session);
        Ok(true)
    }

    /// Write data to a PTY session's stdin.
    pub fn write(&self, pty_id: &PtyId, data: &[u8]) -> Result<(), String> {
        let key = pty_id.as_str();
        let mut sessions = self.sessions.lock();
        let session = sessions
            .get_mut(key)
            .ok_or_else(|| format!("PTY '{}' not found", pty_id))?;
        session.write(data)
    }

    /// Resize a PTY session.
    pub fn resize(&self, pty_id: &PtyId, cols: u16, rows: u16) -> Result<(), String> {
        let key = pty_id.as_str();
        let sessions = self.sessions.lock();
        let session = sessions
            .get(key)
            .ok_or_else(|| format!("PTY '{}' not found", pty_id))?;
        session.resize(cols, rows)
    }

    /// Close and remove a PTY session.
    pub fn close(&self, pty_id: &PtyId) -> Result<(), String> {
        let key = pty_id.as_str().to_string();
        if let Some(session) = self.sessions.lock().remove(&key) {
            session.close();
        }
        Ok(())
    }

    /// Check if a PTY session is alive.
    #[allow(dead_code)]
    pub fn is_alive(&self, pty_id: &PtyId) -> bool {
        let key = pty_id.as_str();
        let sessions = self.sessions.lock();
        sessions.get(key).map(|s| s.is_alive()).unwrap_or(false)
    }
}

impl Drop for PtyManager {
    fn drop(&mut self) {
        let sessions = std::mem::take(self.sessions.get_mut());
        for (_, session) in sessions {
            session.close();
        }
    }
}

// ── Tauri Commands ──────────────────────────────────────────────────────────

/// Open a PTY session. For agent tabs, resolves the CLI command from the
/// conversation's provider. For shell tabs, spawns a login shell.
///
/// The `pty_id` format is `"{conversation_id}:{tab_index}"`.
/// - Tab 0 = agent tab (auto-resolves CLI from conversation)
/// - Tab 1+ = shell tab (uses provided config)
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn pty_open(
    state: State<'_, AppState>,
    pty_manager: State<'_, PtyManager>,
    app_handle: tauri::AppHandle,
    pty_id: String,
    cwd: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
    ssh_target: Option<String>,
    ssh_port: Option<u16>,
    ssh_remote_path: Option<String>,
) -> Result<PtyOpenResult, String> {
    let parsed = PtyId::parse(&pty_id).ok_or_else(|| {
        format!("invalid pty_id format: '{pty_id}' (expected 'conv_id:tab_index')")
    })?;

    // SSH path — build an SSH command directly, bypassing agent/shell resolution.
    if let Some(target) = ssh_target {
        let mut ssh_args = vec![];
        if let Some(port) = ssh_port {
            ssh_args.push("-p".to_string());
            ssh_args.push(port.to_string());
        }
        ssh_args.push("-t".to_string());
        ssh_args.push(target);
        let remote_command = ssh_remote_path
            .map(|path| format!("cd {} && exec $SHELL -l", launch::shell_escape(&path)))
            .unwrap_or_else(|| "exec $SHELL -l".to_string());
        ssh_args.push(remote_command);

        let config = PtyOpenConfig {
            cwd: cwd.unwrap_or_default(),
            command: Some(("ssh".to_string(), ssh_args)),
            env: std::collections::HashMap::new(),
            cols: cols.unwrap_or(120),
            rows: rows.unwrap_or(32),
        };
        let is_new = pty_manager.open(&parsed, config, app_handle)?;
        return Ok(PtyOpenResult { pty_id, is_new });
    }

    let config = if parsed.tab_index() == 0 {
        // Agent tab — try to resolve CLI command from conversation's provider.
        // If the conversation is not a CLI conversation, fall back to a shell.
        match launch::resolve_agent_launch(&state, parsed.conversation_id())? {
            Some(agent_config) => agent_config,
            None => {
                let resolved_cwd = cwd.unwrap_or_else(|| {
                    dirs::home_dir()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "/".to_string())
                });
                launch::resolve_shell_launch(&resolved_cwd, None)
            }
        }
    } else {
        // Shell tab — use provided cwd or resolve from conversation.
        let resolved_cwd = cwd.unwrap_or_else(|| {
            dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string())
        });
        launch::resolve_shell_launch(&resolved_cwd, None)
    };

    // Override cols/rows if provided by the frontend.
    let config = PtyOpenConfig {
        cols: cols.unwrap_or(config.cols),
        rows: rows.unwrap_or(config.rows),
        ..config
    };

    let is_new = pty_manager.open(&parsed, config, app_handle)?;

    Ok(PtyOpenResult { pty_id, is_new })
}

/// Write input data to a PTY session.
#[tauri::command]
pub fn pty_write_new(
    pty_manager: State<'_, PtyManager>,
    pty_id: String,
    data: String,
) -> Result<(), String> {
    let parsed = PtyId::parse(&pty_id).ok_or_else(|| format!("invalid pty_id: '{pty_id}'"))?;
    pty_manager.write(&parsed, data.as_bytes())
}

/// Resize a PTY session's terminal window.
#[tauri::command]
pub fn pty_resize_new(
    pty_manager: State<'_, PtyManager>,
    pty_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let parsed = PtyId::parse(&pty_id).ok_or_else(|| format!("invalid pty_id: '{pty_id}'"))?;
    pty_manager.resize(&parsed, cols, rows)
}

/// Close a PTY session.
#[tauri::command]
pub fn pty_close_new(pty_manager: State<'_, PtyManager>, pty_id: String) -> Result<(), String> {
    let parsed = PtyId::parse(&pty_id).ok_or_else(|| format!("invalid pty_id: '{pty_id}'"))?;
    pty_manager.close(&parsed)
}
