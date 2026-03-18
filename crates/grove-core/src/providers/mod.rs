pub mod agent_input;
pub mod budget_meter;
pub mod catalog;
pub mod claude_code;
pub mod claude_code_persistent;
pub mod coding_agent;
pub mod gates;
pub(crate) mod line_reader;
pub mod mcp_inject;
pub mod mock;
pub mod question_detector;
pub mod registry;
pub mod retry;
pub mod stream_parser;
pub mod timeout;

use serde::{Deserialize, Serialize};

use crate::errors::GroveResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionContinuityPolicy {
    None,
    DetachedResume,
    LockedPerProcess,
}

pub fn session_continuity_policy_for_provider_id(id: &str) -> SessionContinuityPolicy {
    match id {
        "claude_code" => SessionContinuityPolicy::LockedPerProcess,
        "claude_code_persistent" => SessionContinuityPolicy::None,
        other => {
            if let Some(adapter) = coding_agent::get_adapter(other) {
                adapter.session_continuity_policy()
            } else {
                SessionContinuityPolicy::None
            }
        }
    }
}

// ── Streaming infrastructure ─────────────────────────────────────────────────

/// Callback for real-time stream events from a provider.
pub trait StreamSink: Send + Sync {
    fn on_event(&self, event: StreamOutputEvent);
}

/// No-op sink for backward compatibility (CLI, tests, legacy callers).
pub struct NullSink;
impl StreamSink for NullSink {
    fn on_event(&self, _event: StreamOutputEvent) {}
}

/// A single event emitted during agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamOutputEvent {
    System {
        message: String,
        session_id: Option<String>,
    },
    AssistantText {
        text: String,
    },
    ToolUse {
        tool: String,
    },
    ToolResult {
        tool: String,
    },
    Result {
        text: String,
        cost_usd: Option<f64>,
        is_error: bool,
        session_id: Option<String>,
    },
    RawLine {
        line: String,
    },
    SkillLoaded {
        skill_name: String,
        skill_path: String,
    },
    PhaseStart {
        phase: String,
        run_id: String,
    },
    PhaseGate {
        phase: String,
        run_id: String,
        requires_approval: bool,
        checkpoint_id: i64,
    },
    PhaseEnd {
        phase: String,
        run_id: String,
        outcome: String,
    },
    Question {
        question: String,
        options: Vec<String>,
        blocking: bool,
    },
    UserAnswer {
        text: String,
    },
    /// Scope check passed — agent stayed within scope.
    ScopeCheckPassed {
        agent: String,
        artifact_count: usize,
    },
    /// Scope violation detected.
    ScopeViolation {
        agent: String,
        violations: Vec<serde_json::Value>,
        action: String,
        attempt: u32,
    },
    /// Agent is being retried after a scope violation was reverted.
    ScopeRetry {
        agent: String,
        attempt: u32,
        violation_summary: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
    pub objective: String,
    pub role: String,
    /// Absolute path to the worktree directory where the agent should work.
    pub worktree_path: String,
    /// Role-specific instructions telling this agent exactly what to do,
    /// including context from the previous agent's output.
    pub instructions: String,
    /// Optional Claude model override (e.g. "claude-opus-4-6"). `None` means
    /// use the provider's default model.
    pub model: Option<String>,
    /// Per-agent tool allowlist derived from `AgentType::allowed_tools()`. When `Some`,
    /// overrides the provider-level `allowed_tools` in Gate permission modes.
    /// `None` means use the provider default (no per-agent restriction).
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    /// Per-request timeout override in seconds. When `Some`, overrides the
    /// provider-level `timeout_secs`. Ignored by providers that don't support it.
    #[serde(default)]
    pub timeout_override: Option<u64>,
    /// Provider-side session ID for conversation resumption.
    /// When `Some`, the provider will pass `--session-id <id>` to resume a
    /// previous conversation. `None` starts a fresh session.
    #[serde(default)]
    pub provider_session_id: Option<String>,
    /// Directory for session log files. When `Some`, the provider tees raw
    /// CLI output to `{log_dir}/session-{grove_session_id}.jsonl`.
    #[serde(default)]
    pub log_dir: Option<String>,
    /// Grove-side session ID used to name the log file.
    #[serde(default)]
    pub grove_session_id: Option<String>,
    /// Callback to register the agent's stdin handle for answer write-back.
    /// Called by the provider after spawning the agent process.
    #[serde(skip)]
    pub input_handle_callback:
        Option<std::sync::Arc<dyn Fn(agent_input::AgentInputHandle) + Send + Sync>>,
    /// Path to an MCP config JSON file to pass to the agent via `--mcp-config`.
    /// When `Some`, the provider injects `--mcp-config <path>` into the agent's
    /// CLI arguments. Used for graph agents that need grove-mcp-server tools.
    #[serde(default)]
    pub mcp_config_path: Option<String>,
}

impl std::fmt::Debug for ProviderRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRequest")
            .field("objective", &self.objective)
            .field("role", &self.role)
            .field("worktree_path", &self.worktree_path)
            .field("instructions", &self.instructions)
            .field("model", &self.model)
            .field("allowed_tools", &self.allowed_tools)
            .field("timeout_override", &self.timeout_override)
            .field("provider_session_id", &self.provider_session_id)
            .field("log_dir", &self.log_dir)
            .field("grove_session_id", &self.grove_session_id)
            .field(
                "input_handle_callback",
                &self.input_handle_callback.as_ref().map(|_| "<callback>"),
            )
            .field("mcp_config_path", &self.mcp_config_path)
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
    pub summary: String,
    pub changed_files: Vec<String>,
    /// Cost reported by the provider in USD. `None` means not reported.
    pub cost_usd: Option<f64>,
    /// Provider-side session ID returned after execution.
    /// Stored in the DB so future runs can resume this conversation.
    #[serde(default)]
    pub provider_session_id: Option<String>,
    /// OS PID of the provider subprocess during execution.
    /// Stored in `sessions.pid` so `grove doctor` can detect zombie sessions.
    #[serde(default)]
    pub pid: Option<u32>,
}

pub trait Provider: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse>;

    /// Streaming execute. Default delegates to `execute()` with a single Result event.
    fn execute_streaming(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
    ) -> GroveResult<ProviderResponse> {
        let response = self.execute(request)?;
        sink.on_event(StreamOutputEvent::Result {
            text: response.summary.clone(),
            cost_usd: response.cost_usd,
            is_error: false,
            session_id: response.provider_session_id.clone(),
        });
        Ok(response)
    }

    /// Interactive streaming execute with Q&A support.
    ///
    /// When an agent emits a blocking question, the engine calls
    /// `qa_source.wait_for_answer(...)` and writes the response to the agent's
    /// stdin so it can continue. Default delegates to `execute_streaming`
    /// (ignoring `qa_source`).
    fn execute_interactive(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
        qa_source: &dyn QaSource,
    ) -> GroveResult<ProviderResponse> {
        let _ = qa_source; // default: ignore Q&A source
        self.execute_streaming(request, sink)
    }

    /// Whether this provider natively supports streaming output events.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// How this provider handles provider-native session continuity.
    fn session_continuity_policy(&self) -> SessionContinuityPolicy {
        SessionContinuityPolicy::None
    }

    /// Optional persistent multi-turn execution strategy.
    fn persistent_phase_provider(&self) -> Option<&dyn PersistentPhaseProvider> {
        None
    }

    /// Inject an abort handle so the provider can register subprocess PIDs
    /// and check for abort between operations. Default: no-op.
    fn set_abort_handle(&self, _handle: crate::orchestrator::abort_handle::AbortHandle) {}
}

pub trait PersistentPhaseProvider: Send + Sync {
    fn start_host(
        &self,
        run_id: &str,
        worktree_path: &str,
        model: Option<&str>,
        allowed_tools: Option<&[String]>,
        log_dir: Option<&str>,
        mcp_config_path: Option<&str>,
    ) -> GroveResult<claude_code_persistent::PersistentHost>;

    fn execute_persistent_turn(
        &self,
        host: &mut claude_code_persistent::PersistentHost,
        turn: &claude_code_persistent::PhaseTurn,
        sink: &dyn StreamSink,
        qa_source: &dyn QaSource,
        grove_session_id: &str,
    ) -> GroveResult<claude_code_persistent::PhaseTurnOutcome>;

    fn abort_host(&self, host: &mut claude_code_persistent::PersistentHost) -> GroveResult<()>;
}

// ── Q&A infrastructure ───────────────────────────────────────────────────────

/// Source of answers for agent questions during interactive execution.
///
/// Implementations are responsible for recording the question (e.g. in the DB)
/// and blocking until an answer arrives (e.g. from the GUI or CLI stdin).
pub trait QaSource: Send + Sync {
    /// Called when the agent emits a blocking question.
    ///
    /// Implementations should persist the question, then block until an answer
    /// is available (or a timeout expires). Returns the answer text.
    fn wait_for_answer(
        &self,
        run_id: &str,
        session_id: Option<&str>,
        question: &str,
        options: &[String],
    ) -> GroveResult<String>;
}

/// No-op Q&A source that returns an empty string, used when Q&A is disabled
/// (e.g. `--print` mode or agents that don't support interactive questions).
pub struct NoQaSource;

impl QaSource for NoQaSource {
    fn wait_for_answer(
        &self,
        _run_id: &str,
        _session_id: Option<&str>,
        _question: &str,
        _options: &[String],
    ) -> GroveResult<String> {
        Ok(String::new())
    }
}

/// Adapter that lets an `Arc<dyn Provider>` be used wherever `Box<dyn Provider>` is expected.
pub struct ArcProvider(pub std::sync::Arc<dyn Provider>);

impl Provider for ArcProvider {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        self.0.execute(request)
    }

    fn execute_streaming(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
    ) -> GroveResult<ProviderResponse> {
        self.0.execute_streaming(request, sink)
    }

    fn execute_interactive(
        &self,
        request: &ProviderRequest,
        sink: &dyn StreamSink,
        qa_source: &dyn QaSource,
    ) -> GroveResult<ProviderResponse> {
        self.0.execute_interactive(request, sink, qa_source)
    }

    fn supports_streaming(&self) -> bool {
        self.0.supports_streaming()
    }

    fn session_continuity_policy(&self) -> SessionContinuityPolicy {
        self.0.session_continuity_policy()
    }

    fn persistent_phase_provider(&self) -> Option<&dyn PersistentPhaseProvider> {
        self.0.persistent_phase_provider()
    }

    fn set_abort_handle(&self, handle: crate::orchestrator::abort_handle::AbortHandle) {
        self.0.set_abort_handle(handle);
    }
}

// Re-export the canonical implementations so existing code that uses
// `providers::MockProvider` or `providers::ClaudeCodeProvider` still compiles.
pub use crate::config::PermissionMode;
pub use claude_code::ClaudeCodeProvider;
pub use claude_code_persistent::ClaudeCodePersistentProvider;
pub use coding_agent::CodingAgentProvider;
pub use mock::MockProvider;
pub use registry::ProviderRegistry;

#[cfg(test)]
mod tests {
    use super::{SessionContinuityPolicy, session_continuity_policy_for_provider_id};

    #[test]
    fn continuity_policy_matches_provider_capabilities() {
        assert_eq!(
            session_continuity_policy_for_provider_id("claude_code"),
            SessionContinuityPolicy::LockedPerProcess
        );
        assert_eq!(
            session_continuity_policy_for_provider_id("claude_code_persistent"),
            SessionContinuityPolicy::None
        );
        assert_eq!(
            session_continuity_policy_for_provider_id("codex"),
            SessionContinuityPolicy::DetachedResume
        );
        assert_eq!(
            session_continuity_policy_for_provider_id("mock"),
            SessionContinuityPolicy::None
        );
    }
}
