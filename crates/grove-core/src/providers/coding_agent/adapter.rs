use crate::errors::GroveResult;
use crate::providers::SessionContinuityPolicy;

/// How the agent formats answers when receiving Q&A input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerFormat {
    /// Write the answer as raw text followed by a newline.
    RawText,
    /// Write the answer as a JSON object: `{"type":"user_input","text":"..."}`.
    Json,
}

/// How the agent process receives its prompt and produces output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    /// Standard subprocess with piped stdin/stdout/stderr.
    /// The prompt is passed on the command line.
    Pipe,
    /// Subprocess inside a pseudo-terminal (for agents that check `isatty(stdout)`).
    /// The prompt is passed on the command line.
    Pty,
    /// Like `Pipe`, but the prompt is written to the process's stdin after startup.
    /// Used for TUI agents (e.g. opencode) that do not accept a prompt argument.
    StdinInjection,
}

// ── Debug impl for trait objects ─────────────────────────────────────────────

/// Provides `Debug` for `Box<dyn CodingAgentAdapter>`, required by
/// `#[derive(Debug)]` on `CodingAgentProvider`.
impl std::fmt::Debug for dyn CodingAgentAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CodingAgentAdapter({})", self.id())
    }
}

/// Per-agent execution adapter.
///
/// Each built-in coding agent implements this trait in its own dedicated file.
/// The adapter encodes everything specific to that agent:
/// - which CLI binary to call
/// - how arguments are assembled (auto-approve flags, model flag, prompt placement)
/// - how the process is launched (pipe, PTY, stdin injection)
/// - any output post-processing (ANSI stripping, structured extraction, etc.)
///
/// To add a new agent: create a new file (e.g. `mynewagent.rs`), implement this
/// trait, and register the adapter in `get_adapter` inside `mod.rs`.
pub trait CodingAgentAdapter: Send + Sync {
    /// Unique ID matching the catalog / `grove.yaml` key (e.g. `"codex"`, `"gemini"`).
    fn id(&self) -> &'static str;

    /// Default binary name or path. Can be overridden per-project in `grove.yaml`.
    fn default_command(&self) -> &str;

    /// How the agent process is launched and how it receives its prompt.
    fn execution_mode(&self) -> ExecutionMode;

    /// Build the complete argument list for one invocation.
    ///
    /// `model` is the user-selected model override (`None` = use the agent's own default).
    /// `prompt` is the full task instructions text.
    fn build_args(&self, model: Option<&str>, prompt: &str) -> Vec<String>;

    /// Post-process collected raw output before it is stored as the session summary.
    ///
    /// Default implementation returns the string unchanged.
    /// Override to strip ANSI sequences, extract structured content, etc.
    fn process_output(&self, raw: String) -> String {
        raw
    }

    /// Parse raw output into `(summary, provider_session_id)`.
    ///
    /// Returns `Err` if the agent reported an explicit failure (e.g. codex
    /// `turn.failed`).  The error propagates as a failed session so retry logic
    /// can fire correctly.
    ///
    /// Override for agents that emit structured output (JSONL with session IDs,
    /// explicit error events, etc.).  The default delegates to `process_output`
    /// and returns `Ok((text, None))`.
    fn parse_output(&self, raw: String) -> GroveResult<(String, Option<String>)> {
        Ok((self.process_output(raw), None))
    }

    /// Build args for resuming a previous session (e.g. `codex exec resume <id>`).
    ///
    /// Return `Some(args)` to override the normal `build_args` path.  The
    /// default returns `None`, meaning the agent does not support session
    /// resumption and `build_args` is used as usual.
    fn build_resume_args(&self, _session_id: &str) -> Option<Vec<String>> {
        None
    }

    /// How this agent handles provider-native thread continuity.
    fn session_continuity_policy(&self) -> SessionContinuityPolicy {
        if self.build_resume_args("resume-probe").is_some() {
            SessionContinuityPolicy::DetachedResume
        } else {
            SessionContinuityPolicy::None
        }
    }

    /// Whether this agent may prompt the user for input during execution.
    ///
    /// When `true`, `CodingAgentProvider::execute_interactive` will monitor
    /// stdout line-by-line for question patterns (via `question_detector`)
    /// and pipe answers back to the agent's stdin or PTY.
    ///
    /// Default is `false` — most agents run headless and never prompt.
    fn supports_interactive(&self) -> bool {
        false
    }

    /// How the agent expects answers to be written to its stdin/PTY.
    ///
    /// Only relevant when `supports_interactive()` returns `true`.
    fn answer_format(&self) -> AnswerFormat {
        AnswerFormat::RawText
    }
}

// ── Standard arg builder ──────────────────────────────────────────────────────

/// Build the standard argument list used by most coding-agent CLIs:
///
/// `[prefix_args…] [auto_approve_flag] [--model <model>] [prompt_flag] <prompt>`
///
/// When `prompt_flag` is `None`, the prompt is appended as the last positional
/// argument. When `inject_stdin` is `true`, the prompt is NOT added to args
/// (it will be written to stdin post-startup — see `ExecutionMode::StdinInjection`).
#[allow(clippy::too_many_arguments)]
pub fn standard_args(
    prefix_args: &[&str],
    auto_approve_flag: Option<&str>,
    model_flag: Option<&str>,
    model: Option<&str>,
    prompt_flag: Option<&str>,
    prompt: &str,
    inject_stdin: bool,
) -> Vec<String> {
    let mut args: Vec<String> = prefix_args.iter().map(|s| s.to_string()).collect();

    if let Some(flag) = auto_approve_flag {
        args.push(flag.to_string());
    }

    if let (Some(flag), Some(m)) = (model_flag, model) {
        if !m.is_empty() {
            args.push(flag.to_string());
            args.push(m.to_string());
        }
    }

    if !inject_stdin {
        match prompt_flag {
            Some(flag) if !flag.is_empty() => {
                args.push(flag.to_string());
                args.push(prompt.to_string());
            }
            _ => {
                args.push(prompt.to_string());
            }
        }
    }

    args
}

// ── String intern helper ──────────────────────────────────────────────────────

/// Intern `id` into a `&'static str`, leaking at most once per unique string.
///
/// `GenericAdapter` needs `&'static str` to satisfy the `CodingAgentAdapter::id`
/// trait bound.  Without interning, every `from_config` call would leak a new
/// allocation.  The intern table is bounded by the number of distinct custom
/// agent IDs in `grove.yaml` (typically < 20 short strings).
fn intern_id(id: String) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static TABLE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut map = TABLE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    if let Some(&s) = map.get(&id) {
        return s;
    }
    let leaked: &'static str = Box::leak(id.clone().into_boxed_str());
    map.insert(id, leaked);
    leaked
}

// ── Generic fallback adapter ──────────────────────────────────────────────────

/// Fallback adapter for custom agents defined only in `grove.yaml` with no
/// dedicated built-in adapter.  Constructed directly from `CodingAgentConfig`
/// fields so arbitrary agents can still be driven through Grove.
pub struct GenericAdapter {
    pub id: &'static str, // Box::leak'd at construction time
    pub command: String,
    pub auto_approve_flag: Option<String>,
    pub prompt_flag: Option<String>,
    pub keystroke_injection: bool,
    pub use_pty: bool,
    pub prefix_args: Vec<String>,
    pub model_flag: Option<String>,
}

impl GenericAdapter {
    /// Construct from `CodingAgentConfig` fields.
    /// `id` is leaked to satisfy `&'static str` — acceptable since agents are
    /// created at most once per run.
    #[allow(clippy::too_many_arguments)]
    pub fn from_config(
        id: impl Into<String>,
        command: impl Into<String>,
        auto_approve_flag: Option<String>,
        prompt_flag: Option<String>,
        keystroke_injection: bool,
        use_pty: bool,
        prefix_args: Vec<String>,
        model_flag: Option<String>,
    ) -> Self {
        let id_static: &'static str = intern_id(id.into());
        Self {
            id: id_static,
            command: command.into(),
            auto_approve_flag,
            prompt_flag,
            keystroke_injection,
            use_pty,
            prefix_args,
            model_flag,
        }
    }
}

impl CodingAgentAdapter for GenericAdapter {
    fn id(&self) -> &'static str {
        self.id
    }

    fn default_command(&self) -> &str {
        &self.command
    }

    fn execution_mode(&self) -> ExecutionMode {
        if self.keystroke_injection {
            ExecutionMode::StdinInjection
        } else if self.use_pty {
            ExecutionMode::Pty
        } else {
            ExecutionMode::Pipe
        }
    }

    fn build_args(&self, model: Option<&str>, prompt: &str) -> Vec<String> {
        let prefix: Vec<&str> = self.prefix_args.iter().map(String::as_str).collect();
        standard_args(
            &prefix,
            self.auto_approve_flag.as_deref(),
            self.model_flag.as_deref(),
            model,
            self.prompt_flag.as_deref(),
            prompt,
            self.keystroke_injection,
        )
    }

    fn process_output(&self, raw: String) -> String {
        // PTY output may contain ANSI escape sequences; strip them for readability.
        if self.use_pty {
            super::strip_ansi(&raw)
        } else {
            raw
        }
    }
}
