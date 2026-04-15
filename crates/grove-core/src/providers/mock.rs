use std::env;
use std::thread;
use std::time::Duration;

use super::claude_code_persistent::{HostState, PersistentHost, PhaseTurn, PhaseTurnOutcome};
use super::{
    PersistentPhaseProvider, Provider, ProviderRequest, ProviderResponse, QaSource, StreamSink,
};
use crate::errors::GroveResult;

/// Deterministic mock provider for tests.
///
/// Responses are keyed by `request.role`. Set `GROVE_MOCK_DELAY_MS` to
/// simulate latency (default 0 ms).
#[derive(Debug, Default)]
pub struct MockProvider;

impl Provider for MockProvider {
    fn name(&self) -> &'static str {
        "mock"
    }

    fn persistent_phase_provider(&self) -> Option<&dyn PersistentPhaseProvider> {
        Some(self)
    }

    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        let delay_ms: u64 = env::var("GROVE_MOCK_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);

        if delay_ms > 0 {
            thread::sleep(Duration::from_millis(delay_ms));
        }

        let (summary, changed_files) = match request.role.as_str() {
            "architect" => (
                format!(
                    "Architect analysed objective '{}' and produced a 3-step plan: \
                     [design data model, define API surface, specify test cases]",
                    request.objective
                ),
                vec!["docs/architecture.md".to_string()],
            ),
            "builder" => (
                format!(
                    "Builder implemented objective '{}': created source files, \
                     wired dependencies, added structured logging",
                    request.objective
                ),
                vec!["src/lib.rs".to_string(), "src/main.rs".to_string()],
            ),
            "tester" => (
                format!(
                    "Tester verified objective '{}': 12 unit tests pass, \
                     2 integration tests pass, 0 failures",
                    request.objective
                ),
                vec!["tests/unit_tests.rs".to_string()],
            ),
            other => (
                format!(
                    "Mock agent '{other}' completed objective '{}'",
                    request.objective
                ),
                vec![],
            ),
        };

        Ok(ProviderResponse {
            summary,
            changed_files,
            cost_usd: Some(0.0),
            provider_session_id: None,
            pid: None,
        })
    }
}

impl PersistentPhaseProvider for MockProvider {
    fn start_host(
        &self,
        run_id: &str,
        worktree_path: &str,
        model: Option<&str>,
        _allowed_tools: Option<&[String]>,
        log_dir: Option<&str>,
        mcp_config_path: Option<&str>,
    ) -> GroveResult<PersistentHost> {
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
    ) -> GroveResult<PhaseTurnOutcome> {
        // Transition host to Running (from Starting or WaitingForGate).
        let _ = host.transition(HostState::Running);
        let request = ProviderRequest {
            objective: turn.instructions.clone(),
            role: turn.phase.clone(),
            worktree_path: host.worktree_path.clone(),
            instructions: turn.instructions.clone(),
            model: host.model.clone(),
            allowed_tools: None,
            timeout_override: None,
            provider_session_id: None,
            log_dir: host.log_dir.clone(),
            grove_session_id: Some(grove_session_id.to_string()),
            input_handle_callback: None,
            mcp_config_path: host.mcp_config_path.clone(),
            conversation_id: None,
        };
        let response = self.execute(&request)?;
        if let Some(c) = response.cost_usd {
            host.add_cost(c);
        }
        host.turn_count += 1;
        Ok(PhaseTurnOutcome::TurnDone {
            response_text: response.summary,
            cost_usd: response.cost_usd,
            session_id: response.provider_session_id,
            grove_control: None,
        })
    }

    fn abort_host(&self, host: &mut PersistentHost) -> GroveResult<()> {
        host.shutdown(HostState::Aborted)
    }
}
