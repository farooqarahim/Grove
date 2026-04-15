use std::io::Write;
use std::process::{Child, ChildStdin};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde::Deserialize;

use super::claude_code::ClaudeCodeProvider;
use super::line_reader::{LineError, TimedLineReader};
use super::stream_parser::{self, StreamEvent};
use super::{
    PersistentPhaseProvider, Provider, ProviderRequest, ProviderResponse, QaSource,
    SessionContinuityPolicy, StreamOutputEvent, StreamSink,
};
use crate::errors::{GroveError, GroveResult};
use crate::orchestrator::abort_handle::{AbortHandle, PidGuard};

const STDOUT_IDLE_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostState {
    Starting,
    Running,
    WaitingForQuestion,
    WaitingForGate,
    Completed,
    Failed,
    Aborted,
}

impl HostState {
    pub fn can_transition_to(self, target: HostState) -> bool {
        if target == HostState::Aborted {
            return true;
        }
        matches!(
            (self, target),
            (HostState::Starting, HostState::Running)
                | (HostState::Running, HostState::WaitingForQuestion)
                | (HostState::Running, HostState::WaitingForGate)
                | (HostState::Running, HostState::Completed)
                | (HostState::Running, HostState::Failed)
                | (HostState::WaitingForQuestion, HostState::Running)
                | (HostState::WaitingForGate, HostState::Running)
        )
    }
}

#[derive(Clone)]
pub struct PersistentRunControlHandle {
    pub run_id: String,
    pub tx: std::sync::mpsc::Sender<RunControlMessage>,
}

#[derive(Debug, Clone)]
pub enum RunControlMessage {
    GateDecision {
        checkpoint_id: i64,
        decision: String,
        notes: Option<String>,
    },
    Abort,
}

#[derive(Debug, Clone)]
pub struct PhaseTurn {
    pub phase: String,
    pub instructions: String,
    pub gate_context: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GroveControlBlock {
    pub grove_control: String,
    pub phase: String,
    pub summary: String,
    #[serde(default)]
    pub artifacts: Vec<String>,
    pub awaiting: String,
}

#[derive(Debug, Clone)]
pub enum PhaseTurnOutcome {
    TurnDone {
        response_text: String,
        cost_usd: Option<f64>,
        session_id: Option<String>,
        grove_control: Option<GroveControlBlock>,
    },
    HostDied {
        last_session_id: Option<String>,
        partial_output: String,
    },
}

/// Generalized persistent host that works for both Claude Code (long-lived
/// process with stdin/stdout) and coding agents (idle host, one-shot per phase).
pub struct PersistentHost {
    pub run_id: String,
    pub provider_thread_id: Option<String>,
    pub state: HostState,
    pub current_phase: Option<String>,
    // Process fields — `Some` for Claude (long-lived process), `None` for coding agents.
    pub child: Option<Child>,
    pub stdin: Option<ChildStdin>,
    pub stdout_reader: Option<TimedLineReader>,
    pub pid: Option<u32>,
    pub total_cost_usd: f64,
    pub turn_count: u32,
    pub last_activity_at: Instant,
    stderr_handle: Option<JoinHandle<String>>,
    _pid_guard: Option<PidGuard>,
    // Execution context — used by coding agents to build ProviderRequest per phase.
    pub worktree_path: String,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub log_dir: Option<String>,
    /// MCP config file path for pipeline worker agents. When set, the persistent
    /// turn request includes `--mcp-config` so the CLI can use MCP tools.
    pub mcp_config_path: Option<String>,
}

impl PersistentHost {
    /// Construct a host backed by a live Claude process (stdin/stdout connected).
    #[allow(clippy::too_many_arguments)]
    pub fn with_process(
        run_id: String,
        child: Child,
        stdin: ChildStdin,
        stdout_reader: TimedLineReader,
        pid: u32,
        stderr_handle: Option<JoinHandle<String>>,
        pid_guard: Option<PidGuard>,
        worktree_path: String,
        model: Option<String>,
        allowed_tools: Option<Vec<String>>,
        log_dir: Option<String>,
        mcp_config_path: Option<String>,
    ) -> Self {
        Self {
            run_id,
            provider_thread_id: None,
            state: HostState::Starting,
            current_phase: None,
            child: Some(child),
            stdin: Some(stdin),
            stdout_reader: Some(stdout_reader),
            pid: Some(pid),
            total_cost_usd: 0.0,
            turn_count: 0,
            last_activity_at: Instant::now(),
            stderr_handle,
            _pid_guard: pid_guard,
            worktree_path,
            model,
            allowed_tools,
            log_dir,
            mcp_config_path,
        }
    }

    /// Construct an idle host (no live process). Used by coding agents that
    /// spawn a fresh process per phase via `execute_interactive`.
    pub fn idle(
        run_id: String,
        worktree_path: String,
        model: Option<String>,
        allowed_tools: Option<Vec<String>>,
        log_dir: Option<String>,
        mcp_config_path: Option<String>,
    ) -> Self {
        Self {
            run_id,
            provider_thread_id: None,
            state: HostState::Starting,
            current_phase: None,
            child: None,
            stdin: None,
            stdout_reader: None,
            pid: None,
            total_cost_usd: 0.0,
            turn_count: 0,
            last_activity_at: Instant::now(),
            stderr_handle: None,
            _pid_guard: None,
            worktree_path,
            model,
            allowed_tools,
            log_dir,
            mcp_config_path,
        }
    }

    pub fn transition(&mut self, target: HostState) -> GroveResult<()> {
        if !self.state.can_transition_to(target) {
            return Err(GroveError::Runtime(format!(
                "invalid persistent host transition {:?} -> {:?}",
                self.state, target
            )));
        }
        self.state = target;
        self.last_activity_at = Instant::now();
        Ok(())
    }

    pub fn send_user_turn(&mut self, text: &str) -> GroveResult<()> {
        let stdin = self.stdin.as_mut().ok_or_else(|| {
            GroveError::Runtime("cannot send user turn: no stdin (idle host)".into())
        })?;
        write_json_input(stdin, text)?;
        self.turn_count += 1;
        self.last_activity_at = Instant::now();
        Ok(())
    }

    pub fn is_alive(&mut self) -> bool {
        match self.child.as_mut() {
            Some(child) => child.try_wait().ok().flatten().is_none(),
            None => false,
        }
    }

    pub fn shutdown(&mut self, target: HostState) -> GroveResult<()> {
        let _ = self.transition(target);
        if let Some(ref mut child) = self.child {
            // Inline alive check to avoid double mutable borrow of self.
            let alive = child.try_wait().ok().flatten().is_none();
            if alive {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        if let Some(handle) = self.stderr_handle.take() {
            let stderr = handle.join().unwrap_or_default();
            if !stderr.trim().is_empty() {
                tracing::warn!(run_id = %self.run_id, stderr = %stderr.trim(), "persistent host stderr");
            }
        }
        Ok(())
    }

    pub fn add_cost(&mut self, cost: f64) {
        self.total_cost_usd += cost;
    }

    pub fn set_provider_thread_id(&mut self, id: String) {
        self.provider_thread_id = Some(id);
    }
}

#[derive(Debug)]
pub struct ClaudeCodePersistentProvider {
    inner: ClaudeCodeProvider,
    prefer_long_lived_run_host: bool,
}

impl ClaudeCodePersistentProvider {
    pub fn new(inner: ClaudeCodeProvider, prefer_long_lived_run_host: bool) -> Self {
        Self {
            inner,
            prefer_long_lived_run_host,
        }
    }
}

impl Provider for ClaudeCodePersistentProvider {
    fn name(&self) -> &'static str {
        "claude_code"
    }

    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        self.inner.execute(request)
    }

    fn execute_streaming(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
    ) -> GroveResult<ProviderResponse> {
        self.inner.execute_streaming(request, sink)
    }

    fn execute_interactive(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
        qa_source: &dyn QaSource,
    ) -> GroveResult<ProviderResponse> {
        self.inner.execute_interactive(request, sink, qa_source)
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn session_continuity_policy(&self) -> SessionContinuityPolicy {
        SessionContinuityPolicy::None
    }

    fn persistent_phase_provider(&self) -> Option<&dyn PersistentPhaseProvider> {
        Some(self)
    }

    fn set_abort_handle(&self, handle: AbortHandle) {
        self.inner.set_abort_handle(handle);
    }
}

impl PersistentPhaseProvider for ClaudeCodePersistentProvider {
    fn start_host(
        &self,
        run_id: &str,
        worktree_path: &str,
        model: Option<&str>,
        allowed_tools: Option<&[String]>,
        log_dir: Option<&str>,
        mcp_config_path: Option<&str>,
    ) -> GroveResult<PersistentHost> {
        if self.prefer_long_lived_run_host {
            tracing::warn!(
                run_id = %run_id,
                "long-lived Claude run host requested, but Claude CLI requires the top-level prompt as a positional argument and only supports stream-json stdin with --print; falling back to one-shot turns"
            );
        }

        Ok(PersistentHost::idle(
            run_id.to_string(),
            worktree_path.to_string(),
            model.map(|m| m.to_string()),
            allowed_tools.map(|t| t.to_vec()),
            log_dir.map(|d| d.to_string()),
            mcp_config_path.map(|p| p.to_string()),
        ))
    }

    fn execute_persistent_turn(
        &self,
        host: &mut PersistentHost,
        turn: &PhaseTurn,
        sink: &dyn StreamSink,
        _qa_source: &dyn QaSource,
        grove_session_id: &str,
    ) -> GroveResult<PhaseTurnOutcome> {
        host.current_phase = Some(turn.phase.clone());
        let _ = host.transition(HostState::Running);

        let request = build_persistent_turn_request(host, turn, grove_session_id);

        tracing::info!(
            run_id = %host.run_id,
            phase = %turn.phase,
            last_provider_session_id = ?host.provider_thread_id,
            grove_session_id = %grove_session_id,
            live_host = host.child.is_some(),
            "executing persistent turn"
        );

        let (response_text, cost_usd, provider_session_id, grove_control) = if host.child.is_some()
        {
            host.send_user_turn(&request.instructions)?;
            match collect_persistent_turn(
                host,
                sink,
                _qa_source,
                None,
                Duration::from_secs(self.inner.timeout_secs),
            )? {
                PhaseTurnOutcome::TurnDone {
                    response_text,
                    cost_usd,
                    session_id,
                    grove_control,
                } => (response_text, cost_usd, session_id, grove_control),
                PhaseTurnOutcome::HostDied {
                    last_session_id,
                    partial_output,
                } => {
                    return Ok(PhaseTurnOutcome::HostDied {
                        last_session_id,
                        partial_output,
                    });
                }
            }
        } else {
            let response = self.inner.execute_streaming(&request, sink)?;
            (
                response.summary,
                response.cost_usd,
                response.provider_session_id,
                None,
            )
        };

        // Track cost.
        if let Some(cost) = cost_usd {
            host.add_cost(cost);
        }

        // Capture the provider session ID for diagnostics on this phase.
        if let Some(ref sid) = provider_session_id {
            host.set_provider_thread_id(sid.clone());
        }

        host.turn_count += 1;
        host.last_activity_at = std::time::Instant::now();

        let grove_control = grove_control.or_else(|| extract_grove_control_block(&response_text));

        Ok(PhaseTurnOutcome::TurnDone {
            response_text,
            cost_usd,
            session_id: provider_session_id,
            grove_control,
        })
    }

    fn abort_host(&self, host: &mut PersistentHost) -> GroveResult<()> {
        host.shutdown(HostState::Aborted)
    }
}

fn build_persistent_turn_request(
    host: &PersistentHost,
    turn: &PhaseTurn,
    grove_session_id: &str,
) -> super::ProviderRequest {
    let prompt = format_phase_prompt(turn);

    super::ProviderRequest {
        objective: prompt.clone(),
        role: turn.phase.clone(),
        worktree_path: host.worktree_path.clone(),
        instructions: prompt,
        model: host.model.clone(),
        allowed_tools: host.allowed_tools.clone(),
        timeout_override: None,
        // One-shot `claude --print` turns cannot safely reuse a prior
        // `--session-id`; Claude rejects that mode with "already in use".
        // Continuity comes from the shared worktree and phase artifacts.
        provider_session_id: None,
        log_dir: host.log_dir.clone(),
        grove_session_id: Some(grove_session_id.to_string()),
        input_handle_callback: None,
        mcp_config_path: host.mcp_config_path.clone(),
        conversation_id: None,
    }
}

pub fn format_phase_prompt(turn: &PhaseTurn) -> String {
    let mut prompt = String::new();
    if let Some(ref gate_context) = turn.gate_context {
        prompt.push_str(gate_context.trim());
        prompt.push_str("\n\n");
    }
    prompt.push_str(
        "You are continuing the same Grove run.\n\
         Use the current worktree contents, generated artifacts, handoff context, and any gate decision notes as your source of continuity.\n",
    );
    prompt.push_str(&format!("Current phase: {}\n\n", turn.phase));
    prompt.push_str(turn.instructions.trim());
    prompt.push_str(
        "\n\nWhen you finish this phase:\n\
         1. Write the required artifact.\n\
         2. Output a short phase summary.\n\
         3. Emit exactly one Grove control block as the last thing you output:\n\
         {\"grove_control\":\"phase_complete\",\"phase\":\"",
    );
    prompt.push_str(&turn.phase);
    prompt.push_str(
        "\",\"summary\":\"<your summary>\",\"artifacts\":[\"<artifact paths>\"],\"awaiting\":\"gate\"}\n\
         4. Stop and wait for Grove gate decision.\n\n\
         Do not continue to the next phase until Grove sends a gate decision message.",
    );
    prompt
}

pub fn collect_persistent_turn(
    host: &mut PersistentHost,
    sink: &dyn StreamSink,
    qa_source: &dyn QaSource,
    mut log_file: Option<std::fs::File>,
    turn_timeout: Duration,
) -> GroveResult<PhaseTurnOutcome> {
    // This function requires a live process — only used by Claude persistent host.
    // Take the reader out of the Option to avoid double-mutable-borrow of `host`
    // when we need to call `host.transition()` in the loop body.
    let stdout_reader = host.stdout_reader.take().ok_or_else(|| {
        GroveError::Runtime(
            "collect_persistent_turn requires a live stdout reader (Claude host)".into(),
        )
    })?;

    let role_upper = host
        .current_phase
        .as_deref()
        .unwrap_or("agent")
        .to_uppercase();
    let mut total_bytes = 0usize;
    let mut result_text = String::new();
    let mut cost_usd: Option<f64> = None;
    let mut session_id = host.provider_thread_id.clone();
    let mut assistant_lines = 0u32;
    let mut accumulated_messages: Vec<String> = Vec::new();
    let mut control_block: Option<GroveControlBlock> = None;
    let started_at = Instant::now();

    loop {
        if started_at.elapsed() > turn_timeout {
            if let Some(ref mut child) = host.child {
                let _ = child.kill();
            }
            return Err(GroveError::Runtime(format!(
                "persistent claude turn timed out after {} seconds",
                turn_timeout.as_secs()
            )));
        }

        match stdout_reader.next_line() {
            Ok(line) => {
                host.last_activity_at = Instant::now();
                total_bytes += line.len() + 1;
                if total_bytes > 10 * 1024 * 1024 {
                    if let Some(ref mut child) = host.child {
                        let _ = child.kill();
                    }
                    return Err(GroveError::Runtime(
                        "persistent claude output exceeded cap of 10485760 bytes".into(),
                    ));
                }
                if let Some(ref mut file) = log_file {
                    let _ = writeln!(file, "{}", line);
                }
                if let Some(event) = stream_parser::parse_event(&line) {
                    match event {
                        StreamEvent::System(ref sys) => {
                            if let Some(ref sid) = sys.session_id {
                                session_id = Some(sid.clone());
                            }
                            sink.on_event(StreamOutputEvent::System {
                                message: sys.message.clone().unwrap_or_default(),
                                session_id: sys.session_id.clone(),
                            });
                        }
                        StreamEvent::Assistant(ref assistant) => {
                            if let Some(ref message) = assistant.message {
                                if assistant_lines < 5 {
                                    eprintln!("[{}] {}", role_upper, message);
                                    assistant_lines += 1;
                                }
                                accumulated_messages.push(message.clone());
                                if control_block.is_none() {
                                    control_block = extract_grove_control_block(message);
                                }
                            }
                            sink.on_event(StreamOutputEvent::AssistantText {
                                text: assistant.message.clone().unwrap_or_default(),
                            });
                        }
                        StreamEvent::ToolUse(ref tu) => {
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
                            cost_usd = res.cost_usd;
                            if let Some(ref sid) = res.session_id {
                                session_id = Some(sid.clone());
                            }
                            if control_block.is_none() {
                                control_block = extract_grove_control_block(&result_text);
                            }
                            sink.on_event(StreamOutputEvent::Result {
                                text: res.result.clone(),
                                cost_usd: res.cost_usd,
                                is_error: res.is_error,
                                session_id: res.session_id.clone(),
                            });
                        }
                        StreamEvent::Question(ref question) => {
                            host.transition(HostState::WaitingForQuestion)?;
                            sink.on_event(StreamOutputEvent::Question {
                                question: question.question.clone(),
                                options: question.options.clone(),
                                blocking: question.blocking,
                            });
                            if question.blocking {
                                let answer = qa_source.wait_for_answer(
                                    &host.run_id,
                                    session_id.as_deref(),
                                    &question.question,
                                    &question.options,
                                )?;
                                if !answer.is_empty() {
                                    let stdin = host.stdin.as_mut().ok_or_else(|| {
                                        GroveError::Runtime("no stdin for Q&A answer".into())
                                    })?;
                                    write_json_input(stdin, &answer)?;
                                    sink.on_event(StreamOutputEvent::UserAnswer { text: answer });
                                }
                            }
                            host.transition(HostState::Running)?;
                        }
                        StreamEvent::ThreadStarted(ref ts) => {
                            if let Some(ref thread_id) = ts.thread_id {
                                session_id = Some(thread_id.clone());
                            }
                            sink.on_event(StreamOutputEvent::System {
                                message: String::new(),
                                session_id: ts.thread_id.clone(),
                            });
                        }
                        StreamEvent::TurnStarted {} => {}
                        StreamEvent::ItemCompleted(ref item_completed) => {
                            if let Some(ref item) = item_completed.item {
                                if item.item_type.as_deref() == Some("agent_message") {
                                    if let Some(ref text) = item.text {
                                        if !text.trim().is_empty() {
                                            if assistant_lines < 5 {
                                                eprintln!("[{}] {}", role_upper, text);
                                                assistant_lines += 1;
                                            }
                                            accumulated_messages.push(text.clone());
                                            if control_block.is_none() {
                                                control_block = extract_grove_control_block(text);
                                            }
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
                            if control_block.is_none() {
                                control_block = extract_grove_control_block(&result_text);
                            }
                            // Restore the reader so the host can be reused for the next turn.
                            host.stdout_reader = Some(stdout_reader);
                            return Ok(PhaseTurnOutcome::TurnDone {
                                response_text: result_text,
                                cost_usd,
                                session_id,
                                grove_control: control_block,
                            });
                        }
                        StreamEvent::TurnFailed(ref failed) => {
                            let msg = failed
                                .error
                                .as_ref()
                                .and_then(|e| e.message.as_deref())
                                .unwrap_or("persistent claude turn failed");
                            return Err(GroveError::Runtime(msg.to_string()));
                        }
                    }
                }
            }
            Err(LineError::IdleTimeout) => {
                if let Some(ref mut child) = host.child {
                    let _ = child.kill();
                }
                return Err(GroveError::Runtime(format!(
                    "persistent claude host idle for {} seconds; process killed",
                    STDOUT_IDLE_TIMEOUT_SECS
                )));
            }
            Err(LineError::Eof) | Err(LineError::Io(_)) => {
                if result_text.is_empty() && !accumulated_messages.is_empty() {
                    result_text = accumulated_messages.join("\n\n");
                }
                return Ok(PhaseTurnOutcome::HostDied {
                    last_session_id: session_id,
                    partial_output: result_text,
                });
            }
        }
    }
}

pub fn extract_grove_control_block(text: &str) -> Option<GroveControlBlock> {
    for line in text.lines().rev() {
        let trimmed = line.trim().trim_matches('`').trim();
        if trimmed.starts_with('{') && trimmed.contains("\"grove_control\"") {
            if let Ok(parsed) = serde_json::from_str::<GroveControlBlock>(trimmed) {
                return Some(parsed);
            }
        }
    }

    let compact = text.replace("```json", "").replace("```", "");
    if let Some(start) = compact.rfind("{\"grove_control\"") {
        let candidate = &compact[start..];
        if let Some(end) = candidate.rfind('}') {
            return serde_json::from_str::<GroveControlBlock>(&candidate[..=end]).ok();
        }
    }

    None
}

fn write_json_input(stdin: &mut ChildStdin, text: &str) -> GroveResult<()> {
    let payload = serde_json::json!({ "type": "user_input", "text": text });
    writeln!(stdin, "{payload}")?;
    stdin.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_state_machine_rejects_invalid_transition() {
        assert!(HostState::Starting.can_transition_to(HostState::Running));
        assert!(!HostState::Starting.can_transition_to(HostState::Completed));
        assert!(HostState::WaitingForGate.can_transition_to(HostState::Running));
    }

    #[test]
    fn parses_grove_control_block_from_output() {
        let text = "Summary line\n{\"grove_control\":\"phase_complete\",\"phase\":\"builder\",\"summary\":\"done\",\"artifacts\":[\"GROVE_PRD.md\"],\"awaiting\":\"gate\"}";
        let parsed = extract_grove_control_block(text).expect("control block");
        assert_eq!(
            parsed,
            GroveControlBlock {
                grove_control: "phase_complete".into(),
                phase: "builder".into(),
                summary: "done".into(),
                artifacts: vec!["GROVE_PRD.md".into()],
                awaiting: "gate".into(),
            }
        );
    }

    #[test]
    fn formats_phase_prompt_with_gate_context() {
        let turn = PhaseTurn {
            phase: "reviewer".into(),
            instructions: "Review the implementation.".into(),
            gate_context: Some("Gate decision for checkpoint 9: approved".into()),
        };
        let prompt = format_phase_prompt(&turn);
        assert!(prompt.contains("Gate decision for checkpoint 9: approved"));
        assert!(prompt.contains("continuing the same Grove run"));
        assert!(prompt.contains("source of continuity"));
        assert!(prompt.contains("Current phase: reviewer"));
        assert!(prompt.contains("\"grove_control\":\"phase_complete\""));
    }

    #[test]
    fn persistent_turn_request_uses_grove_session_logging_without_provider_resume() {
        let mut host = PersistentHost::idle(
            "run_123".into(),
            "/tmp/worktree".into(),
            Some("claude-test".into()),
            Some(vec!["Read".into()]),
            Some("/tmp/logs".into()),
            Some("/tmp/run-mcp.json".into()),
        );
        host.set_provider_thread_id("provider-session-1".into());
        let turn = PhaseTurn {
            phase: "plan_system_design".into(),
            instructions: "Write the design artifact.".into(),
            gate_context: None,
        };

        let request = build_persistent_turn_request(&host, &turn, "sess_abc");

        assert_eq!(request.provider_session_id, None);
        assert_eq!(request.grove_session_id.as_deref(), Some("sess_abc"));
        assert_eq!(request.log_dir.as_deref(), Some("/tmp/logs"));
        assert_eq!(
            request.mcp_config_path.as_deref(),
            Some("/tmp/run-mcp.json")
        );
        assert_eq!(request.role, "plan_system_design");
    }

    #[test]
    #[ignore = "requires a real claude CLI session"]
    fn claude_accepts_multiple_turns_over_stdin() {
        // Transport validation for real environments is intentionally manual/ignored.
    }
}
