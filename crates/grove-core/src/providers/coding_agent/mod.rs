use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub mod adapter;
mod aider;
mod amp;
mod auggie;
mod cline;
mod codex;
mod continue_agent;
mod copilot;
mod cursor;
mod gemini;
mod goose;
mod kilocode;
mod kimi;
mod kiro;
mod opencode;
mod qwen_code;

pub use adapter::{CodingAgentAdapter, ExecutionMode, GenericAdapter};

use super::claude_code::{apply_resource_limits, collect_capped};
use super::timeout;
use super::{
    Provider, ProviderRequest, ProviderResponse, SessionContinuityPolicy, StreamOutputEvent,
    StreamSink,
};
use crate::errors::{GroveError, GroveResult};
use crate::orchestrator::abort_handle::AbortHandle;

// ── Token filter shim env ────────────────────────────────────────────────────

/// Environment variables for the token filter shim, computed once and passed
/// into the `run_with_*` functions.
#[derive(Clone)]
struct FilterShimEnv {
    /// PATH with shim dir prepended.
    path: String,
    /// Path to `.grove-filter-state.json`.
    state_file: String,
    /// Path to `.grove-filter-bin/` directory.
    bin_dir: String,
}

/// Set up token filter shim if available, returning effective PATH and env vars.
fn prepare_filter_shim(worktree: &str, run_id: &str, model: Option<&str>) -> Option<FilterShimEnv> {
    use std::path::Path;

    let shim_setup =
        crate::token_filter::shim::setup(Path::new(worktree), run_id, model.unwrap_or(""), None)?;

    let effective_path = format!(
        "{}:{}",
        shim_setup.shim_dir.display(),
        crate::capability::shell_path()
    );

    Some(FilterShimEnv {
        path: effective_path,
        state_file: shim_setup.state_file.to_string_lossy().to_string(),
        bin_dir: shim_setup.shim_dir.to_string_lossy().to_string(),
    })
}

/// Apply filter env vars to a `Command`, or fall back to default PATH.
fn apply_filter_env(cmd: &mut Command, filter_env: &Option<FilterShimEnv>) {
    match filter_env {
        Some(env) => {
            cmd.env("PATH", &env.path);
            cmd.env("GROVE_FILTER_STATE", &env.state_file);
            cmd.env("GROVE_FILTER_BIN_DIR", &env.bin_dir);
        }
        None => {
            cmd.env("PATH", crate::capability::shell_path());
        }
    }
}

// ── Adapter registry ──────────────────────────────────────────────────────────

/// Return a boxed adapter for the given agent ID, or `None` for unknown agents.
///
/// To register a new built-in agent: add its adapter module above and a match
/// arm here.  Everything else (execution, output processing) is handled by
/// `CodingAgentProvider` using the adapter's trait methods.
pub fn get_adapter(id: &str) -> Option<Box<dyn CodingAgentAdapter>> {
    match id {
        "codex" => Some(Box::new(codex::CodexAdapter)),
        "gemini" => Some(Box::new(gemini::GeminiAdapter)),
        "aider" => Some(Box::new(aider::AiderAdapter)),
        "cursor" => Some(Box::new(cursor::CursorAdapter)),
        "copilot" => Some(Box::new(copilot::CopilotAdapter)),
        "qwen_code" => Some(Box::new(qwen_code::QwenCodeAdapter)),
        "opencode" => Some(Box::new(opencode::OpenCodeAdapter)),
        "kimi" => Some(Box::new(kimi::KimiAdapter)),
        "amp" => Some(Box::new(amp::AmpAdapter)),
        "goose" => Some(Box::new(goose::GooseAdapter)),
        "cline" => Some(Box::new(cline::ClineAdapter)),
        "continue" => Some(Box::new(continue_agent::ContinueAdapter)),
        "kiro" => Some(Box::new(kiro::KiroAdapter)),
        "auggie" => Some(Box::new(auggie::AuggieAdapter)),
        "kilocode" => Some(Box::new(kilocode::KilocodeAdapter)),
        _ => None,
    }
}

// ── Provider struct ───────────────────────────────────────────────────────────

/// A `Provider` implementation that drives any coding-agent CLI.
///
/// Each agent has a dedicated [`CodingAgentAdapter`] that encodes its exact
/// invocation contract (CLI flags, execution mode, output processing).
/// `CodingAgentProvider` is the shared execution engine that delegates to the
/// adapter for all agent-specific behaviour.
#[derive(Debug)]
pub struct CodingAgentProvider {
    adapter: Box<dyn CodingAgentAdapter>,
    /// Resolved command (adapter default or `grove.yaml` override).
    command: String,
    /// Wall-clock timeout for a single invocation in seconds.
    timeout_secs: u64,
    /// Maximum bytes of stdout to collect. Process is killed if exceeded.
    max_output_bytes: usize,
    /// RLIMIT_FSIZE: maximum file size in MiB the agent may write. Unix-only.
    max_file_size_mb: Option<u32>,
    /// RLIMIT_NOFILE: maximum open file descriptors for the agent. Unix-only.
    max_open_files: Option<u32>,
    abort_handle: Mutex<Option<AbortHandle>>,
}

impl CodingAgentProvider {
    /// Construct a new provider.
    ///
    /// `command` is the resolved binary path — use `adapter.default_command()`
    /// as the default, overridden by the user's `grove.yaml` `command` field.
    pub fn new(
        adapter: Box<dyn CodingAgentAdapter>,
        command: impl Into<String>,
        timeout_secs: u64,
    ) -> Self {
        Self {
            adapter,
            command: command.into(),
            timeout_secs,
            max_output_bytes: 10 * 1024 * 1024,
            max_file_size_mb: None,
            max_open_files: None,
            abort_handle: Mutex::new(None),
        }
    }

    pub fn with_max_output_bytes(mut self, max_output_bytes: usize) -> Self {
        self.max_output_bytes = max_output_bytes;
        self
    }

    /// Set RLIMIT_FSIZE and RLIMIT_NOFILE for spawned agent processes (Unix-only).
    pub fn with_resource_limits(
        mut self,
        max_file_size_mb: Option<u32>,
        max_open_files: Option<u32>,
    ) -> Self {
        self.max_file_size_mb = max_file_size_mb;
        self.max_open_files = max_open_files;
        self
    }
}

impl Provider for CodingAgentProvider {
    fn name(&self) -> &'static str {
        self.adapter.id()
    }

    fn persistent_phase_provider(&self) -> Option<&dyn super::PersistentPhaseProvider> {
        Some(self)
    }

    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        // Use resume args when a prior session ID is available (e.g. codex resume).
        // Fall back to normal build_args when the adapter doesn't support resumption.
        let args = if let Some(ref sid) = request.provider_session_id {
            self.adapter.build_resume_args(sid).unwrap_or_else(|| {
                self.adapter
                    .build_args(request.model.as_deref(), &request.instructions)
            })
        } else {
            self.adapter
                .build_args(request.model.as_deref(), &request.instructions)
        };
        let prompt = request.instructions.clone(); // needed for StdinInjection
        let command = self.command.clone();
        let worktree = request.worktree_path.clone();
        let role = request.role.clone();
        let max_output_bytes = self.max_output_bytes;
        let max_file_size_mb = self.max_file_size_mb;
        let max_open_files = self.max_open_files;
        let effective_timeout_secs = request.timeout_override.unwrap_or(self.timeout_secs);
        let timeout = Duration::from_secs(effective_timeout_secs);
        let execution_mode = self.adapter.execution_mode();

        let child_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
        let child_pid_thread = Arc::clone(&child_pid);

        let abort_handle = self.abort_handle.lock().unwrap().clone();
        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }
        let abort_for_closure = abort_handle.clone();

        let log_dir = request.log_dir.clone();
        let grove_session_id = request.grove_session_id.clone();
        let input_handle_cb = request.input_handle_callback.clone();
        let mcp_config_path = request.mcp_config_path.clone();

        // Set up token filter shim (best-effort — None if grove-filter not found).
        let filter_env =
            prepare_filter_shim(&worktree, &request.objective, request.model.as_deref());

        let result = timeout::with_timeout(timeout, move || {
            let log_file = match (&log_dir, &grove_session_id) {
                (Some(dir), Some(sid)) => super::claude_code::open_log_file(dir, sid),
                _ => None,
            };

            match execution_mode {
                ExecutionMode::Pty => run_with_pty(
                    &command,
                    &args,
                    &worktree,
                    &role,
                    max_output_bytes,
                    &child_pid_thread,
                    abort_for_closure.as_ref(),
                    log_file,
                    input_handle_cb.as_ref(),
                    &filter_env,
                ),
                ExecutionMode::Pipe => run_with_pipe(
                    &command,
                    &args,
                    &worktree,
                    &role,
                    max_output_bytes,
                    max_file_size_mb,
                    max_open_files,
                    false,
                    String::new(),
                    &child_pid_thread,
                    abort_for_closure.as_ref(),
                    log_file,
                    input_handle_cb.as_ref(),
                    mcp_config_path.as_deref(),
                    &filter_env,
                ),
                ExecutionMode::StdinInjection => run_with_pipe(
                    &command,
                    &args,
                    &worktree,
                    &role,
                    max_output_bytes,
                    max_file_size_mb,
                    max_open_files,
                    true,
                    prompt,
                    &child_pid_thread,
                    abort_for_closure.as_ref(),
                    log_file,
                    input_handle_cb.as_ref(),
                    mcp_config_path.as_deref(),
                    &filter_env,
                ),
            }
        });

        // Kill orphaned subprocess if the timeout fired.
        if result.is_err() {
            if let Some(pid) = *child_pid.lock().unwrap() {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            }
        }

        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }

        // Delegate to adapter's parse_output for structured extraction (session
        // ID, summary text) or plain post-processing (ANSI stripping, etc.).
        // `parse_output` may return Err for explicit agent failures (e.g. codex
        // turn.failed), which propagates so retry logic fires correctly.
        result.and_then(|resp| {
            let (summary, session_id) = self.adapter.parse_output(resp.summary)?;
            Ok(ProviderResponse {
                summary,
                provider_session_id: session_id.or(resp.provider_session_id),
                ..resp
            })
        })
    }

    fn execute_streaming(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
    ) -> GroveResult<ProviderResponse> {
        // Coding agents don't have structured streaming. Run the normal execute()
        // and emit a RawLine for each line of the summary, plus a Result at the end.
        let response = self.execute(request)?;

        // Emit each line as both RawLine and AssistantText so the frontend can
        // render structured messages. Coding agents don't stream token-by-token;
        // we emit per line after completion.
        for line in response.summary.lines() {
            sink.on_event(StreamOutputEvent::RawLine {
                line: line.to_string(),
            });
            if !line.trim().is_empty() {
                sink.on_event(StreamOutputEvent::AssistantText {
                    text: line.to_string(),
                });
            }
        }

        // Emit the final Result event.
        sink.on_event(StreamOutputEvent::Result {
            text: response.summary.clone(),
            cost_usd: response.cost_usd,
            is_error: false,
            session_id: response.provider_session_id.clone(),
        });

        Ok(response)
    }

    fn execute_interactive(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
        qa_source: &dyn super::QaSource,
    ) -> GroveResult<ProviderResponse> {
        // Only use interactive path when the adapter opts in.
        if !self.adapter.supports_interactive() {
            return self.execute_streaming(request, sink);
        }

        let args = if let Some(ref sid) = request.provider_session_id {
            self.adapter.build_resume_args(sid).unwrap_or_else(|| {
                self.adapter
                    .build_args(request.model.as_deref(), &request.instructions)
            })
        } else {
            self.adapter
                .build_args(request.model.as_deref(), &request.instructions)
        };
        let prompt = request.instructions.clone();
        let command = self.command.clone();
        let worktree = request.worktree_path.clone();
        let role = request.role.clone();
        let run_id = request.objective.clone();
        let max_output_bytes = self.max_output_bytes;
        let max_file_size_mb = self.max_file_size_mb;
        let max_open_files = self.max_open_files;
        let effective_timeout_secs = request.timeout_override.unwrap_or(self.timeout_secs);
        let timeout = Duration::from_secs(effective_timeout_secs);
        let execution_mode = self.adapter.execution_mode();
        let answer_format = self.adapter.answer_format();
        let log_dir = request.log_dir.clone();
        let grove_session_id = request.grove_session_id.clone();
        let mcp_config_path = request.mcp_config_path.clone();

        // Set up token filter shim (best-effort).
        let filter_env =
            prepare_filter_shim(&worktree, &request.objective, request.model.as_deref());

        let child_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
        let child_pid_thread = Arc::clone(&child_pid);

        let abort_handle = self.abort_handle.lock().unwrap().clone();
        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }
        let abort_for_closure = abort_handle.clone();

        // SAFETY: with_timeout blocks until the closure finishes.
        let sink_ptr = SinkPtr::from_ref(sink);
        let qa_ptr = QaSourcePtr::from_ref(qa_source);

        let result = timeout::with_timeout(timeout, move || {
            let sink: &dyn StreamSink = unsafe { sink_ptr.as_ref() };
            let qa: &dyn super::QaSource = unsafe { qa_ptr.as_ref() };

            let log_file = match (&log_dir, &grove_session_id) {
                (Some(dir), Some(sid)) => super::claude_code::open_log_file(dir, sid),
                _ => None,
            };

            run_with_pipe_interactive(
                &command,
                &args,
                &worktree,
                &role,
                &run_id,
                max_output_bytes,
                max_file_size_mb,
                max_open_files,
                execution_mode == adapter::ExecutionMode::StdinInjection,
                prompt,
                &child_pid_thread,
                abort_for_closure.as_ref(),
                log_file,
                sink,
                qa,
                answer_format,
                mcp_config_path.as_deref(),
                &filter_env,
            )
        });

        if result.is_err() {
            if let Some(pid) = *child_pid.lock().unwrap() {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            }
        }

        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }

        result.and_then(|resp| {
            let (summary, session_id) = self.adapter.parse_output(resp.summary)?;
            Ok(ProviderResponse {
                summary,
                provider_session_id: session_id.or(resp.provider_session_id),
                ..resp
            })
        })
    }

    fn set_abort_handle(&self, handle: AbortHandle) {
        *self.abort_handle.lock().unwrap() = Some(handle);
    }

    fn session_continuity_policy(&self) -> SessionContinuityPolicy {
        self.adapter.session_continuity_policy()
    }
}

impl super::PersistentPhaseProvider for CodingAgentProvider {
    fn start_host(
        &self,
        run_id: &str,
        worktree_path: &str,
        model: Option<&str>,
        _allowed_tools: Option<&[String]>,
        log_dir: Option<&str>,
        mcp_config_path: Option<&str>,
    ) -> crate::errors::GroveResult<super::claude_code_persistent::PersistentHost> {
        Ok(super::claude_code_persistent::PersistentHost::idle(
            run_id.to_string(),
            worktree_path.to_string(),
            model.map(|m| m.to_string()),
            None,
            log_dir.map(|d| d.to_string()),
            mcp_config_path.map(|p| p.to_string()),
        ))
    }

    fn execute_persistent_turn(
        &self,
        host: &mut super::claude_code_persistent::PersistentHost,
        turn: &super::claude_code_persistent::PhaseTurn,
        sink: &dyn super::StreamSink,
        qa_source: &dyn super::QaSource,
        grove_session_id: &str,
    ) -> crate::errors::GroveResult<super::claude_code_persistent::PhaseTurnOutcome> {
        // Each coding-agent phase is an independent CLI invocation. Do NOT carry a
        // thread_id from a previous phase — doing so causes agents like codex to call
        // `exec resume <prior_thread_id>` without a prompt, which fails immediately.
        // The thread_id is only valid for the initial DetachedResume case (explicitly
        // set in RunOptions.resume_provider_session_id before the first phase runs).
        let phase_changed = host.current_phase.as_deref() != Some(turn.phase.as_str());
        if phase_changed {
            host.provider_thread_id = None;
        }

        host.current_phase = Some(turn.phase.clone());
        let _ = host.transition(super::claude_code_persistent::HostState::Running);

        let instructions = super::claude_code_persistent::format_phase_prompt(turn);
        let request = super::ProviderRequest {
            objective: host.run_id.clone(),
            role: turn.phase.clone(),
            worktree_path: host.worktree_path.clone(),
            instructions,
            model: host.model.clone(),
            allowed_tools: host.allowed_tools.clone(),
            timeout_override: None,
            provider_session_id: host.provider_thread_id.clone(),
            log_dir: host.log_dir.clone(),
            grove_session_id: Some(grove_session_id.to_string()),
            input_handle_callback: None,
            mcp_config_path: host.mcp_config_path.clone(),
            conversation_id: None,
        };

        let result = self.execute_interactive(&request, sink, qa_source);

        match result {
            Ok(response) => {
                if let Some(cost) = response.cost_usd {
                    host.add_cost(cost);
                }
                if let Some(ref pid) = response.pid {
                    host.pid = Some(*pid);
                }
                host.turn_count += 1;
                let grove_control =
                    super::claude_code_persistent::extract_grove_control_block(&response.summary);
                Ok(super::claude_code_persistent::PhaseTurnOutcome::TurnDone {
                    response_text: response.summary,
                    cost_usd: response.cost_usd,
                    session_id: response.provider_session_id,
                    grove_control,
                })
            }
            Err(e) => Err(e),
        }
    }

    fn abort_host(
        &self,
        host: &mut super::claude_code_persistent::PersistentHost,
    ) -> crate::errors::GroveResult<()> {
        host.shutdown(super::claude_code_persistent::HostState::Aborted)
    }
}

// ── Fat-pointer helpers for passing borrowed trait references into closures ──
// Same trick as claude_code.rs: fat pointer stored as [usize; 2] for Send + 'static.
// SAFETY: the closure is joined before the borrow expires.

#[derive(Clone, Copy)]
struct SinkPtr([usize; 2]);

impl SinkPtr {
    fn from_ref(sink: &dyn StreamSink) -> Self {
        let raw: [usize; 2] = unsafe { std::mem::transmute(sink as *const dyn StreamSink) };
        Self(raw)
    }
    unsafe fn as_ref(&self) -> &dyn StreamSink {
        unsafe {
            let ptr: *const dyn StreamSink = std::mem::transmute(self.0);
            &*ptr
        }
    }
}

#[derive(Clone, Copy)]
struct QaSourcePtr([usize; 2]);

impl QaSourcePtr {
    fn from_ref(qa: &dyn super::QaSource) -> Self {
        let raw: [usize; 2] = unsafe { std::mem::transmute(qa as *const dyn super::QaSource) };
        Self(raw)
    }
    unsafe fn as_ref(&self) -> &dyn super::QaSource {
        unsafe {
            let ptr: *const dyn super::QaSource = std::mem::transmute(self.0);
            &*ptr
        }
    }
}

// SAFETY: The raw usize arrays don't reference memory directly — they're
// reconstituted only while the original reference is alive.
unsafe impl Send for SinkPtr {}
unsafe impl Send for QaSourcePtr {}

// ── Standard pipe execution ───────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_with_pipe(
    command: &str,
    args: &[String],
    worktree: &str,
    role: &str,
    max_output_bytes: usize,
    max_file_size_mb: Option<u32>,
    max_open_files: Option<u32>,
    inject_stdin: bool,
    prompt: String,
    child_pid: &Arc<Mutex<Option<u32>>>,
    abort_handle: Option<&AbortHandle>,
    log_file: Option<std::fs::File>,
    _input_handle_callback: Option<
        &Arc<dyn Fn(super::agent_input::AgentInputHandle) + Send + Sync>,
    >,
    mcp_config_path: Option<&str>,
    filter_env: &Option<FilterShimEnv>,
) -> GroveResult<ProviderResponse> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .current_dir(worktree)
        // Prevent nested Grove→claude invocations from acting on the CLAUDECODE
        // env var that Claude Code sets for its own sub-processes.
        .env_remove("CLAUDECODE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Apply token filter shim env vars (PATH + state file + bin dir), or default PATH.
    apply_filter_env(&mut cmd, filter_env);

    // Expose MCP config path as an environment variable for agents that
    // support MCP integration. Each agent adapter can read this to inject
    // provider-specific CLI flags.
    if let Some(mcp_path) = mcp_config_path {
        cmd.env("GROVE_MCP_CONFIG", mcp_path);
    }

    // Apply POSIX resource limits (best-effort, Unix-only).
    apply_resource_limits(&mut cmd, max_file_size_mb, max_open_files);

    if inject_stdin {
        cmd.stdin(Stdio::piped());
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| GroveError::Runtime(format!("failed to launch '{command}': {e}")))?;

    let pid = child.id();
    *child_pid.lock().unwrap() = Some(pid);

    let _abort_guard = abort_handle.map(|h| h.register_pid(pid));

    if inject_stdin {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.as_bytes());
            let _ = stdin.write_all(b"\n");
        }
    }

    // Capture stderr on a background thread so it's available even when
    // collect_capped errors (e.g. idle timeout). Without this, stderr is
    // lost on timeout because cap_result? propagates before the old
    // child.stderr.take() block.
    let stderr_handle = child.stderr.take().and_then(|se| {
        std::thread::Builder::new()
            .name(format!("{command}-stderr"))
            .spawn(move || {
                let mut buf = String::new();
                let mut reader = se;
                let _ = reader.read_to_string(&mut buf);
                buf
            })
            .ok()
    });

    let stdout = child.stdout.take().expect("stdout was piped");
    let reader = BufReader::new(stdout);
    // Collect output but do NOT propagate error yet — child.wait() must always
    // be called to reap the process and prevent zombies, even when collect_capped
    // kills the child because it exceeded the byte cap.
    let cap_result = collect_capped(reader, max_output_bytes, role, &mut child, log_file, None);

    let status = child
        .wait()
        .map_err(|e| GroveError::Runtime(format!("failed to wait for '{command}': {e}")))?;

    // Collect stderr from background thread (with short timeout to avoid blocking).
    let stderr_output = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    if !stderr_output.trim().is_empty() {
        tracing::warn!(
            pid = pid,
            command = %command,
            stderr = %stderr_output.trim(),
            "agent stderr output"
        );
    }

    // Propagate cap error (if any) — after the child has been reaped and
    // stderr captured. Include stderr in the error for diagnostics.
    if let Err(mut e) = cap_result {
        if !stderr_output.trim().is_empty() {
            e = GroveError::Runtime(format!("{e}\nstderr: {}", stderr_output.trim()));
        }
        return Err(e);
    }
    let summary = cap_result.unwrap();

    if !status.success() {
        let msg = stderr_output.trim();
        return Err(GroveError::Runtime(if msg.is_empty() {
            format!("'{command}' exited with status {status}")
        } else {
            format!("'{command}' exited with status {status}: {msg}")
        }));
    }

    Ok(ProviderResponse {
        summary,
        changed_files: vec![],
        cost_usd: None,
        provider_session_id: None,
        pid: *child_pid.lock().unwrap(),
    })
}

// ── Interactive pipe execution (for agents that may prompt) ──────────────────

/// Like `run_with_pipe`, but monitors stdout line-by-line for question patterns
/// and pipes answers back via stdin. Only used when the adapter declares
/// `supports_interactive() -> true`.
#[allow(clippy::too_many_arguments)]
fn run_with_pipe_interactive(
    command: &str,
    args: &[String],
    worktree: &str,
    role: &str,
    run_id: &str,
    max_output_bytes: usize,
    max_file_size_mb: Option<u32>,
    max_open_files: Option<u32>,
    inject_stdin: bool,
    prompt: String,
    child_pid: &Arc<Mutex<Option<u32>>>,
    abort_handle: Option<&AbortHandle>,
    mut log_file: Option<std::fs::File>,
    sink: &dyn StreamSink,
    qa_source: &dyn super::QaSource,
    answer_format: adapter::AnswerFormat,
    mcp_config_path: Option<&str>,
    filter_env: &Option<FilterShimEnv>,
) -> GroveResult<ProviderResponse> {
    use super::line_reader::{LineError, TimedLineReader};

    const IDLE_TIMEOUT_SECS: u64 = 300;
    const QUESTION_CONFIDENCE_THRESHOLD: f32 = 0.8;

    let mut cmd = Command::new(command);
    cmd.args(args)
        .current_dir(worktree)
        .env_remove("CLAUDECODE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Always pipe stdin for interactive agents — we may need to write answers.
        .stdin(Stdio::piped());

    // Apply token filter shim env vars (PATH + state file + bin dir), or default PATH.
    apply_filter_env(&mut cmd, filter_env);

    // Expose MCP config path as an environment variable for agents that
    // support MCP integration.
    if let Some(mcp_path) = mcp_config_path {
        cmd.env("GROVE_MCP_CONFIG", mcp_path);
    }

    apply_resource_limits(&mut cmd, max_file_size_mb, max_open_files);

    let mut child = cmd
        .spawn()
        .map_err(|e| GroveError::Runtime(format!("failed to launch '{command}': {e}")))?;

    let pid = child.id();
    *child_pid.lock().unwrap() = Some(pid);
    let _abort_guard = abort_handle.map(|h| h.register_pid(pid));

    // If StdinInjection mode, write the prompt to stdin first.
    if inject_stdin {
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(prompt.as_bytes());
            let _ = stdin.write_all(b"\n");
            let _ = stdin.flush();
        }
    }

    // Capture stderr on a background thread.
    let stderr_handle = child.stderr.take().and_then(|se| {
        std::thread::Builder::new()
            .name(format!("{command}-stderr-interactive"))
            .spawn(move || {
                let mut buf = String::new();
                let mut reader = se;
                let _ = reader.read_to_string(&mut buf);
                buf
            })
            .ok()
    });

    let stdout = child.stdout.take().expect("stdout was piped");
    let reader = BufReader::new(stdout);
    let role_upper = role.to_uppercase();
    let mut collected = String::new();

    let timed = TimedLineReader::new(reader, Duration::from_secs(IDLE_TIMEOUT_SECS));

    loop {
        match timed.next_line() {
            Ok(l) => {
                eprintln!("[{}] {}", role_upper, l);

                // Tee to log file.
                if let Some(ref mut f) = log_file {
                    let _ = writeln!(f, "{}", l);
                }

                // Emit raw line to frontend.
                sink.on_event(StreamOutputEvent::RawLine { line: l.clone() });

                collected.push_str(&l);
                collected.push('\n');
                if collected.len() > max_output_bytes {
                    let _ = child.kill();
                    return Err(GroveError::Runtime(format!(
                        "agent output exceeded cap of {} bytes; process killed",
                        max_output_bytes
                    )));
                }

                // Check for question patterns.
                if let Some(detected) = super::question_detector::detect_question(&l) {
                    if detected.confidence >= QUESTION_CONFIDENCE_THRESHOLD {
                        sink.on_event(StreamOutputEvent::Question {
                            question: detected.question.clone(),
                            options: detected.options.clone(),
                            blocking: true,
                        });

                        match qa_source.wait_for_answer(
                            run_id,
                            None, // no structured session_id for generic agents
                            &detected.question,
                            &detected.options,
                        ) {
                            Ok(answer) if !answer.is_empty() => {
                                if let Some(ref mut writer) = child.stdin {
                                    let write_result = match answer_format {
                                        adapter::AnswerFormat::RawText => writer
                                            .write_all(answer.as_bytes())
                                            .and_then(|_| writer.write_all(b"\n"))
                                            .and_then(|_| writer.flush()),
                                        adapter::AnswerFormat::Json => {
                                            let payload = serde_json::json!({
                                                "type": "user_input",
                                                "text": answer,
                                            });
                                            writeln!(writer, "{}", payload)
                                                .and_then(|_| writer.flush())
                                        }
                                    };
                                    if let Err(e) = write_result {
                                        tracing::warn!(
                                            error = %e,
                                            "failed to write Q&A answer to agent stdin"
                                        );
                                    }
                                }
                                sink.on_event(StreamOutputEvent::UserAnswer { text: answer });
                            }
                            Ok(_) => {
                                tracing::debug!("Q&A returned empty answer — skipping");
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    question = %detected.question,
                                    "Q&A wait_for_answer failed"
                                );
                            }
                        }
                    }
                }
            }
            Err(LineError::IdleTimeout) => {
                tracing::warn!(
                    "agent process idle — no output for {IDLE_TIMEOUT_SECS} seconds; killing"
                );
                let _ = child.kill();
                // Fall through to child.wait + stderr collection.
                break;
            }
            Err(LineError::Eof) | Err(LineError::Io(_)) => break,
        }
    }

    let status = child
        .wait()
        .map_err(|e| GroveError::Runtime(format!("failed to wait for '{command}': {e}")))?;

    let stderr_output = stderr_handle
        .and_then(|h| h.join().ok())
        .unwrap_or_default();
    if !stderr_output.trim().is_empty() {
        tracing::warn!(
            pid = pid,
            command = %command,
            stderr = %stderr_output.trim(),
            "agent stderr output (interactive)"
        );
    }

    if !status.success() {
        let msg = stderr_output.trim();
        return Err(GroveError::Runtime(if msg.is_empty() {
            format!("'{command}' exited with status {status}")
        } else {
            format!("'{command}' exited with status {status}: {msg}")
        }));
    }

    // Emit final result.
    sink.on_event(StreamOutputEvent::Result {
        text: collected.clone(),
        cost_usd: None,
        is_error: false,
        session_id: None,
    });

    Ok(ProviderResponse {
        summary: collected,
        changed_files: vec![],
        cost_usd: None,
        provider_session_id: None,
        pid: *child_pid.lock().unwrap(),
    })
}

// ── PTY execution (for agents that check isatty) ──────────────────────────────

#[allow(clippy::too_many_arguments)]
fn run_with_pty(
    command: &str,
    args: &[String],
    worktree: &str,
    role: &str,
    max_output_bytes: usize,
    child_pid: &Arc<Mutex<Option<u32>>>,
    abort_handle: Option<&AbortHandle>,
    mut log_file: Option<std::fs::File>,
    input_handle_callback: Option<&Arc<dyn Fn(super::agent_input::AgentInputHandle) + Send + Sync>>,
    filter_env: &Option<FilterShimEnv>,
) -> GroveResult<ProviderResponse> {
    use portable_pty::{CommandBuilder, PtySize, native_pty_system};

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 220,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| GroveError::Runtime(format!("failed to open PTY for '{command}': {e}")))?;

    let mut cmd = CommandBuilder::new(command);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.cwd(worktree);
    // Apply token filter shim env vars or default PATH.
    match filter_env {
        Some(env) => {
            cmd.env("PATH", &env.path);
            cmd.env("GROVE_FILTER_STATE", &env.state_file);
            cmd.env("GROVE_FILTER_BIN_DIR", &env.bin_dir);
        }
        None => {
            cmd.env("PATH", crate::capability::shell_path());
        }
    }
    // PTY merges stdout+stderr so we can't separate them, but we can at least
    // prevent the CLAUDECODE env var from propagating to nested invocations.
    cmd.env("CLAUDECODE", "");

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| GroveError::Runtime(format!("failed to launch '{command}' in PTY: {e}")))?;

    // Drop the slave end so the master reader reaches EOF when the child exits.
    drop(pair.slave);

    // Record PID for abort/kill support.
    if let Some(pid) = child.process_id() {
        *child_pid.lock().unwrap() = Some(pid);
        if let Some(h) = abort_handle {
            let _ = h.register_pid(pid);
        }
    }

    // Read from the PTY master. Raw output is returned and post-processed by
    // the adapter's `parse_output` method (ANSI stripping, structured parsing, etc.).
    let master_reader = pair.master.try_clone_reader().map_err(|e| {
        GroveError::Runtime(format!("failed to read PTY master for '{command}': {e}"))
    })?;

    // Register PTY master writer for answer write-back (Q&A support).
    if let Some(cb) = input_handle_callback {
        match pair.master.take_writer() {
            Ok(writer) => {
                cb(super::agent_input::AgentInputHandle::Pty(writer));
            }
            Err(e) => {
                tracing::debug!(
                    "PTY writer not available for '{command}': {e} — Q&A disabled for this agent"
                );
            }
        }
    }

    let reader = BufReader::new(master_reader);
    let role_upper = role.to_uppercase();
    let mut collected = String::new();
    // Rolling buffer of the last 20 lines for diagnostic error messages.
    let mut tail: VecDeque<String> = VecDeque::with_capacity(21);

    for line in reader.lines() {
        match line {
            Ok(raw) => {
                eprintln!("[{}] {}", role_upper, raw);
                // Tee raw line to log file (best-effort).
                if let Some(ref mut f) = log_file {
                    use std::io::Write as _;
                    let _ = writeln!(f, "{}", raw);
                }
                collected.push_str(&raw);
                collected.push('\n');
                if tail.len() == 20 {
                    tail.pop_front();
                }
                tail.push_back(raw);
                if collected.len() > max_output_bytes {
                    let _ = child.kill();
                    return Err(GroveError::Runtime(format!(
                        "agent output exceeded cap of {} bytes; process killed",
                        max_output_bytes
                    )));
                }
            }
            Err(_) => break, // EOF — child exited
        }
    }

    let status = child
        .wait()
        .map_err(|e| GroveError::Runtime(format!("failed to wait for '{command}': {e}")))?;

    if !status.success() {
        let tail_text: String = tail.iter().cloned().collect::<Vec<_>>().join("\n");
        let detail = if tail_text.trim().is_empty() {
            String::new()
        } else {
            format!("\nLast output:\n{tail_text}")
        };
        return Err(GroveError::Runtime(format!(
            "'{command}' exited with non-zero status{detail}"
        )));
    }

    Ok(ProviderResponse {
        summary: collected,
        changed_files: vec![],
        cost_usd: None,
        provider_session_id: None,
        pid: *child_pid.lock().unwrap(),
    })
}

// ── ANSI escape sequence stripper ─────────────────────────────────────────────

/// Remove ANSI/VT100 escape sequences from a string.
/// Used by adapters whose agents emit ANSI codes (e.g. codex via PTY).
/// Handles CSI sequences (`ESC[…m`), OSC sequences (`ESC]…BEL`), and lone ESC.
pub(crate) fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // CSI: skip until a letter (final byte of the sequence).
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            Some(']') => {
                // OSC: skip until BEL (`\x07`) or the ST sequence (`ESC\`).
                loop {
                    match chars.next() {
                        Some('\x07') | None => break,
                        Some('\x1b') => {
                            chars.next(); // consume the `\` of ST
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Some('(') | Some(')') => {
                // Character set designation — skip one more byte.
                chars.next();
            }
            _ => {} // lone ESC or unknown sequence — drop it
        }
    }
    result
}
