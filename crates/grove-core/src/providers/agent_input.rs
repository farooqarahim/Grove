use std::io::{self, Write};

/// Unified handle for writing answers back to a running agent process.
/// Stored in AppState keyed by run_id. Removed when agent exits.
pub enum AgentInputHandle {
    /// Claude Code: piped stdin, expects JSON-formatted answers.
    Pipe(std::process::ChildStdin),
    /// PTY agents: the MasterPty writer, expects raw text + newline.
    Pty(Box<dyn Write + Send>),
}

// Safety: ChildStdin is Send. Box<dyn Write + Send> is Send by bound.
// Wrapped in Mutex in AppState for Sync.
unsafe impl Sync for AgentInputHandle {}

impl AgentInputHandle {
    /// Write a user's answer to the running agent.
    pub fn write_answer(&mut self, text: &str) -> io::Result<()> {
        match self {
            Self::Pipe(stdin) => {
                let payload = serde_json::json!({
                    "type": "user_input",
                    "text": text,
                });
                writeln!(stdin, "{payload}")?;
                stdin.flush()
            }
            Self::Pty(writer) => {
                writer.write_all(text.as_bytes())?;
                writer.write_all(b"\n")?;
                writer.flush()
            }
        }
    }
}
