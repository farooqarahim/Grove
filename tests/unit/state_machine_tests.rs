use grove_core::orchestrator::RunState;
use grove_core::orchestrator::state_machine::is_valid_run_transition as is_valid_transition;

#[test]
fn all_legal_transitions_return_true() {
    let legal = [
        (RunState::Created, RunState::Planning),
        (RunState::Planning, RunState::Executing),
        (RunState::Executing, RunState::Verifying),
        (RunState::Executing, RunState::Failed),
        (RunState::Executing, RunState::Paused),
        (RunState::Verifying, RunState::Publishing),
        (RunState::Verifying, RunState::Failed),
        (RunState::Publishing, RunState::Completed),
        (RunState::Publishing, RunState::Failed),
        (RunState::Failed, RunState::Executing),
        (RunState::Paused, RunState::Executing),
    ];
    for (from, to) in legal {
        assert!(
            is_valid_transition(from, to),
            "{:?} → {:?} should be a valid transition",
            from,
            to
        );
    }
}

#[test]
fn all_illegal_transitions_return_false() {
    let illegal = [
        (RunState::Completed, RunState::Executing),
        (RunState::Executing, RunState::Created),
        (RunState::Created, RunState::Completed),  // skip
        (RunState::Planning, RunState::Completed), // skip
        (RunState::Planning, RunState::Merging),   // skip
        (RunState::Planning, RunState::Publishing),
        (RunState::Completed, RunState::Planning),
        (RunState::Failed, RunState::Completed),
        (RunState::Paused, RunState::Completed),
        (RunState::Publishing, RunState::Planning),
    ];
    for (from, to) in illegal {
        assert!(
            !is_valid_transition(from, to),
            "{:?} → {:?} should be an invalid transition",
            from,
            to
        );
    }
}

#[test]
fn completed_to_executing_is_rejected() {
    assert!(!is_valid_transition(
        RunState::Completed,
        RunState::Executing
    ));
}

#[test]
fn valid_recovery_paths() {
    assert!(is_valid_transition(RunState::Failed, RunState::Executing));
    assert!(is_valid_transition(RunState::Paused, RunState::Executing));
}

#[test]
fn valid_abort_path() {
    assert!(is_valid_transition(RunState::Executing, RunState::Paused));
}
