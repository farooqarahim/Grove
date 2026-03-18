use assert_cmd::Command;
use tempfile::TempDir;

fn grove(dir: &TempDir) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_grove"));
    cmd.args(["--project", dir.path().to_str().unwrap()]);
    // Use mock provider so tests don't need Claude CLI.
    cmd.env("GROVE_PROVIDER", "mock");
    cmd
}

#[test]
fn init_creates_grove_dir_and_db() {
    let dir = TempDir::new().unwrap();

    grove(&dir).arg("init").assert().success();

    let grove_dir = dir.path().join(".grove");
    assert!(grove_dir.exists(), ".grove/ directory should be created");
    assert!(grove_dir.join("grove.db").exists(), "grove.db should exist");
    assert!(grove_dir.join("reports").exists(), "reports/ should exist");
}

#[test]
fn init_is_idempotent() {
    let dir = TempDir::new().unwrap();

    grove(&dir).arg("init").assert().success();
    grove(&dir).arg("init").assert().success(); // second call must not fail
}

#[test]
fn run_completes_and_returns_run_id() {
    let dir = TempDir::new().unwrap();
    grove(&dir).arg("init").assert().success();

    let output = grove(&dir)
        .args(["--format", "json", "run", "build a demo feature"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "grove run should exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["run_id"].is_string(), "run_id must be present");
    assert_eq!(json["state"], "completed");
}

#[test]
fn report_generates_json_file() {
    let dir = TempDir::new().unwrap();
    grove(&dir).arg("init").assert().success();

    // Run to create a run.
    let output = grove(&dir)
        .args(["--format", "json", "run", "demo"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "grove run failed; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let run_id = json["run_id"].as_str().unwrap();

    // Generate the report.
    grove(&dir).args(["report", run_id]).assert().success();

    // Verify the JSON file was written.
    let report_path = dir
        .path()
        .join(".grove")
        .join("reports")
        .join(format!("{run_id}.json"));
    assert!(
        report_path.exists(),
        "report file should exist at {}",
        report_path.display()
    );

    let content = std::fs::read_to_string(&report_path).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("report file should be valid JSON");
    assert_eq!(
        parsed["run_id"].as_str(),
        Some(run_id),
        "report should contain the run_id"
    );
}
