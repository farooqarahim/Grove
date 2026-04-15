// Test approach: full execute_objective integration test using a RecordingProvider
// that implements both Provider and PersistentPhaseProvider with
// SessionContinuityPolicy::LockedPerProcess. The provider is wired through the
// real orchestrator (tempfile DB + GroveConfig) so both the DB-level seeding in
// orchestrator/mod.rs and the host-level seeding in engine.rs are exercised
// end-to-end. This is stronger than a unit-only test.
//
// To verify the fix: the RecordingProvider captures provider_session_id from the
// ProviderRequest passed to its execute_persistent_turn. Before the fix, only
// DetachedResume providers received the resumed session id; LockedPerProcess
// providers silently got None.

use std::sync::{Arc, Mutex};

use grove_core::config::{DEFAULT_CONFIG_YAML, GroveConfig};
use grove_core::db;
use grove_core::orchestrator::{RunOptions, execute_objective};
use grove_core::providers::claude_code_persistent::{
    HostState, PersistentHost, PhaseTurn, PhaseTurnOutcome,
};
use grove_core::providers::{
    PersistentPhaseProvider, Provider, ProviderRequest, ProviderResponse, QaSource,
    SessionContinuityPolicy, StreamSink,
};
use tempfile::TempDir;

// ── RecordingProvider ──────────────────────────────────────────────────────────

/// A minimal provider that records the `provider_session_id` field of every
/// ProviderRequest built during execute_persistent_turn. Returns a fixed
/// successful response so the run completes without error.
struct RecordingProvider {
    captured_session_ids: Arc<Mutex<Vec<Option<String>>>>,
}

impl RecordingProvider {
    fn new() -> (Self, Arc<Mutex<Vec<Option<String>>>>) {
        let shared = Arc::new(Mutex::new(Vec::new()));
        (
            RecordingProvider {
                captured_session_ids: Arc::clone(&shared),
            },
            shared,
        )
    }
}

impl Provider for RecordingProvider {
    fn name(&self) -> &'static str {
        "recording"
    }

    fn session_continuity_policy(&self) -> SessionContinuityPolicy {
        SessionContinuityPolicy::LockedPerProcess
    }

    fn persistent_phase_provider(&self) -> Option<&dyn PersistentPhaseProvider> {
        Some(self)
    }

    fn execute(
        &self,
        request: &ProviderRequest,
    ) -> grove_core::errors::GroveResult<ProviderResponse> {
        // Fallback for non-persistent paths (e.g. conflict resolution agent).
        self.captured_session_ids
            .lock()
            .unwrap()
            .push(request.provider_session_id.clone());
        Ok(ProviderResponse {
            summary: format!("recording provider handled role '{}'", request.role),
            changed_files: vec![],
            cost_usd: Some(0.0),
            provider_session_id: None,
            pid: None,
        })
    }
}

impl PersistentPhaseProvider for RecordingProvider {
    fn start_host(
        &self,
        run_id: &str,
        worktree_path: &str,
        model: Option<&str>,
        _allowed_tools: Option<&[String]>,
        log_dir: Option<&str>,
        mcp_config_path: Option<&str>,
    ) -> grove_core::errors::GroveResult<PersistentHost> {
        Ok(PersistentHost::idle(
            run_id.to_string(),
            worktree_path.to_string(),
            model.map(str::to_string),
            None,
            log_dir.map(str::to_string),
            mcp_config_path.map(str::to_string),
        ))
    }

    fn execute_persistent_turn(
        &self,
        host: &mut PersistentHost,
        turn: &PhaseTurn,
        _sink: &dyn StreamSink,
        _qa_source: &dyn QaSource,
        grove_session_id: &str,
    ) -> grove_core::errors::GroveResult<PhaseTurnOutcome> {
        // Transition host to Running (mirrors MockProvider behaviour).
        let _ = host.transition(HostState::Running);

        // Build a ProviderRequest the same way a real coding-agent provider would:
        // seed provider_session_id from the host's provider_thread_id so the
        // engine's host.set_provider_thread_id() call is observable.
        let request = ProviderRequest {
            objective: turn.instructions.clone(),
            role: turn.phase.clone(),
            worktree_path: host.worktree_path.clone(),
            instructions: turn.instructions.clone(),
            model: host.model.clone(),
            allowed_tools: None,
            timeout_override: None,
            // This is the field under test: the engine must seed host.provider_thread_id
            // before calling execute_persistent_turn for LockedPerProcess providers.
            provider_session_id: host.provider_thread_id.clone(),
            log_dir: host.log_dir.clone(),
            grove_session_id: Some(grove_session_id.to_string()),
            input_handle_callback: None,
            mcp_config_path: host.mcp_config_path.clone(),
            conversation_id: None,
        };

        // Record the session id for the test assertion.
        self.captured_session_ids
            .lock()
            .unwrap()
            .push(request.provider_session_id.clone());

        Ok(PhaseTurnOutcome::TurnDone {
            response_text: format!(
                "recording provider handled phase '{}' for objective '{}'",
                turn.phase, turn.instructions
            ),
            cost_usd: Some(0.0),
            session_id: None,
            grove_control: None,
        })
    }

    fn abort_host(&self, host: &mut PersistentHost) -> grove_core::errors::GroveResult<()> {
        host.shutdown(HostState::Aborted)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn minimal_config() -> GroveConfig {
    let mut cfg: GroveConfig = serde_yaml::from_str(DEFAULT_CONFIG_YAML).unwrap();
    cfg.providers.claude_code.enabled = false;
    cfg.orchestration.enforce_design_first = false;
    cfg.agents.reviewer.enabled = false;
    cfg.agents.qa.enabled = false;
    cfg.agents.security.enabled = false;
    cfg.agents.validator.enabled = false;
    cfg.agents.prd.enabled = false;
    cfg.agents.spec.enabled = false;
    cfg.agents.documenter.enabled = false;
    cfg.agents.reporter.enabled = false;
    cfg.agents.compliance.enabled = false;
    cfg.agents.dependency_manager.enabled = false;
    cfg.agents.optimizer.enabled = false;
    cfg.agents.accessibility.enabled = false;
    // Disable strict verdicts so the mock/recording provider's plain-text
    // summary is treated as an implicit APPROVED rather than failing the run.
    cfg.discipline.strict_verdicts = false;
    cfg
}

fn minimal_options() -> RunOptions {
    RunOptions {
        budget_usd: None,
        max_agents: None,
        model: None,
        interactive: false,
        pause_after: vec![],
        disable_phase_gates: false,
        permission_mode: None,
        pipeline: None,
        conversation_id: None,
        continue_last: false,
        db_path: None,
        abort_handle: None,
        issue_id: None,
        issue: None,
        provider: None,
        on_run_created: None,
        resume_provider_session_id: None,
        input_handle_callback: None,
        run_control_callback: None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

/// The core regression test: a provider with LockedPerProcess policy must receive
/// the resumed session id that was passed via options.resume_provider_session_id.
///
/// Before the fix in orchestrator/mod.rs and engine.rs, only DetachedResume
/// providers had the id seeded; LockedPerProcess fell through to None, silently
/// dropping it.
#[test]
fn locked_per_process_receives_seeded_session_id() {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let cfg = minimal_config();

    let (provider, captured) = RecordingProvider::new();
    let provider: Arc<dyn Provider> = Arc::new(provider);

    let mut opts = minimal_options();
    opts.resume_provider_session_id = Some("SID-123".to_string());

    let result =
        execute_objective(dir.path(), &cfg, "test resume seeding", opts, provider).unwrap();

    // The run must have completed (not failed due to provider error).
    assert_eq!(
        result.state, "completed",
        "run must complete without error; got state '{}'",
        result.state
    );

    let ids = captured.lock().unwrap();
    assert!(
        !ids.is_empty(),
        "provider must be called at least once by the orchestrator"
    );
    // The first agent call must carry the seeded session id.
    assert_eq!(
        ids[0].as_deref(),
        Some("SID-123"),
        "first provider call must have provider_session_id == Some(\"SID-123\"); got {:?}",
        ids[0]
    );
}

/// Confirm the None variant is unaffected: passing no resume id still results in
/// the provider receiving None for provider_session_id.
#[test]
fn no_resume_id_yields_none_session_id() {
    let dir = TempDir::new().unwrap();
    db::initialize(dir.path()).unwrap();
    let cfg = minimal_config();

    let (provider, captured) = RecordingProvider::new();
    let provider: Arc<dyn Provider> = Arc::new(provider);

    let result = execute_objective(
        dir.path(),
        &cfg,
        "test no resume seeding",
        minimal_options(),
        provider,
    )
    .unwrap();

    assert_eq!(
        result.state, "completed",
        "run must complete without error; got state '{}'",
        result.state
    );

    let ids = captured.lock().unwrap();
    assert!(
        !ids.is_empty(),
        "provider must be called at least once by the orchestrator"
    );
    assert_eq!(
        ids[0].as_deref(),
        None,
        "without a resume id, provider_session_id must be None; got {:?}",
        ids[0]
    );
}
