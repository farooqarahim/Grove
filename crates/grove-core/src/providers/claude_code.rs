use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use super::gates::{self, GateDecision, PermissionRequest};
use super::line_reader::{LineError, TimedLineReader};
use super::stream_parser::{self, StreamEvent, StreamResult};
use super::timeout;
use super::{
    Provider, ProviderRequest, ProviderResponse, QaSource, SessionContinuityPolicy,
    StreamOutputEvent, StreamSink,
};

/// Set up token filter shim and return the effective PATH (with shim dir prepended)
/// plus the env vars to inject. Returns `None` if grove-filter binary is unavailable.
fn prepare_claude_filter_env(
    worktree: &str,
    run_id: &str,
    model: Option<&str>,
) -> Option<(String, String, String)> {
    let shim_setup = crate::token_filter::shim::setup(
        std::path::Path::new(worktree),
        run_id,
        model.unwrap_or(""),
        None,
    )?;
    let path = format!(
        "{}:{}",
        shim_setup.shim_dir.display(),
        crate::capability::shell_path()
    );
    Some((
        path,
        shim_setup.state_file.to_string_lossy().to_string(),
        shim_setup.shim_dir.to_string_lossy().to_string(),
    ))
}

/// Apply token filter env vars to a `Command`, or fall back to default PATH.
fn apply_claude_filter_env(cmd: &mut Command, filter_env: &Option<(String, String, String)>) {
    match filter_env {
        Some((path, state_file, bin_dir)) => {
            cmd.env("PATH", path);
            cmd.env("GROVE_FILTER_STATE", state_file);
            cmd.env("GROVE_FILTER_BIN_DIR", bin_dir);
        }
        None => {
            cmd.env("PATH", crate::capability::shell_path());
        }
    }
}

/// Default idle timeout for per-line reads (10 minutes).
/// If the agent produces no stdout AND no filesystem/session-log activity for
/// this long, it is considered stuck and killed.
const STDOUT_IDLE_TIMEOUT_SECS: u64 = 600;
use crate::config::PermissionMode;
use crate::errors::{GroveError, GroveResult};
use crate::orchestrator::abort_handle::AbortHandle;

/// Wrapper around a fat pointer to `dyn StreamSink` that enables passing a
/// borrowed sink reference into a `Send + 'static` closure. This is safe
/// when the closure is joined before the borrow expires (e.g.
/// `timeout::with_timeout` blocks on the background thread).
///
/// Uses `[usize; 2]` to store the fat pointer (data + vtable) as a plain value
/// that is both `Send` and `'static`.
#[derive(Clone, Copy)]
struct SinkPtr([usize; 2]);

impl SinkPtr {
    fn from_ref(sink: &dyn StreamSink) -> Self {
        // SAFETY: A trait object reference is a fat pointer (2 usizes).
        let raw: [usize; 2] = unsafe { std::mem::transmute(sink as *const dyn StreamSink) };
        Self(raw)
    }

    /// Reconstruct the trait-object reference.
    ///
    /// # Safety
    /// The original reference must still be alive when this is called.
    unsafe fn as_ref(&self) -> &dyn StreamSink {
        unsafe {
            let ptr: *const dyn StreamSink = std::mem::transmute(self.0);
            &*ptr
        }
    }
}

/// Same fat-pointer trick as [`SinkPtr`] but for `&dyn QaSource`.
///
/// # Safety
/// The original reference must still be alive when `as_ref` is called.
#[derive(Clone, Copy)]
struct QaSourcePtr([usize; 2]);

impl QaSourcePtr {
    fn from_ref(qa: &dyn QaSource) -> Self {
        let raw: [usize; 2] = unsafe { std::mem::transmute(qa as *const dyn QaSource) };
        Self(raw)
    }

    unsafe fn as_ref(&self) -> &dyn QaSource {
        unsafe {
            let ptr: *const dyn QaSource = std::mem::transmute(self.0);
            &*ptr
        }
    }
}

#[derive(Debug)]
pub struct ClaudeCodeProvider {
    pub command: String,
    pub timeout_secs: u64,
    pub permission_mode: PermissionMode,
    /// Seed set of pre-approved tools for `HumanGate` / `AutonomousGate` modes.
    pub allowed_tools: Vec<String>,
    /// Model used by the autonomous gatekeeper (defaults to Haiku if `None`).
    pub gatekeeper_model: Option<String>,
    /// Maximum bytes of stdout collected per invocation. Default: 10 MiB.
    pub max_output_bytes: usize,
    /// RLIMIT_FSIZE: maximum file size in MiB the agent may write. Unix-only.
    pub max_file_size_mb: Option<u32>,
    /// RLIMIT_NOFILE: maximum open file descriptors for the agent. Unix-only.
    pub max_open_files: Option<u32>,
    /// Abort handle for subprocess termination. Set via `set_abort_handle`.
    abort_handle: Mutex<Option<AbortHandle>>,
}

impl ClaudeCodeProvider {
    pub fn new(
        command: impl Into<String>,
        timeout_secs: u64,
        permission_mode: PermissionMode,
        allowed_tools: Vec<String>,
        gatekeeper_model: Option<String>,
    ) -> Self {
        Self {
            command: command.into(),
            timeout_secs,
            permission_mode,
            allowed_tools,
            gatekeeper_model,
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

/// Subset of the JSON output produced by `claude --output-format json`.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ClaudeOutput {
    #[serde(default)]
    result: String,
    #[serde(default)]
    cost_usd: Option<f64>,
    #[serde(default)]
    is_error: bool,
}

impl Provider for ClaudeCodeProvider {
    fn name(&self) -> &'static str {
        "claude_code"
    }

    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        // Fast path: skip all permission checks.
        if self.permission_mode == PermissionMode::SkipAll {
            return self.run_once(request, None);
        }

        // HumanGate / AutonomousGate: retry loop with per-attempt tool escalation.
        // Per-agent allowed_tools (from AgentType::allowed_tools()) takes precedence over
        // the provider-level config when set.
        let mut current_allowed_tools = request
            .allowed_tools
            .clone()
            .unwrap_or_else(|| self.allowed_tools.clone());

        for attempt in 0..=3usize {
            let response = self.run_once(request, Some(&current_allowed_tools))?;

            if let Some(perm_req) = detect_permission_request(&response.summary) {
                if attempt >= 3 {
                    return Err(GroveError::Runtime(
                        "permission retry limit reached (3 retries)".into(),
                    ));
                }

                let decision = match self.permission_mode {
                    PermissionMode::HumanGate => gates::human_gate_prompt(&perm_req),
                    PermissionMode::AutonomousGate => gates::gatekeeper_agent(
                        &perm_req,
                        &request.objective,
                        &request.role,
                        &response.summary,
                        self.gatekeeper_model.as_deref(),
                        &self.command,
                    )?,
                    PermissionMode::SkipAll => unreachable!(),
                };

                match decision {
                    GateDecision::AllowOnce | GateDecision::AllowAlways => {
                        eprintln!(
                            "[PERMISSION] granted: {} (attempt {}/3)",
                            perm_req.tool,
                            attempt + 1
                        );
                        current_allowed_tools.push(perm_req.tool);
                    }
                    GateDecision::Deny => {
                        return Err(GroveError::Runtime(format!(
                            "permission denied for tool '{}': {}",
                            perm_req.tool, perm_req.reason
                        )));
                    }
                    GateDecision::Abort => {
                        return Err(GroveError::Runtime(
                            "run aborted by user at permission gate".into(),
                        ));
                    }
                }
            } else {
                return Ok(response);
            }
        }

        Err(GroveError::Runtime("permission retry limit reached".into()))
    }

    fn execute_streaming(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
    ) -> GroveResult<ProviderResponse> {
        // Fast path: skip all permission checks.
        if self.permission_mode == PermissionMode::SkipAll {
            return self.run_once_streaming(request, None, sink);
        }

        // HumanGate / AutonomousGate: retry loop with per-attempt tool escalation.
        let mut current_allowed_tools = request
            .allowed_tools
            .clone()
            .unwrap_or_else(|| self.allowed_tools.clone());

        for attempt in 0..=3usize {
            let response = self.run_once_streaming(request, Some(&current_allowed_tools), sink)?;

            if let Some(perm_req) = detect_permission_request(&response.summary) {
                if attempt >= 3 {
                    return Err(GroveError::Runtime(
                        "permission retry limit reached (3 retries)".into(),
                    ));
                }

                let decision = match self.permission_mode {
                    PermissionMode::HumanGate => gates::human_gate_prompt(&perm_req),
                    PermissionMode::AutonomousGate => gates::gatekeeper_agent(
                        &perm_req,
                        &request.objective,
                        &request.role,
                        &response.summary,
                        self.gatekeeper_model.as_deref(),
                        &self.command,
                    )?,
                    PermissionMode::SkipAll => unreachable!(),
                };

                match decision {
                    GateDecision::AllowOnce | GateDecision::AllowAlways => {
                        eprintln!(
                            "[PERMISSION] granted: {} (attempt {}/3)",
                            perm_req.tool,
                            attempt + 1
                        );
                        current_allowed_tools.push(perm_req.tool);
                    }
                    GateDecision::Deny => {
                        return Err(GroveError::Runtime(format!(
                            "permission denied for tool '{}': {}",
                            perm_req.tool, perm_req.reason
                        )));
                    }
                    GateDecision::Abort => {
                        return Err(GroveError::Runtime(
                            "run aborted by user at permission gate".into(),
                        ));
                    }
                }
            } else {
                return Ok(response);
            }
        }

        Err(GroveError::Runtime("permission retry limit reached".into()))
    }

    fn execute_interactive(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
        qa_source: &dyn QaSource,
    ) -> GroveResult<ProviderResponse> {
        // Fast path: skip all permission checks.
        // Use run_once_streaming (--print one-shot mode) since SkipAll has no Q&A.
        // run_once_interactive (no --print) hangs because Claude CLI in conversational
        // mode expects TTY-like interaction that doesn't work with piped stdin.
        if self.permission_mode == PermissionMode::SkipAll {
            return self.run_once_streaming(request, None, sink);
        }

        // HumanGate / AutonomousGate: retry loop with per-attempt tool escalation.
        let mut current_allowed_tools = request
            .allowed_tools
            .clone()
            .unwrap_or_else(|| self.allowed_tools.clone());

        for attempt in 0..=3usize {
            let response =
                self.run_once_interactive(request, Some(&current_allowed_tools), sink, qa_source)?;

            if let Some(perm_req) = detect_permission_request(&response.summary) {
                if attempt >= 3 {
                    return Err(GroveError::Runtime(
                        "permission retry limit reached (3 retries)".into(),
                    ));
                }

                let decision = match self.permission_mode {
                    PermissionMode::HumanGate => gates::human_gate_prompt(&perm_req),
                    PermissionMode::AutonomousGate => gates::gatekeeper_agent(
                        &perm_req,
                        &request.objective,
                        &request.role,
                        &response.summary,
                        self.gatekeeper_model.as_deref(),
                        &self.command,
                    )?,
                    PermissionMode::SkipAll => unreachable!(),
                };

                match decision {
                    GateDecision::AllowOnce | GateDecision::AllowAlways => {
                        eprintln!(
                            "[PERMISSION] granted: {} (attempt {}/3)",
                            perm_req.tool,
                            attempt + 1
                        );
                        current_allowed_tools.push(perm_req.tool);
                    }
                    GateDecision::Deny => {
                        return Err(GroveError::Runtime(format!(
                            "permission denied for tool '{}': {}",
                            perm_req.tool, perm_req.reason
                        )));
                    }
                    GateDecision::Abort => {
                        return Err(GroveError::Runtime(
                            "run aborted by user at permission gate".into(),
                        ));
                    }
                }
            } else {
                return Ok(response);
            }
        }

        Err(GroveError::Runtime("permission retry limit reached".into()))
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn session_continuity_policy(&self) -> SessionContinuityPolicy {
        SessionContinuityPolicy::LockedPerProcess
    }

    fn set_abort_handle(&self, handle: AbortHandle) {
        *self.abort_handle.lock().unwrap() = Some(handle);
    }
}

impl ClaudeCodeProvider {
    /// Run the claude CLI once, either with `--dangerously-skip-permissions` (when
    /// `allowed_tools` is `None`) or `--allowedTools <list>` (when provided).
    fn run_once(
        &self,
        request: &ProviderRequest,
        allowed_tools: Option<&[String]>,
    ) -> GroveResult<ProviderResponse> {
        let prompt = request.instructions.clone();
        let model = request.model.clone();
        let command = self.command.clone();
        let worktree = request.worktree_path.clone();
        let role = request.role.clone();
        // Per-request timeout_override takes effect when set (e.g. per-agent config).
        let effective_timeout_secs = request.timeout_override.unwrap_or(self.timeout_secs);
        let timeout = Duration::from_secs(effective_timeout_secs);
        // 7.3: resource limits applied via RLIMIT_FSIZE / RLIMIT_NOFILE on Unix.
        let max_file_size_mb = self.max_file_size_mb;
        let max_open_files = self.max_open_files;
        let permission_mode = self.permission_mode.clone();
        let tools: Vec<String> = allowed_tools.map(|t| t.to_vec()).unwrap_or_default();
        let max_output_bytes = self.max_output_bytes;

        // GROVE-026: share the child PID so the outer scope can kill the process
        // when the wall-clock timeout fires. The background thread sets the PID
        // immediately after `spawn()` and before any blocking I/O.
        let child_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
        let child_pid_thread = Arc::clone(&child_pid);

        let provider_session_id = request.provider_session_id.clone();
        let log_dir = request.log_dir.clone();
        let grove_session_id = request.grove_session_id.clone();
        let mcp_config_path = request.mcp_config_path.clone();

        // Set up token filter shim (best-effort — None if grove-filter not found).
        let filter_env =
            prepare_claude_filter_env(&worktree, &request.objective, request.model.as_deref());

        // Clone the abort handle (if set) so the timeout closure can register PIDs.
        let abort_handle = self.abort_handle.lock().unwrap().clone();

        // Pre-flight: bail immediately if already aborted.
        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }

        let abort_for_closure = abort_handle.clone();

        let result = timeout::with_timeout_and_pid(timeout, Arc::clone(&child_pid), move || {
            let mut args: Vec<String> = vec![
                "--print".into(),
                "--verbose".into(),
                "--output-format".into(),
                "stream-json".into(),
            ];

            match permission_mode {
                PermissionMode::SkipAll => {
                    args.push("--dangerously-skip-permissions".into());
                }
                PermissionMode::HumanGate | PermissionMode::AutonomousGate => {
                    if !tools.is_empty() {
                        args.push("--allowedTools".into());
                        args.push(tools.join(","));
                    }
                }
            }

            if let Some(ref m) = model {
                args.push("--model".into());
                args.push(m.clone());
            }
            if let Some(ref sid) = provider_session_id {
                args.push("--session-id".into());
                args.push(sid.clone());
            }
            // Inject MCP config for graph agents.
            if let Some(ref mcp_path) = mcp_config_path {
                super::mcp_inject::inject_mcp_args_claude(
                    &mut args,
                    std::path::Path::new(mcp_path),
                );
            }
            // Use `--` to separate flags from the positional prompt argument.
            // Without this, --mcp-config (variadic) consumes the prompt as
            // a second config value, causing ENAMETOOLONG errors.
            args.push("--".into());
            args.push(prompt.clone());

            let mut cmd = Command::new(&command);
            cmd.args(&args)
                .current_dir(&worktree)
                .env_remove("CLAUDECODE")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            apply_claude_filter_env(&mut cmd, &filter_env);

            // 7.3: Apply per-process POSIX resource limits on Unix (best-effort).
            // Uses CommandExt::before_exec to run setrlimit in the child after fork.
            apply_resource_limits(&mut cmd, max_file_size_mb, max_open_files);

            let mut child = cmd
                .spawn()
                .map_err(|e| GroveError::Runtime(format!("failed to launch claude CLI: {e}")))?;

            // Store PID before any blocking I/O so the outer scope can kill
            // this process if the wall-clock timeout fires.
            let pid = child.id();
            *child_pid_thread.lock().unwrap() = Some(pid);

            // Register PID with abort handle — PidGuard auto-unregisters on drop.
            let _abort_guard = abort_for_closure.as_ref().map(|h| h.register_pid(pid));

            // Stream NDJSON events from stdout, echoing key events to stderr.
            // Tee raw lines to a log file when log_dir is configured.
            let log_file = match (&log_dir, &grove_session_id) {
                (Some(dir), Some(sid)) => open_log_file(dir, sid),
                _ => None,
            };
            let stdout = child.stdout.take().unwrap();
            let reader = BufReader::new(stdout);
            let stream_result =
                collect_stream(reader, max_output_bytes, &role, &mut child, log_file)?;

            let status = child
                .wait()
                .map_err(|e| GroveError::Runtime(format!("failed to wait for claude CLI: {e}")))?;

            if !status.success() {
                // Capture stderr so the error message is actionable.
                let stderr_output = child
                    .stderr
                    .take()
                    .map(|mut se| {
                        let mut buf = String::new();
                        use std::io::Read;
                        se.read_to_string(&mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                let stderr_trimmed = stderr_output.trim();
                return Err(GroveError::Runtime(if stderr_trimmed.is_empty() {
                    format!("claude CLI exited with status {status}")
                } else {
                    format!("claude CLI exited with status {status}: {stderr_trimmed}")
                }));
            }

            if stream_result.is_error {
                return Err(GroveError::Runtime(format!(
                    "claude CLI reported error: {}",
                    stream_result.result_text
                )));
            }

            Ok(ProviderResponse {
                summary: stream_result.result_text,
                changed_files: vec![],
                cost_usd: stream_result.cost_usd,
                provider_session_id: stream_result.session_id,
                pid: *child_pid_thread.lock().unwrap(),
            })
        });

        // GROVE-026: if the timeout fired, the background thread's child process
        // is still running. Kill it so it does not become an orphan.
        if result.is_err() {
            if let Some(pid) = *child_pid.lock().unwrap() {
                let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
            }
        }

        // If abort was requested while the process was running, convert any
        // error (including non-zero exit from SIGKILL) into GroveError::Aborted.
        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }

        result
    }

    /// Streaming variant of `run_once` that emits `StreamOutputEvent`s to `sink`.
    fn run_once_streaming(
        &self,
        request: &ProviderRequest,
        allowed_tools: Option<&[String]>,
        sink: &dyn StreamSink,
    ) -> GroveResult<ProviderResponse> {
        let prompt = request.instructions.clone();
        let model = request.model.clone();
        let command = self.command.clone();
        let worktree = request.worktree_path.clone();
        let role = request.role.clone();
        let effective_timeout_secs = request.timeout_override.unwrap_or(self.timeout_secs);
        let timeout = Duration::from_secs(effective_timeout_secs);
        let max_file_size_mb = self.max_file_size_mb;
        let max_open_files = self.max_open_files;
        let permission_mode = self.permission_mode.clone();
        let tools: Vec<String> = allowed_tools.map(|t| t.to_vec()).unwrap_or_default();
        let max_output_bytes = self.max_output_bytes;

        let child_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
        let child_pid_thread = Arc::clone(&child_pid);

        let provider_session_id = request.provider_session_id.clone();
        let log_dir = request.log_dir.clone();
        let grove_session_id = request.grove_session_id.clone();
        let mcp_config_path = request.mcp_config_path.clone();

        // Set up token filter shim (best-effort).
        let filter_env =
            prepare_claude_filter_env(&worktree, &request.objective, request.model.as_deref());

        let abort_handle = self.abort_handle.lock().unwrap().clone();

        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }

        let abort_for_closure = abort_handle.clone();
        // SAFETY: `with_timeout` blocks until the closure finishes, so `sink`
        // is guaranteed alive for the duration. SinkPtr stores the fat pointer
        // as plain usize values that are Send + 'static.
        let sink_ptr = SinkPtr::from_ref(sink);

        let result = timeout::with_timeout_and_pid(timeout, Arc::clone(&child_pid), move || {
            // SAFETY: lifetime guaranteed by with_timeout_and_pid blocking join.
            let sink: &dyn StreamSink = unsafe { sink_ptr.as_ref() };

            let mut args: Vec<String> = vec![
                "--print".into(),
                "--verbose".into(),
                "--output-format".into(),
                "stream-json".into(),
            ];

            match permission_mode {
                PermissionMode::SkipAll => {
                    args.push("--dangerously-skip-permissions".into());
                }
                PermissionMode::HumanGate | PermissionMode::AutonomousGate => {
                    if !tools.is_empty() {
                        args.push("--allowedTools".into());
                        args.push(tools.join(","));
                    }
                }
            }

            if let Some(ref m) = model {
                args.push("--model".into());
                args.push(m.clone());
            }
            if let Some(ref sid) = provider_session_id {
                args.push("--session-id".into());
                args.push(sid.clone());
            }
            // Inject MCP config for graph agents.
            if let Some(ref mcp_path) = mcp_config_path {
                super::mcp_inject::inject_mcp_args_claude(
                    &mut args,
                    std::path::Path::new(mcp_path),
                );
            }
            // Use `--` to separate flags from the positional prompt argument.
            // Without this, --mcp-config (variadic) consumes the prompt as
            // a second config value, causing ENAMETOOLONG errors.
            args.push("--".into());
            args.push(prompt.clone());

            let mut cmd = Command::new(&command);
            cmd.args(&args)
                .current_dir(&worktree)
                .env_remove("CLAUDECODE")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // Non-interactive streaming never writes answers back to Claude.
                // Leaving stdin piped/open can keep the child alive waiting for
                // EOF even after it has emitted its final result.
                .stdin(Stdio::null());
            apply_claude_filter_env(&mut cmd, &filter_env);

            apply_resource_limits(&mut cmd, max_file_size_mb, max_open_files);

            let mut child = cmd
                .spawn()
                .map_err(|e| GroveError::Runtime(format!("failed to launch claude CLI: {e}")))?;

            let pid = child.id();
            *child_pid_thread.lock().unwrap() = Some(pid);

            let _abort_guard = abort_for_closure.as_ref().map(|h| h.register_pid(pid));

            tracing::info!(
                pid = pid,
                command = %command,
                role = %role,
                "claude CLI spawned — waiting for output"
            );

            // Capture stderr on a background thread so we always have it —
            // even if the process is killed by timeout.
            let stderr_handle = child
                .stderr
                .take()
                .and_then(|se| {
                    std::thread::Builder::new()
                        .name("claude-stderr".into())
                        .spawn(move || {
                            let mut buf = String::new();
                            use std::io::Read;
                            let mut reader = se;
                            let _ = reader.read_to_string(&mut buf);
                            buf
                        })
                        .ok()
                });

            // Emit a system event so the UI shows something while waiting for API response.
            sink.on_event(StreamOutputEvent::System {
                message: format!("Agent {} started, waiting for response...", role),
                session_id: grove_session_id.clone(),
            });

            let log_file = match (&log_dir, &grove_session_id) {
                (Some(dir), Some(sid)) => open_log_file(dir, sid),
                _ => None,
            };
            let session_log_path: Option<std::path::PathBuf> = match (&log_dir, &grove_session_id) {
                (Some(dir), Some(sid)) => Some(Path::new(dir).join(format!("session-{sid}.jsonl"))),
                _ => None,
            };
            let stdout = child.stdout.take().unwrap();
            let reader = BufReader::new(stdout);
            let stream_result = collect_stream_live(
                reader,
                max_output_bytes,
                &role,
                &mut child,
                sink,
                log_file,
                Some(&worktree),
                session_log_path.as_deref(),
            )?;

            let status = child
                .wait()
                .map_err(|e| GroveError::Runtime(format!("failed to wait for claude CLI: {e}")))?;

            // Wait for stderr collection only after the child has exited.
            let stderr_output = stderr_handle
                .and_then(|h| h.join().ok())
                .unwrap_or_default();
            if !stderr_output.trim().is_empty() {
                tracing::warn!(
                    pid = pid,
                    stderr = %stderr_output.trim(),
                    "claude CLI stderr output"
                );
            }

            if !status.success() {
                let stderr_trimmed = stderr_output.trim();
                return Err(GroveError::Runtime(if stderr_trimmed.is_empty() {
                    format!("claude CLI exited with status {status}")
                } else {
                    format!("claude CLI exited with status {status}: {stderr_trimmed}")
                }));
            }

            if stream_result.is_error {
                return Err(GroveError::Runtime(format!(
                    "claude CLI reported error: {}",
                    stream_result.result_text
                )));
            }

            Ok(ProviderResponse {
                summary: stream_result.result_text,
                changed_files: vec![],
                cost_usd: stream_result.cost_usd,
                provider_session_id: stream_result.session_id,
                pid: *child_pid_thread.lock().unwrap(),
            })
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

        result
    }

    /// Interactive variant of `run_once_streaming` that supports agent Q&A.
    ///
    /// Key differences from `run_once_streaming`:
    /// - Does NOT use `--print` flag (keeps the process alive for stdin input)
    /// - Pipes stdin so answers can be written back to the agent
    /// - Passes `qa_source` to `collect_stream_live` for blocking-question handling
    fn run_once_interactive(
        &self,
        request: &ProviderRequest,
        allowed_tools: Option<&[String]>,
        sink: &dyn StreamSink,
        qa_source: &dyn QaSource,
    ) -> GroveResult<ProviderResponse> {
        let prompt = request.instructions.clone();
        let model = request.model.clone();
        let command = self.command.clone();
        let worktree = request.worktree_path.clone();
        let role = request.role.clone();
        let run_id = request.objective.clone(); // used for Q&A context
        let effective_timeout_secs = request.timeout_override.unwrap_or(self.timeout_secs);
        let timeout = Duration::from_secs(effective_timeout_secs);
        let max_file_size_mb = self.max_file_size_mb;
        let max_open_files = self.max_open_files;
        let permission_mode = self.permission_mode.clone();
        let tools: Vec<String> = allowed_tools.map(|t| t.to_vec()).unwrap_or_default();
        let max_output_bytes = self.max_output_bytes;

        let child_pid: Arc<Mutex<Option<u32>>> = Arc::new(Mutex::new(None));
        let child_pid_thread = Arc::clone(&child_pid);

        let provider_session_id = request.provider_session_id.clone();
        let log_dir = request.log_dir.clone();
        let grove_session_id = request.grove_session_id.clone();
        let mcp_config_path = request.mcp_config_path.clone();

        // Set up token filter shim (best-effort).
        let filter_env =
            prepare_claude_filter_env(&worktree, &request.objective, request.model.as_deref());

        let abort_handle = self.abort_handle.lock().unwrap().clone();

        if let Some(ref h) = abort_handle {
            if h.is_aborted() {
                return Err(GroveError::Aborted);
            }
        }

        let abort_for_closure = abort_handle.clone();

        // SAFETY: `with_timeout` blocks until the closure finishes, so `sink`
        // and `qa_source` are guaranteed alive for the duration.
        let sink_ptr = SinkPtr::from_ref(sink);
        let qa_ptr = QaSourcePtr::from_ref(qa_source);

        let result = timeout::with_timeout_and_pid(timeout, Arc::clone(&child_pid), move || {
            let sink: &dyn StreamSink = unsafe { sink_ptr.as_ref() };
            let qa: &dyn QaSource = unsafe { qa_ptr.as_ref() };

            // Interactive mode: no --print flag, so the process stays alive
            // and accepts stdin input for Q&A responses.
            let mut args: Vec<String> = vec![
                "--verbose".into(),
                "--output-format".into(),
                "stream-json".into(),
            ];

            match permission_mode {
                PermissionMode::SkipAll => {
                    args.push("--dangerously-skip-permissions".into());
                }
                PermissionMode::HumanGate | PermissionMode::AutonomousGate => {
                    if !tools.is_empty() {
                        args.push("--allowedTools".into());
                        args.push(tools.join(","));
                    }
                }
            }

            if let Some(ref m) = model {
                args.push("--model".into());
                args.push(m.clone());
            }
            if let Some(ref sid) = provider_session_id {
                args.push("--session-id".into());
                args.push(sid.clone());
            }
            // Inject MCP config for graph agents.
            if let Some(ref mcp_path) = mcp_config_path {
                super::mcp_inject::inject_mcp_args_claude(
                    &mut args,
                    std::path::Path::new(mcp_path),
                );
            }
            // Use `--` to separate flags from the positional prompt argument.
            // Without this, --mcp-config (variadic) consumes the prompt as
            // a second config value, causing ENAMETOOLONG errors.
            args.push("--".into());
            args.push(prompt.clone());

            let mut cmd = Command::new(&command);
            cmd.args(&args)
                .current_dir(&worktree)
                .env_remove("CLAUDECODE")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::piped()); // Interactive: pipe stdin for Q&A
            apply_claude_filter_env(&mut cmd, &filter_env);

            apply_resource_limits(&mut cmd, max_file_size_mb, max_open_files);

            let mut child = cmd.spawn().map_err(|e| {
                GroveError::Runtime(format!("failed to launch claude CLI (interactive): {e}"))
            })?;

            let pid = child.id();
            *child_pid_thread.lock().unwrap() = Some(pid);

            let _abort_guard = abort_for_closure.as_ref().map(|h| h.register_pid(pid));

            tracing::info!(
                pid = pid,
                command = %command,
                role = %role,
                "claude CLI (interactive) spawned — waiting for output"
            );

            // Capture stderr on a background thread so we always have it.
            let stderr_handle = child
                .stderr
                .take()
                .and_then(|se| {
                    std::thread::Builder::new()
                        .name("claude-stderr-interactive".into())
                        .spawn(move || {
                            let mut buf = String::new();
                            use std::io::Read;
                            let mut reader = se;
                            let _ = reader.read_to_string(&mut buf);
                            buf
                        })
                        .ok()
                });

            // Emit a system event so the UI shows something while waiting for API response.
            sink.on_event(StreamOutputEvent::System {
                message: format!("Agent {} started, waiting for response...", role),
                session_id: grove_session_id.clone(),
            });

            let log_file = match (&log_dir, &grove_session_id) {
                (Some(dir), Some(sid)) => open_log_file(dir, sid),
                _ => None,
            };
            let session_log_path: Option<std::path::PathBuf> = match (&log_dir, &grove_session_id) {
                (Some(dir), Some(sid)) => Some(Path::new(dir).join(format!("session-{sid}.jsonl"))),
                _ => None,
            };

            // Take stdin from the child for writing Q&A answers.
            let child_stdin = child.stdin.take();

            let stdout = child.stdout.take().unwrap();
            let reader = BufReader::new(stdout);
            let stream_result = collect_stream_live_interactive(
                reader,
                max_output_bytes,
                &role,
                &mut child,
                sink,
                log_file,
                child_stdin,
                qa,
                &run_id,
                grove_session_id.as_deref(),
                Some(&worktree),
                session_log_path.as_deref(),
            )?;

            let status = child
                .wait()
                .map_err(|e| GroveError::Runtime(format!("failed to wait for claude CLI: {e}")))?;

            let stderr_output = stderr_handle
                .and_then(|h| h.join().ok())
                .unwrap_or_default();
            if !stderr_output.trim().is_empty() {
                tracing::warn!(
                    pid = pid,
                    stderr = %stderr_output.trim(),
                    "claude CLI (interactive) stderr output"
                );
            }

            if !status.success() {
                let stderr_trimmed = stderr_output.trim();
                return Err(GroveError::Runtime(if stderr_trimmed.is_empty() {
                    format!("claude CLI exited with status {status}")
                } else {
                    format!("claude CLI exited with status {status}: {stderr_trimmed}")
                }));
            }

            if stream_result.is_error {
                return Err(GroveError::Runtime(format!(
                    "claude CLI reported error: {}",
                    stream_result.result_text
                )));
            }

            Ok(ProviderResponse {
                summary: stream_result.result_text,
                changed_files: vec![],
                cost_usd: stream_result.cost_usd,
                provider_session_id: stream_result.session_id,
                pid: *child_pid_thread.lock().unwrap(),
            })
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

        result
    }
}

/// Read lines from `reader`, echoing each to stderr with `[ROLE]` prefix.
///
/// Stops reading and kills `child` if the total bytes collected exceed
/// `max_bytes`. Returns `Err(GroveError::Runtime)` in that case.
#[allow(dead_code)]
pub(crate) fn collect_capped(
    reader: impl BufRead + Send + 'static,
    max_bytes: usize,
    role: &str,
    child: &mut Child,
    mut log_file: Option<std::fs::File>,
    sink: Option<&dyn super::StreamSink>,
) -> GroveResult<String> {
    let mut collected = String::new();
    let timed = TimedLineReader::new(reader, Duration::from_secs(STDOUT_IDLE_TIMEOUT_SECS));

    loop {
        match timed.next_line() {
            Ok(l) => {
                eprintln!("[{}] {}", role.to_uppercase(), l);
                // Tee raw line to log file (best-effort).
                if let Some(ref mut f) = log_file {
                    let _ = writeln!(f, "{}", l);
                }
                // Emit raw line to sink for real-time streaming + check for questions
                if let Some(s) = sink {
                    s.on_event(super::StreamOutputEvent::RawLine { line: l.clone() });
                    if let Some(detected) = super::question_detector::detect_question(&l) {
                        if detected.confidence >= 0.8 {
                            s.on_event(super::StreamOutputEvent::Question {
                                question: detected.question,
                                options: detected.options,
                                blocking: true,
                            });
                        }
                    }
                }
                collected.push_str(&l);
                collected.push('\n');
                if collected.len() > max_bytes {
                    let _ = child.kill();
                    return Err(GroveError::Runtime(format!(
                        "agent output exceeded cap of {} bytes; process killed",
                        max_bytes
                    )));
                }
            }
            Err(LineError::IdleTimeout) => {
                tracing::warn!(
                    "agent process idle — no output for {} seconds; killing",
                    STDOUT_IDLE_TIMEOUT_SECS
                );
                let _ = child.kill();
                return Err(GroveError::Runtime(format!(
                    "agent process idle — no output for {} seconds; process killed",
                    STDOUT_IDLE_TIMEOUT_SECS
                )));
            }
            Err(LineError::Eof) => break,
            Err(LineError::Io(_)) => break,
        }
    }
    Ok(collected)
}

/// Open a log file for tee-ing raw output. Creates parent dirs if needed.
/// Returns `None` (with a warning) if the file can't be created.
pub(crate) fn open_log_file(log_dir: &str, session_id: &str) -> Option<std::fs::File> {
    let dir = Path::new(log_dir);
    if let Err(e) = fs::create_dir_all(dir) {
        tracing::warn!("failed to create log dir {}: {e}", dir.display());
        return None;
    }
    let path = dir.join(format!("session-{session_id}.jsonl"));
    match fs::File::create(&path) {
        Ok(f) => {
            tracing::info!("session log: {}", path.display());
            Some(f)
        }
        Err(e) => {
            tracing::warn!("failed to create log file {}: {e}", path.display());
            None
        }
    }
}

/// Read NDJSON lines from `reader`, parse each as a stream event, and echo
/// key information to stderr with `[ROLE]` prefix.
///
/// When `log_file` is `Some`, each raw line is tee'd to the file for later
/// replay in the GUI thread view.
///
/// Stops reading and kills `child` if the total bytes collected exceed
/// `max_bytes`. Returns the aggregated `StreamResult`.
pub(crate) fn collect_stream(
    reader: impl BufRead + Send + 'static,
    max_bytes: usize,
    role: &str,
    child: &mut Child,
    mut log_file: Option<std::fs::File>,
) -> GroveResult<StreamResult> {
    let role_upper = role.to_uppercase();
    let mut total_bytes = 0usize;
    let mut result_text = String::new();
    let mut is_error = false;
    let mut cost_usd: Option<f64> = None;
    let mut session_id: Option<String> = None;
    let mut assistant_lines = 0u32;
    // Accumulates agent_message text from ItemCompleted events (Codex/new format).
    let mut accumulated_messages: Vec<String> = Vec::new();

    let timed = TimedLineReader::new(reader, Duration::from_secs(STDOUT_IDLE_TIMEOUT_SECS));

    loop {
        match timed.next_line() {
            Ok(l) => {
                total_bytes += l.len() + 1; // +1 for newline
                if total_bytes > max_bytes {
                    let _ = child.kill();
                    return Err(GroveError::Runtime(format!(
                        "agent output exceeded cap of {} bytes; process killed",
                        max_bytes
                    )));
                }

                // Tee raw line to log file (best-effort, don't fail the run).
                if let Some(ref mut f) = log_file {
                    let _ = writeln!(f, "{}", l);
                }

                if let Some(event) = stream_parser::parse_event(&l) {
                    match event {
                        StreamEvent::System(sys) => {
                            if let Some(ref sid) = sys.session_id {
                                session_id = Some(sid.clone());
                            }
                            if let Some(ref msg) = sys.message {
                                eprintln!("[{}] {}", role_upper, msg);
                            }
                        }
                        StreamEvent::Assistant(a) => {
                            if assistant_lines < 5 {
                                if let Some(ref msg) = a.message {
                                    eprintln!("[{}] {}", role_upper, msg);
                                    assistant_lines += 1;
                                }
                            }
                        }
                        StreamEvent::ToolUse(tu) => {
                            if let Some(ref name) = tu.name {
                                eprintln!("[{}] tool: {}", role_upper, name);
                            }
                        }
                        StreamEvent::ToolResult(_) => {
                            // Too verbose to echo
                        }
                        StreamEvent::Result(res) => {
                            result_text = res.result;
                            is_error = res.is_error;
                            cost_usd = res.cost_usd;
                            if let Some(ref sid) = res.session_id {
                                session_id = Some(sid.clone());
                            }
                        }
                        StreamEvent::Question(_) => {
                            // Questions are only forwarded via collect_stream_live
                        }
                        // ── Codex / new-format events ───────────────────
                        StreamEvent::ThreadStarted(ref ts) => {
                            if let Some(ref tid) = ts.thread_id {
                                session_id = Some(tid.clone());
                            }
                        }
                        StreamEvent::TurnStarted {} => {}
                        StreamEvent::ItemCompleted(ref ic) => {
                            if let Some(ref item) = ic.item {
                                if item.item_type.as_deref() == Some("agent_message") {
                                    if let Some(ref text) = item.text {
                                        if !text.trim().is_empty() {
                                            if assistant_lines < 5 {
                                                eprintln!("[{}] {}", role_upper, text);
                                                assistant_lines += 1;
                                            }
                                            accumulated_messages.push(text.clone());
                                        }
                                    }
                                }
                            }
                        }
                        StreamEvent::TurnCompleted(_) => {
                            // Terminal: build result from accumulated messages.
                            if result_text.is_empty() && !accumulated_messages.is_empty() {
                                result_text = accumulated_messages.join("\n\n");
                            }
                            break;
                        }
                        StreamEvent::TurnFailed(ref tf) => {
                            is_error = true;
                            let msg = tf
                                .error
                                .as_ref()
                                .and_then(|e| e.message.as_deref())
                                .unwrap_or("unknown error");
                            if result_text.is_empty() {
                                result_text = msg.to_string();
                            }
                            break;
                        }
                    }
                }
            }
            Err(LineError::IdleTimeout) => {
                tracing::warn!(
                    "agent process idle — no output for {} seconds; killing",
                    STDOUT_IDLE_TIMEOUT_SECS
                );
                let _ = child.kill();
                return Err(GroveError::Runtime(format!(
                    "agent process idle — no output for {} seconds; process killed",
                    STDOUT_IDLE_TIMEOUT_SECS
                )));
            }
            Err(LineError::Eof) => break,
            Err(LineError::Io(_)) => break,
        }
    }

    Ok(StreamResult {
        result_text,
        is_error,
        cost_usd,
        session_id,
    })
}

/// Tracks whether the agent process is making real progress during stdout-idle periods.
///
/// Two independent signals are checked when `TimedLineReader` fires `IdleTimeout`:
/// - **Session log growth**: the `.jsonl` file grows as Claude writes events (tool calls,
///   text chunks). If it grew, the process is actively working.
/// - **Worktree mtime**: any file write in the worktree (up to 4 levels deep, `.git`
///   excluded) means a tool is executing. If a file is newer than `last_check`, the
///   process is doing real work.
///
/// If either signal fires, the idle timer is reset and we keep waiting. Only when
/// both signals are silent for a full `STDOUT_IDLE_TIMEOUT_SECS` window do we kill.
struct ActivityChecker {
    worktree_path: Option<std::path::PathBuf>,
    session_log_path: Option<std::path::PathBuf>,
    last_session_log_size: u64,
    last_check: std::time::SystemTime,
}

impl ActivityChecker {
    fn new(worktree_path: Option<&str>, session_log_path: Option<&Path>) -> Self {
        let last_session_log_size = session_log_path
            .and_then(|p| fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        Self {
            worktree_path: worktree_path.map(std::path::PathBuf::from),
            session_log_path: session_log_path.map(|p| p.to_path_buf()),
            last_session_log_size,
            last_check: std::time::SystemTime::now(),
        }
    }

    /// Returns `true` if either signal shows activity since the last call.
    /// Always updates internal state so the next call measures a fresh window.
    fn has_activity(&mut self) -> bool {
        let log_changed = self.check_session_log();
        let fs_changed = self.check_worktree();
        self.last_check = std::time::SystemTime::now();
        log_changed || fs_changed
    }

    fn check_session_log(&mut self) -> bool {
        let Some(ref path) = self.session_log_path else {
            return false;
        };
        let current_size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let changed = current_size != self.last_session_log_size;
        self.last_session_log_size = current_size;
        changed
    }

    fn check_worktree(&self) -> bool {
        let Some(ref worktree) = self.worktree_path else {
            return false;
        };
        worktree_has_recent_mtime(worktree, self.last_check, 0)
    }
}

/// Walk `dir` up to 4 levels deep (`.git` excluded) and return `true` if any
/// file has an mtime strictly after `since`.
fn worktree_has_recent_mtime(dir: &Path, since: std::time::SystemTime, depth: u32) -> bool {
    if depth > 4 {
        return false;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().map(|n| n == ".git").unwrap_or(false) {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        if meta.modified().map(|m| m > since).unwrap_or(false) {
            return true;
        }
        if meta.is_dir() && worktree_has_recent_mtime(&path, since, depth + 1) {
            return true;
        }
    }
    false
}

/// Read NDJSON lines from `reader`, parse each as a stream event, echo
/// key information to stderr, AND emit `StreamOutputEvent`s to `sink` in
/// real time.
///
/// Stops reading and kills `child` if the total bytes collected exceed
/// `max_bytes`. Returns the aggregated `StreamResult`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_stream_live(
    reader: impl BufRead + Send + 'static,
    max_bytes: usize,
    role: &str,
    child: &mut Child,
    sink: &dyn StreamSink,
    mut log_file: Option<std::fs::File>,
    worktree_path: Option<&str>,
    session_log_path: Option<&Path>,
) -> GroveResult<StreamResult> {
    let role_upper = role.to_uppercase();
    let mut total_bytes = 0usize;
    let mut result_text = String::new();
    let mut is_error = false;
    let mut cost_usd: Option<f64> = None;
    let mut session_id: Option<String> = None;
    let mut assistant_lines = 0u32;
    // Accumulates agent_message text from ItemCompleted events (Codex/new format).
    let mut accumulated_messages: Vec<String> = Vec::new();

    let timed = TimedLineReader::new(reader, Duration::from_secs(STDOUT_IDLE_TIMEOUT_SECS));
    let mut checker = ActivityChecker::new(worktree_path, session_log_path);

    loop {
        match timed.next_line() {
            Ok(l) => {
                total_bytes += l.len() + 1;
                if total_bytes > max_bytes {
                    let _ = child.kill();
                    return Err(GroveError::Runtime(format!(
                        "agent output exceeded cap of {} bytes; process killed",
                        max_bytes
                    )));
                }

                // Tee raw line to log file (best-effort).
                if let Some(ref mut f) = log_file {
                    let _ = writeln!(f, "{}", l);
                }

                if let Some(event) = stream_parser::parse_event(&l) {
                    match event {
                        StreamEvent::System(ref sys) => {
                            if let Some(ref sid) = sys.session_id {
                                session_id = Some(sid.clone());
                            }
                            if let Some(ref msg) = sys.message {
                                eprintln!("[{}] {}", role_upper, msg);
                            }
                            sink.on_event(StreamOutputEvent::System {
                                message: sys.message.clone().unwrap_or_default(),
                                session_id: sys.session_id.clone(),
                            });
                        }
                        StreamEvent::Assistant(ref a) => {
                            if assistant_lines < 5 {
                                if let Some(ref msg) = a.message {
                                    eprintln!("[{}] {}", role_upper, msg);
                                    assistant_lines += 1;
                                }
                            }
                            sink.on_event(StreamOutputEvent::AssistantText {
                                text: a.message.clone().unwrap_or_default(),
                            });
                        }
                        StreamEvent::ToolUse(ref tu) => {
                            if let Some(ref name) = tu.name {
                                eprintln!("[{}] tool: {}", role_upper, name);
                            }
                            sink.on_event(StreamOutputEvent::ToolUse {
                                tool: tu.name.clone().unwrap_or_default(),
                            });
                        }
                        StreamEvent::ToolResult(ref tr) => {
                            sink.on_event(StreamOutputEvent::ToolResult {
                                tool: tr.name.clone().unwrap_or_default(),
                            });
                        }
                        StreamEvent::Result(ref res) => {
                            result_text = res.result.clone();
                            is_error = res.is_error;
                            cost_usd = res.cost_usd;
                            if let Some(ref sid) = res.session_id {
                                session_id = Some(sid.clone());
                            }
                            sink.on_event(StreamOutputEvent::Result {
                                text: res.result.clone(),
                                cost_usd: res.cost_usd,
                                is_error: res.is_error,
                                session_id: res.session_id.clone(),
                            });
                        }
                        StreamEvent::Question(ref q) => {
                            sink.on_event(StreamOutputEvent::Question {
                                question: q.question.clone(),
                                options: q.options.clone(),
                                blocking: q.blocking,
                            });
                        }
                        // ── Codex / new-format events ───────────────────
                        StreamEvent::ThreadStarted(ref ts) => {
                            if let Some(ref tid) = ts.thread_id {
                                session_id = Some(tid.clone());
                            }
                            sink.on_event(StreamOutputEvent::System {
                                message: String::new(),
                                session_id: ts.thread_id.clone(),
                            });
                        }
                        StreamEvent::TurnStarted {} => {}
                        StreamEvent::ItemCompleted(ref ic) => {
                            if let Some(ref item) = ic.item {
                                if item.item_type.as_deref() == Some("agent_message") {
                                    if let Some(ref text) = item.text {
                                        if !text.trim().is_empty() {
                                            if assistant_lines < 5 {
                                                eprintln!("[{}] {}", role_upper, text);
                                                assistant_lines += 1;
                                            }
                                            accumulated_messages.push(text.clone());
                                            sink.on_event(StreamOutputEvent::AssistantText {
                                                text: text.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        StreamEvent::TurnCompleted(_) => {
                            if result_text.is_empty() && !accumulated_messages.is_empty() {
                                result_text = accumulated_messages.join("\n\n");
                            }
                            sink.on_event(StreamOutputEvent::Result {
                                text: result_text.clone(),
                                cost_usd,
                                is_error,
                                session_id: session_id.clone(),
                            });
                            break;
                        }
                        StreamEvent::TurnFailed(ref tf) => {
                            is_error = true;
                            let msg = tf
                                .error
                                .as_ref()
                                .and_then(|e| e.message.as_deref())
                                .unwrap_or("unknown error");
                            if result_text.is_empty() {
                                result_text = msg.to_string();
                            }
                            sink.on_event(StreamOutputEvent::Result {
                                text: result_text.clone(),
                                cost_usd,
                                is_error: true,
                                session_id: session_id.clone(),
                            });
                            break;
                        }
                    }
                }
            }
            Err(LineError::IdleTimeout) => {
                if checker.has_activity() {
                    tracing::warn!(
                        role = %role_upper,
                        "stdout idle for {STDOUT_IDLE_TIMEOUT_SECS}s but session-log or worktree activity detected — still working, continuing"
                    );
                    continue;
                }
                tracing::warn!(
                    role = %role_upper,
                    "agent process idle — no stdout or filesystem activity for {STDOUT_IDLE_TIMEOUT_SECS}s; killing"
                );
                let _ = child.kill();
                return Err(GroveError::Runtime(format!(
                    "agent process idle — no stdout or filesystem activity for {} seconds; process killed",
                    STDOUT_IDLE_TIMEOUT_SECS
                )));
            }
            Err(LineError::Eof) => break,
            Err(LineError::Io(_)) => break,
        }
    }

    // If we only got new-format events and broke on EOF (not TurnCompleted),
    // build result from accumulated messages.
    if result_text.is_empty() && !accumulated_messages.is_empty() {
        result_text = accumulated_messages.join("\n\n");
    }

    Ok(StreamResult {
        result_text,
        is_error,
        cost_usd,
        session_id,
    })
}

/// Interactive variant of [`collect_stream_live`] that handles blocking questions
/// by delegating to a [`QaSource`] and writing answers back to the agent's stdin.
///
/// When a `StreamEvent::Question` with `blocking: true` is detected:
/// 1. The question is emitted via `sink` so the frontend can display it
/// 2. `qa_source.wait_for_answer(...)` is called (blocks until user responds)
/// 3. The answer is written as JSON to `stdin` so the agent can continue
/// 4. A `StreamOutputEvent::UserAnswer` is emitted via `sink`
#[allow(clippy::too_many_arguments)]
pub(crate) fn collect_stream_live_interactive(
    reader: impl BufRead + Send + 'static,
    max_bytes: usize,
    role: &str,
    child: &mut Child,
    sink: &dyn StreamSink,
    mut log_file: Option<std::fs::File>,
    mut stdin: Option<std::process::ChildStdin>,
    qa_source: &dyn QaSource,
    run_id: &str,
    session_id_hint: Option<&str>,
    worktree_path: Option<&str>,
    session_log_path: Option<&Path>,
) -> GroveResult<StreamResult> {
    let role_upper = role.to_uppercase();
    let mut total_bytes = 0usize;
    let mut result_text = String::new();
    let mut is_error = false;
    let mut cost_usd: Option<f64> = None;
    let mut session_id: Option<String> = None;
    let mut assistant_lines = 0u32;
    // Accumulates agent_message text from ItemCompleted events (Codex/new format).
    let mut accumulated_messages: Vec<String> = Vec::new();

    let timed = TimedLineReader::new(reader, Duration::from_secs(STDOUT_IDLE_TIMEOUT_SECS));
    let mut checker = ActivityChecker::new(worktree_path, session_log_path);

    loop {
        match timed.next_line() {
            Ok(l) => {
                total_bytes += l.len() + 1;
                if total_bytes > max_bytes {
                    let _ = child.kill();
                    return Err(GroveError::Runtime(format!(
                        "agent output exceeded cap of {} bytes; process killed",
                        max_bytes
                    )));
                }

                // Tee raw line to log file (best-effort).
                if let Some(ref mut f) = log_file {
                    let _ = writeln!(f, "{}", l);
                }

                if let Some(event) = stream_parser::parse_event(&l) {
                    match event {
                        StreamEvent::System(ref sys) => {
                            if let Some(ref sid) = sys.session_id {
                                session_id = Some(sid.clone());
                            }
                            if let Some(ref msg) = sys.message {
                                eprintln!("[{}] {}", role_upper, msg);
                            }
                            sink.on_event(StreamOutputEvent::System {
                                message: sys.message.clone().unwrap_or_default(),
                                session_id: sys.session_id.clone(),
                            });
                        }
                        StreamEvent::Assistant(ref a) => {
                            if assistant_lines < 5 {
                                if let Some(ref msg) = a.message {
                                    eprintln!("[{}] {}", role_upper, msg);
                                    assistant_lines += 1;
                                }
                            }
                            sink.on_event(StreamOutputEvent::AssistantText {
                                text: a.message.clone().unwrap_or_default(),
                            });
                        }
                        StreamEvent::ToolUse(ref tu) => {
                            if let Some(ref name) = tu.name {
                                eprintln!("[{}] tool: {}", role_upper, name);
                            }
                            sink.on_event(StreamOutputEvent::ToolUse {
                                tool: tu.name.clone().unwrap_or_default(),
                            });
                        }
                        StreamEvent::ToolResult(ref tr) => {
                            sink.on_event(StreamOutputEvent::ToolResult {
                                tool: tr.name.clone().unwrap_or_default(),
                            });
                        }
                        StreamEvent::Result(ref res) => {
                            result_text = res.result.clone();
                            is_error = res.is_error;
                            cost_usd = res.cost_usd;
                            if let Some(ref sid) = res.session_id {
                                session_id = Some(sid.clone());
                            }
                            sink.on_event(StreamOutputEvent::Result {
                                text: res.result.clone(),
                                cost_usd: res.cost_usd,
                                is_error: res.is_error,
                                session_id: res.session_id.clone(),
                            });
                        }
                        StreamEvent::Question(ref q) => {
                            // Emit the question to the frontend first.
                            sink.on_event(StreamOutputEvent::Question {
                                question: q.question.clone(),
                                options: q.options.clone(),
                                blocking: q.blocking,
                            });

                            // If blocking: wait for an answer and pipe it back to the agent.
                            if q.blocking {
                                let effective_session_id =
                                    session_id.as_deref().or(session_id_hint);
                                match qa_source.wait_for_answer(
                                    run_id,
                                    effective_session_id,
                                    &q.question,
                                    &q.options,
                                ) {
                                    Ok(answer) if !answer.is_empty() => {
                                        // Write answer to agent's stdin as JSON.
                                        if let Some(ref mut writer) = stdin {
                                            let answer_payload = serde_json::json!({
                                                "type": "user_input",
                                                "text": answer,
                                            });
                                            if let Err(e) = writeln!(writer, "{}", answer_payload) {
                                                tracing::warn!(
                                                    error = %e,
                                                    "failed to write Q&A answer to agent stdin"
                                                );
                                            } else if let Err(e) = writer.flush() {
                                                tracing::warn!(
                                                    error = %e,
                                                    "failed to flush Q&A answer to agent stdin"
                                                );
                                            }
                                        }
                                        sink.on_event(StreamOutputEvent::UserAnswer {
                                            text: answer,
                                        });
                                    }
                                    Ok(_) => {
                                        // Empty answer: Q&A source returned nothing (e.g. NoQaSource).
                                        tracing::debug!(
                                            "Q&A source returned empty answer — skipping"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            error = %e,
                                            question = %q.question,
                                            "Q&A wait_for_answer failed"
                                        );
                                    }
                                }
                            }
                        }
                        // ── Codex / new-format events ───────────────────
                        StreamEvent::ThreadStarted(ref ts) => {
                            if let Some(ref tid) = ts.thread_id {
                                session_id = Some(tid.clone());
                            }
                            sink.on_event(StreamOutputEvent::System {
                                message: String::new(),
                                session_id: ts.thread_id.clone(),
                            });
                        }
                        StreamEvent::TurnStarted {} => {}
                        StreamEvent::ItemCompleted(ref ic) => {
                            if let Some(ref item) = ic.item {
                                if item.item_type.as_deref() == Some("agent_message") {
                                    if let Some(ref text) = item.text {
                                        if !text.trim().is_empty() {
                                            if assistant_lines < 5 {
                                                eprintln!("[{}] {}", role_upper, text);
                                                assistant_lines += 1;
                                            }
                                            accumulated_messages.push(text.clone());
                                            sink.on_event(StreamOutputEvent::AssistantText {
                                                text: text.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        StreamEvent::TurnCompleted(_) => {
                            if result_text.is_empty() && !accumulated_messages.is_empty() {
                                result_text = accumulated_messages.join("\n\n");
                            }
                            sink.on_event(StreamOutputEvent::Result {
                                text: result_text.clone(),
                                cost_usd,
                                is_error,
                                session_id: session_id.clone(),
                            });
                            break;
                        }
                        StreamEvent::TurnFailed(ref tf) => {
                            is_error = true;
                            let msg = tf
                                .error
                                .as_ref()
                                .and_then(|e| e.message.as_deref())
                                .unwrap_or("unknown error");
                            if result_text.is_empty() {
                                result_text = msg.to_string();
                            }
                            sink.on_event(StreamOutputEvent::Result {
                                text: result_text.clone(),
                                cost_usd,
                                is_error: true,
                                session_id: session_id.clone(),
                            });
                            break;
                        }
                    }
                }
            }
            Err(LineError::IdleTimeout) => {
                if checker.has_activity() {
                    tracing::warn!(
                        role = %role_upper,
                        "stdout idle for {STDOUT_IDLE_TIMEOUT_SECS}s but session-log or worktree activity detected — still working, continuing"
                    );
                    continue;
                }
                tracing::warn!(
                    role = %role_upper,
                    "agent process idle — no stdout or filesystem activity for {STDOUT_IDLE_TIMEOUT_SECS}s; killing"
                );
                let _ = child.kill();
                return Err(GroveError::Runtime(format!(
                    "agent process idle — no stdout or filesystem activity for {} seconds; process killed",
                    STDOUT_IDLE_TIMEOUT_SECS
                )));
            }
            Err(LineError::Eof) => break,
            Err(LineError::Io(_)) => break,
        }
    }

    // If we only got new-format events and broke on EOF (not TurnCompleted),
    // build result from accumulated messages.
    if result_text.is_empty() && !accumulated_messages.is_empty() {
        result_text = accumulated_messages.join("\n\n");
    }

    Ok(StreamResult {
        result_text,
        is_error,
        cost_usd,
        session_id,
    })
}

/// Known Claude Code tool names used for permission-denial detection.
static TOOL_NAMES: &[&str] = &[
    "Bash",
    "Read",
    "Write",
    "Edit",
    "Glob",
    "Grep",
    "LS",
    "WebFetch",
    "WebSearch",
    "NotebookEdit",
    "NotebookRead",
    "Task",
    "TodoRead",
    "TodoWrite",
];

/// Phrases Claude emits when blocked from using a tool.
static DENIAL_PHRASES: &[&str] = &[
    "not in my allowed tools",
    "is not in my allowed tools",
    "not allowed to use",
    "I cannot use",
    "I can't use",
    "not permitted to use",
    "not available in my allowed",
    "is not available",
    "is not allowed",
    "tool is not enabled",
];

/// Scan Claude's output for a permission denial. Returns `Some(PermissionRequest)`
/// if a known denial phrase is found alongside a recognisable tool name.
pub fn detect_permission_request(output: &str) -> Option<PermissionRequest> {
    let lower = output.to_lowercase();
    let has_denial = DENIAL_PHRASES
        .iter()
        .any(|p| lower.contains(&p.to_lowercase()));
    if !has_denial {
        return None;
    }

    let tool = TOOL_NAMES
        .iter()
        .find(|&&t| output.contains(t))
        .copied()
        .unwrap_or("unknown");

    let reason = output
        .lines()
        .find(|l| {
            let ll = l.to_lowercase();
            DENIAL_PHRASES
                .iter()
                .any(|p| ll.contains(&p.to_lowercase()))
        })
        .unwrap_or("Claude requires a tool not in the allowed list")
        .trim()
        .to_string();

    Some(PermissionRequest {
        tool: tool.to_string(),
        reason,
    })
}

/// Apply POSIX resource limits to `cmd` before it spawns the agent subprocess.
///
/// On non-Unix platforms this is a no-op.  Limits are applied best-effort —
/// a `setrlimit` failure is silently ignored so the spawn still succeeds.
pub(crate) fn apply_resource_limits(
    cmd: &mut Command,
    max_file_size_mb: Option<u32>,
    max_open_files: Option<u32>,
) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        if let Some(fsize_mb) = max_file_size_mb {
            let bytes = (fsize_mb as libc::rlim_t) * 1024 * 1024;
            // SAFETY: setrlimit is async-signal-safe and correct in a post-fork callback.
            unsafe {
                cmd.pre_exec(move || {
                    let limit = libc::rlimit {
                        rlim_cur: bytes,
                        rlim_max: bytes,
                    };
                    let _ = libc::setrlimit(libc::RLIMIT_FSIZE, &limit);
                    Ok(())
                });
            }
        }
        if let Some(max_files) = max_open_files {
            let n = max_files as libc::rlim_t;
            unsafe {
                cmd.pre_exec(move || {
                    let limit = libc::rlimit {
                        rlim_cur: n,
                        rlim_max: n,
                    };
                    let _ = libc::setrlimit(libc::RLIMIT_NOFILE, &limit);
                    Ok(())
                });
            }
        }
    }
    // Silence unused-variable warnings on non-Unix.
    #[cfg(not(unix))]
    {
        let _ = (cmd, max_file_size_mb, max_open_files);
    }
}

#[allow(dead_code)]
fn parse_output(stdout: &str) -> GroveResult<ProviderResponse> {
    let json_str = extract_json(stdout).unwrap_or(stdout);

    match serde_json::from_str::<ClaudeOutput>(json_str) {
        Ok(out) => {
            if out.is_error {
                return Err(GroveError::Runtime(format!(
                    "claude CLI reported error: {}",
                    out.result
                )));
            }
            Ok(ProviderResponse {
                summary: out.result,
                changed_files: vec![],
                cost_usd: out.cost_usd,
                provider_session_id: None,
                pid: None,
            })
        }
        Err(_) => Ok(ProviderResponse {
            summary: stdout.trim().to_string(),
            changed_files: vec![],
            cost_usd: None,
            provider_session_id: None,
            pid: None,
        }),
    }
}

#[allow(dead_code)]
/// Extract the last `{...}` block from a string that may contain non-JSON prefix output.
fn extract_json(s: &str) -> Option<&str> {
    let end = s.rfind('}')?;
    let before = &s[..=end];
    let start = before.rfind('{')?;
    Some(&s[start..=end])
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    use crate::config::PermissionMode;
    use crate::providers::{NullSink, Provider, ProviderRequest};

    use super::{ClaudeCodeProvider, collect_capped, collect_stream};

    /// Spawn a long-lived no-op child so we have a valid `Child` handle.
    fn dummy_child() -> std::process::Child {
        Command::new("true")
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawning `true` must succeed")
    }

    #[test]
    fn collect_capped_returns_all_lines_when_under_cap() {
        let input = "line one\nline two\nline three\n";
        let reader = Cursor::new(input);
        let mut child = dummy_child();
        let result = collect_capped(reader, 1024, "builder", &mut child, None, None).unwrap();
        assert_eq!(result, "line one\nline two\nline three\n");
    }

    #[test]
    fn collect_capped_errors_when_output_exceeds_cap() {
        // Cap of 10 bytes; each line is "aaaaaaaaaa\n" = 11 bytes after push_str + '\n'.
        let input = "aaaaaaaaaa\nbbbbbbbbbb\n";
        let reader = Cursor::new(input);
        let mut child = dummy_child();
        let result = collect_capped(reader, 10, "builder", &mut child, None, None);
        assert!(result.is_err(), "must error when output exceeds cap");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("cap"), "error message should mention cap");
    }

    #[test]
    fn collect_capped_at_exact_boundary_succeeds() {
        // Exactly cap bytes: "hello\n" = 6 bytes, cap = 6 — within limit.
        let input = "hello\n";
        let reader = Cursor::new(input);
        let mut child = dummy_child();
        let result = collect_capped(reader, 6, "tester", &mut child, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn collect_stream_captures_session_id_and_result() {
        let input = concat!(
            r#"{"type":"system","session_id":"sid-001","message":"Session started"}"#,
            "\n",
            r#"{"type":"assistant","message":"Working on it..."}"#,
            "\n",
            r#"{"type":"tool_use","name":"Read"}"#,
            "\n",
            r#"{"type":"result","result":"Done!","cost_usd":0.12,"is_error":false,"session_id":"sid-001"}"#,
            "\n",
        );
        let reader = Cursor::new(input);
        let mut child = dummy_child();
        let sr = collect_stream(reader, 1_000_000, "builder", &mut child, None).unwrap();
        assert_eq!(sr.result_text, "Done!");
        assert!(!sr.is_error);
        assert_eq!(sr.cost_usd, Some(0.12));
        assert_eq!(sr.session_id.as_deref(), Some("sid-001"));
    }

    #[test]
    fn collect_stream_handles_empty_input() {
        let input = "\n\n";
        let reader = Cursor::new(input);
        let mut child = dummy_child();
        let sr = collect_stream(reader, 1_000_000, "tester", &mut child, None).unwrap();
        assert_eq!(sr.result_text, "");
        assert!(!sr.is_error);
        assert!(sr.cost_usd.is_none());
        assert!(sr.session_id.is_none());
    }

    #[test]
    fn collect_stream_errors_on_cap_exceeded() {
        // Each line is ~70+ bytes; cap of 50 should trigger on second line.
        let input = concat!(
            r#"{"type":"system","session_id":"sid-001","message":"Session started"}"#,
            "\n",
            r#"{"type":"assistant","message":"This is a long assistant message that pushes over the cap"}"#,
            "\n",
        );
        let reader = Cursor::new(input);
        let mut child = dummy_child();
        let result = collect_stream(reader, 50, "builder", &mut child, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("cap"), "error should mention cap");
    }

    #[cfg(unix)]
    #[test]
    fn execute_streaming_does_not_hold_stdin_open_in_non_interactive_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("tempdir");
        let script_path = dir.path().join("fake-claude.sh");
        fs::write(
            &script_path,
            r#"#!/bin/sh
printf '%s\n' '{"type":"result","result":"Done!","is_error":false}'
cat >/dev/null
"#,
        )
        .expect("write script");
        let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

        let provider = ClaudeCodeProvider::new(
            script_path.to_string_lossy().to_string(),
            2,
            PermissionMode::SkipAll,
            vec![],
            None,
        );

        // Keep any registered stdin handle alive to model the GUI path.
        let held_input = Arc::new(Mutex::new(None));
        let held_input_cb = {
            let held_input = Arc::clone(&held_input);
            Arc::new(move |handle| {
                *held_input.lock().expect("lock input handle") = Some(handle);
            })
        };

        let request = ProviderRequest {
            objective: "test objective".to_string(),
            role: "build_prd".to_string(),
            worktree_path: dir.path().to_string_lossy().to_string(),
            instructions: "reply".to_string(),
            model: None,
            allowed_tools: None,
            timeout_override: Some(2),
            provider_session_id: None,
            log_dir: None,
            grove_session_id: None,
            input_handle_callback: Some(held_input_cb),
            mcp_config_path: None,
        };

        let response = provider
            .execute_streaming(&request, &NullSink)
            .expect("streaming execution should finish");

        assert_eq!(response.summary, "Done!");
        assert!(
            held_input.lock().expect("lock input handle").is_none(),
            "non-interactive streaming should not retain stdin handles"
        );
    }
}
