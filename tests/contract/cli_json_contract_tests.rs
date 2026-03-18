/// Contract tests: every CLI command's `--format json` output must contain
/// the expected top-level keys.  Shapes are pinned against the snapshot files
/// in `tests/contract/snapshots/`.
use assert_cmd::Command;
use grove_core::orchestrator;
use tempfile::TempDir;

/// Assert that `value[key]` exists and has the expected JSON type name.
/// Valid `expected` values: "string", "number", "bool", "array", "object", "null".
fn assert_type(value: &serde_json::Value, key: &str, expected: &str) {
    let v = &value[key];
    let actual = match v {
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Null => "null",
    };
    assert_eq!(
        actual, expected,
        "field '{key}' expected type {expected}, got {actual} (full value: {value})"
    );
}

fn grove_bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_grove"))
}

fn grove(dir: &TempDir) -> Command {
    let mut cmd = Command::new(grove_bin());
    cmd.args([
        "--project",
        dir.path().to_str().unwrap(),
        "--format",
        "json",
    ]);
    // Use mock provider so tests don't need Claude CLI.
    cmd.env("GROVE_PROVIDER", "mock");
    cmd
}

fn initialized_dir() -> TempDir {
    let dir = TempDir::new().unwrap();
    Command::new(grove_bin())
        .args(["--project", dir.path().to_str().unwrap(), "init"])
        .assert()
        .success();
    dir
}

fn run_mock(dir: &TempDir) -> String {
    let output = grove(dir).args(["run", "contract test"]).output().unwrap();
    assert!(
        output.status.success(),
        "grove run failed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    json["run_id"].as_str().unwrap().to_string()
}

// ── init ─────────────────────────────────────────────────────────────────────

#[test]
fn init_json_has_required_fields() {
    let dir = TempDir::new().unwrap();
    let output = grove(&dir).arg("init").output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["project_root"].is_string(), "missing project_root");
    assert!(json["grove_dir"].is_string(), "missing grove_dir");
    assert!(json["db_path"].is_string(), "missing db_path");
    assert!(json["schema_version"].is_number(), "missing schema_version");
    assert!(json["created"].is_boolean(), "missing created");

    // Type assertions
    assert_type(&json, "project_root", "string");
    assert_type(&json, "grove_dir", "string");
    assert_type(&json, "db_path", "string");
    assert_type(&json, "schema_version", "number");
    assert_type(&json, "created", "bool");
}

// ── doctor ────────────────────────────────────────────────────────────────────

#[test]
fn doctor_json_has_required_fields() {
    let dir = initialized_dir();
    let output = grove(&dir).arg("doctor").output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["ok"].is_boolean(), "missing ok");
    assert!(json["checks"].is_array(), "missing checks");
    assert!(json["fixes_applied"].is_array(), "missing fixes_applied");

    // Type assertions
    assert_type(&json, "ok", "bool");
    assert_type(&json, "checks", "array");
    assert_type(&json, "fixes_applied", "array");
}

// ── run ───────────────────────────────────────────────────────────────────────

#[test]
fn run_json_has_required_fields() {
    let dir = initialized_dir();
    let output = grove(&dir)
        .args(["run", "test objective"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "grove run failed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["run_id"].is_string(), "missing run_id");
    assert!(json["state"].is_string(), "missing state");
    assert!(json["objective"].is_string(), "missing objective");
    assert!(json["plan"].is_array(), "missing plan");

    // Type assertions
    assert_type(&json, "run_id", "string");
    assert_type(&json, "state", "string");
    assert_type(&json, "objective", "string");
    assert_type(&json, "plan", "array");
}

// ── status ────────────────────────────────────────────────────────────────────

#[test]
fn status_json_has_runs_array() {
    let dir = initialized_dir();
    run_mock(&dir);

    let output = grove(&dir).arg("status").output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["runs"].is_array(), "missing runs array");

    if let Some(runs) = json["runs"].as_array() {
        if !runs.is_empty() {
            let first = &runs[0];
            assert!(first["id"].is_string(), "run entry missing id");
            assert!(first["state"].is_string(), "run entry missing state");
        }
    }

    // Type assertions
    assert_type(&json, "runs", "array");
    let runs = json["runs"].as_array().unwrap();
    if !runs.is_empty() {
        let first = &runs[0];
        assert_type(first, "id", "string");
        assert_type(first, "state", "string");
    }
}

// ── logs ──────────────────────────────────────────────────────────────────────

#[test]
fn logs_json_has_events_array() {
    let dir = initialized_dir();
    let run_id = run_mock(&dir);

    let output = grove(&dir).args(["logs", &run_id]).output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["events"].is_array(), "missing events array");

    // Type assertions
    assert_type(&json, "run_id", "string");
    assert_type(&json, "events", "array");
}

// ── report ────────────────────────────────────────────────────────────────────

#[test]
fn report_json_has_structured_fields() {
    let dir = initialized_dir();
    let run_id = run_mock(&dir);

    let output = grove(&dir).args(["report", &run_id]).output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["run_id"].is_string(), "missing run_id field: {json}");
    assert!(
        json["objective"].is_string(),
        "missing objective field: {json}"
    );
    assert!(json["state"].is_string(), "missing state field: {json}");
    assert!(
        json["sessions"].is_array(),
        "missing sessions array: {json}"
    );
    assert!(json["events"].is_array(), "missing events array: {json}");

    // Type assertions
    assert_type(&json, "run_id", "string");
    assert_type(&json, "objective", "string");
    assert_type(&json, "state", "string");
    assert_type(&json, "sessions", "array");
    assert_type(&json, "events", "array");
}

// ── task-cancel ───────────────────────────────────────────────────────────────

#[test]
fn task_cancel_nonexistent_task_returns_nonzero() {
    let dir = initialized_dir();
    Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "task-cancel",
            "task_does_not_exist",
        ])
        .assert()
        .failure();
}

#[test]
fn task_cancel_queued_task_returns_cancelled_state() {
    let dir = initialized_dir();
    // Insert a queued task directly via grove-core (avoids drain-queue side-effect of `grove queue`).
    let task = orchestrator::queue_task(
        dir.path(),
        "cancel me",
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    )
    .unwrap();

    let output = grove(&dir)
        .args(["task-cancel", &task.id])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "task-cancel should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["task_id"].is_string(), "missing task_id");
    assert_eq!(json["state"], "cancelled", "state must be 'cancelled'");

    // Type assertions
    assert_type(&json, "task_id", "string");
    assert_type(&json, "state", "string");
}

#[test]
fn task_cancel_already_cancelled_task_returns_nonzero() {
    let dir = initialized_dir();
    let task = orchestrator::queue_task(
        dir.path(),
        "cancel twice",
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    )
    .unwrap();
    // Cancel once successfully.
    grove(&dir)
        .args(["task-cancel", &task.id])
        .assert()
        .success();
    // Second cancel must fail — task is no longer 'queued'.
    Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "task-cancel",
            &task.id,
        ])
        .assert()
        .failure();
}

// ── sessions ──────────────────────────────────────────────────────────────────

#[test]
fn sessions_invalid_run_id_returns_nonzero() {
    let dir = initialized_dir();
    Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "sessions",
            "run_does_not_exist",
        ])
        .assert()
        .failure();
}

#[test]
fn sessions_json_has_required_fields() {
    let dir = initialized_dir();
    let run_id = run_mock(&dir);

    let output = grove(&dir).args(["sessions", &run_id]).output().unwrap();
    assert!(
        output.status.success(),
        "sessions should succeed for valid run; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["run_id"].is_string(), "missing run_id");
    assert!(json["sessions"].is_array(), "missing sessions array");

    // Type assertions
    assert_type(&json, "run_id", "string");
    assert_type(&json, "sessions", "array");
}

// ── ownership ─────────────────────────────────────────────────────────────────

#[test]
fn ownership_no_args_json_has_required_fields() {
    let dir = initialized_dir();

    let output = grove(&dir).arg("ownership").output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["locks"].is_array(), "missing locks array");
    assert!(json["total"].is_number(), "missing total");
    // No sessions are running so there are no locks.
    assert_eq!(json["total"], 0);

    // Type assertions
    assert_type(&json, "locks", "array");
    assert_type(&json, "total", "number");
}

#[test]
fn ownership_with_run_id_filter_json_has_required_fields() {
    let dir = initialized_dir();
    let run_id = run_mock(&dir);

    let output = grove(&dir).args(["ownership", &run_id]).output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["locks"].is_array(), "missing locks array");
    assert!(json["total"].is_number(), "missing total");

    // Type assertions
    assert_type(&json, "locks", "array");
    assert_type(&json, "total", "number");
}

// ── merge-status ──────────────────────────────────────────────────────────────

#[test]
fn merge_status_unknown_conversation_returns_empty() {
    let dir = initialized_dir();
    let output = grove(&dir)
        .args(["merge-status", "conv_does_not_exist"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "merge-status should succeed (empty list) for unknown conversation; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn merge_status_json_has_required_fields() {
    let dir = initialized_dir();
    let run_id = run_mock(&dir);

    // Get the conversation_id from the run
    let conn = grove_core::db::DbHandle::new(dir.path()).connect().unwrap();
    let conv_id: String = conn
        .query_row(
            "SELECT conversation_id FROM runs WHERE id=?1",
            [&run_id],
            |r| r.get(0),
        )
        .unwrap();
    drop(conn);

    let output = grove(&dir)
        .args(["merge-status", &conv_id])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "merge-status should succeed for valid conversation; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        json["conversation_id"].is_string(),
        "missing conversation_id"
    );
    assert!(json["entries"].is_array(), "missing entries array");

    // Type assertions
    assert_type(&json, "conversation_id", "string");
    assert_type(&json, "entries", "array");
}

// ── doctor --fix-all ──────────────────────────────────────────────────────────

#[test]
fn doctor_fix_all_includes_config_and_db_in_checks() {
    let dir = initialized_dir();

    let output = grove(&dir).args(["doctor", "--fix-all"]).output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["ok"].is_boolean(), "missing ok");
    assert!(json["checks"].is_array(), "missing checks");
    assert!(json["fixes_applied"].is_array(), "missing fixes_applied");

    // Type assertions
    assert_type(&json, "ok", "bool");
    assert_type(&json, "checks", "array");
    assert_type(&json, "fixes_applied", "array");

    let check_names: Vec<&str> = json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(
        check_names.contains(&"config"),
        "config check must be present with --fix-all; got: {check_names:?}"
    );
    assert!(
        check_names.contains(&"db_exists"),
        "db_exists check must be present with --fix-all; got: {check_names:?}"
    );
    assert!(
        check_names.contains(&"secret_scan"),
        "secret_scan check must be present with --fix-all; got: {check_names:?}"
    );
}

#[test]
fn doctor_fix_all_reports_ok_true_on_initialized_project() {
    let dir = initialized_dir();

    let output = grove(&dir).args(["doctor", "--fix-all"]).output().unwrap();
    assert!(output.status.success());

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["ok"], true,
        "ok must be true on an initialized project"
    );
}

// ── logs --all ────────────────────────────────────────────────────────────────

#[test]
fn logs_all_invalid_run_id_returns_nonzero() {
    let dir = initialized_dir();
    Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "logs",
            "run_does_not_exist",
            "--all",
        ])
        .assert()
        .failure();
}

#[test]
fn logs_all_flag_returns_non_empty_events_array() {
    let dir = initialized_dir();
    let run_id = run_mock(&dir);

    let output = grove(&dir)
        .args(["logs", &run_id, "--all"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "logs --all should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["run_id"].is_string(), "missing run_id");
    assert!(json["events"].is_array(), "missing events array");
    // A mock run emits at least run_created, plan_generated, run_completed.
    let events = json["events"].as_array().unwrap();
    assert!(
        events.len() >= 3,
        "expected at least 3 events, got {}",
        events.len()
    );

    // Type assertions
    assert_type(&json, "run_id", "string");
    assert_type(&json, "events", "array");
}

#[test]
fn logs_all_and_logs_return_same_events_for_small_run() {
    let dir = initialized_dir();
    let run_id = run_mock(&dir);

    let with_all = {
        let out = grove(&dir)
            .args(["logs", &run_id, "--all"])
            .output()
            .unwrap();
        let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        j["events"].as_array().unwrap().len()
    };
    let without_all = {
        let out = grove(&dir).args(["logs", &run_id]).output().unwrap();
        let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        j["events"].as_array().unwrap().len()
    };
    assert_eq!(
        with_all, without_all,
        "for a small run both paths must return the same count"
    );
}

// ── worktrees --delete-all ────────────────────────────────────────────────────

#[test]
fn worktrees_delete_all_yes_flag_skips_prompt_and_succeeds() {
    let dir = initialized_dir();

    let output = grove(&dir)
        .args(["worktrees", "--delete-all", "--yes"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "worktrees --delete-all --yes should succeed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["deleted"].is_number(), "missing deleted count");
    assert!(json["freed_bytes"].is_number(), "missing freed_bytes");
    // No worktrees exist on a fresh project.
    assert_eq!(json["deleted"], 0);
    assert_eq!(json["freed_bytes"], 0);

    // Type assertions
    assert_type(&json, "deleted", "number");
    assert_type(&json, "freed_bytes", "number");
}

// ── error-path contract tests ─────────────────────────────────────────────────

/// When a command fails, the error envelope is written to stderr as JSON.
/// Verify that stderr is valid JSON and contains the expected envelope shape.
#[test]
fn error_envelope_has_required_fields_on_nonexistent_run_id() {
    let dir = initialized_dir();
    // `sessions` on a nonexistent run_id returns a GroveError::NotFound which
    // is classified and emitted to stderr as JSON with a non-zero exit code.
    let output = Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            "sessions",
            "run_does_not_exist",
        ])
        .env("GROVE_PROVIDER", "mock")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for unknown run_id"
    );

    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let json: serde_json::Value = serde_json::from_str(stderr_str.trim())
        .unwrap_or_else(|e| panic!("stderr is not valid JSON: {e}\nstderr was: {stderr_str}"));

    // Top-level "error" key must be an object
    assert_type(&json, "error", "object");

    let error_obj = &json["error"];

    // Required envelope fields
    assert_type(error_obj, "code", "string");
    assert_type(error_obj, "message", "string");
    assert_type(error_obj, "hint", "string");
    assert_type(error_obj, "details", "object");

    // code and message must be non-empty
    assert!(
        !error_obj["code"].as_str().unwrap().is_empty(),
        "error.code must not be empty"
    );
    assert!(
        !error_obj["message"].as_str().unwrap().is_empty(),
        "error.message must not be empty"
    );
}

#[test]
fn error_envelope_has_required_fields_on_nonexistent_task_cancel() {
    let dir = initialized_dir();
    let output = Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            "task-cancel",
            "task_does_not_exist",
        ])
        .env("GROVE_PROVIDER", "mock")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for unknown task_id"
    );

    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let json: serde_json::Value = serde_json::from_str(stderr_str.trim())
        .unwrap_or_else(|e| panic!("stderr is not valid JSON: {e}\nstderr was: {stderr_str}"));

    assert_type(&json, "error", "object");

    let error_obj = &json["error"];
    assert_type(error_obj, "code", "string");
    assert_type(error_obj, "message", "string");
    assert_type(error_obj, "hint", "string");
    assert_type(error_obj, "details", "object");

    assert!(
        !error_obj["code"].as_str().unwrap().is_empty(),
        "error.code must not be empty"
    );
    assert!(
        !error_obj["message"].as_str().unwrap().is_empty(),
        "error.message must not be empty"
    );
}

#[test]
fn error_envelope_has_required_fields_on_invalid_logs_run_id() {
    let dir = initialized_dir();
    // `logs --all` on a nonexistent run_id returns a database error (no such
    // run exists) and exits with a non-zero code, emitting an error envelope
    // on stderr.  Without --all the command returns an empty events array
    // with exit 0, so --all is required to trigger the error path.
    let output = Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            "logs",
            "run_does_not_exist",
            "--all",
        ])
        .env("GROVE_PROVIDER", "mock")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "expected non-zero exit for unknown run_id in logs --all"
    );

    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let json: serde_json::Value = serde_json::from_str(stderr_str.trim())
        .unwrap_or_else(|e| panic!("stderr is not valid JSON: {e}\nstderr was: {stderr_str}"));

    assert_type(&json, "error", "object");

    let error_obj = &json["error"];
    assert_type(error_obj, "code", "string");
    assert_type(error_obj, "message", "string");
    assert_type(error_obj, "hint", "string");
    assert_type(error_obj, "details", "object");
}

#[test]
fn error_envelope_second_task_cancel_has_invariant_violation_code() {
    // Re-cancelling an already-cancelled task produces an InvalidTransition
    // error which must be classified as INVARIANT_VIOLATION.
    let dir = initialized_dir();
    let task = orchestrator::queue_task(
        dir.path(),
        "cancel twice for error test",
        None,
        0,
        None,
        None,
        None,
        None,
        None,
        None,
        false,
    )
    .unwrap();

    // First cancel succeeds
    grove(&dir)
        .args(["task-cancel", &task.id])
        .assert()
        .success();

    // Second cancel: non-zero exit + structured error on stderr
    let output = Command::new(grove_bin())
        .args([
            "--project",
            dir.path().to_str().unwrap(),
            "--format",
            "json",
            "task-cancel",
            &task.id,
        ])
        .env("GROVE_PROVIDER", "mock")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "second task-cancel must fail with non-zero exit"
    );

    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let json: serde_json::Value = serde_json::from_str(stderr_str.trim())
        .unwrap_or_else(|e| panic!("stderr is not valid JSON: {e}\nstderr was: {stderr_str}"));

    assert_type(&json, "error", "object");

    let error_obj = &json["error"];
    assert_type(error_obj, "code", "string");
    assert_type(error_obj, "message", "string");
    assert_type(error_obj, "hint", "string");
    assert_type(error_obj, "details", "object");

    // The code must be non-empty; we do not assert the exact value to avoid
    // brittleness, but it should not be blank.
    assert!(
        !error_obj["code"].as_str().unwrap().is_empty(),
        "error.code must not be empty on second cancel"
    );
}
