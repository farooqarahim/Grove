use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Composite PTY session key: "{conversation_id}:{tab_index}".
///
/// Tab 0 is always the agent tab (auto-launched).
/// Tabs 1+ are user-created shell tabs.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct PtyId(String);

#[allow(dead_code)]
impl PtyId {
    /// Create a PtyId for the given conversation and tab index.
    pub fn new(conversation_id: &str, tab_index: u32) -> Self {
        Self(format!("{conversation_id}:{tab_index}"))
    }

    /// Shorthand for the agent tab (tab index 0).
    pub fn agent(conversation_id: &str) -> Self {
        Self::new(conversation_id, 0)
    }

    /// Parse a raw string into a PtyId. Returns None if the format is invalid.
    pub fn parse(raw: &str) -> Option<Self> {
        let colon = raw.rfind(':')?;
        let _tab: u32 = raw[colon + 1..].parse().ok()?;
        Some(Self(raw.to_string()))
    }

    /// Extract the conversation_id portion.
    pub fn conversation_id(&self) -> &str {
        let colon = self.0.rfind(':').expect("PtyId always contains ':'");
        &self.0[..colon]
    }

    /// Extract the tab index portion.
    pub fn tab_index(&self) -> u32 {
        let colon = self.0.rfind(':').expect("PtyId always contains ':'");
        self.0[colon + 1..]
            .parse()
            .expect("tab_index is always u32")
    }

    /// The raw string representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PtyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Configuration for opening a new PTY session.
#[derive(Debug, Clone, Deserialize)]
pub struct PtyOpenConfig {
    /// Working directory for the spawned process.
    pub cwd: String,
    /// Command + args to spawn. `None` = spawn the user's default login shell.
    pub command: Option<(String, Vec<String>)>,
    /// Extra environment variables to set in the PTY process.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Initial terminal width in columns.
    pub cols: u16,
    /// Initial terminal height in rows.
    pub rows: u16,
}

/// Payload for the `pty:output:{pty_id}` Tauri event.
#[derive(Clone, Serialize)]
pub struct PtyOutputPayload {
    pub data: String,
}

/// Payload for the `pty:exit:{pty_id}` Tauri event.
#[derive(Clone, Serialize)]
pub struct PtyExitPayload {
    pub code: Option<i32>,
}

/// Result returned by `pty_open`.
#[derive(Serialize)]
pub struct PtyOpenResult {
    /// The pty_id that was opened.
    pub pty_id: String,
    /// `true` when a fresh process was spawned, `false` when reusing an existing live session.
    pub is_new: bool,
}
