//! End-to-end lifecycle test for `grove daemon`.
//!
//! Marked `#[ignore]` because it requires `grove-daemon` to be built into
//! `target/debug/` alongside `grove`. CI runs it via:
//!   cargo build -p grove-daemon && cargo test -p grove-cli --test daemon_lifecycle -- --ignored

use assert_cmd::Command;
use predicates::prelude::*;
use std::time::{Duration, Instant};

fn grove() -> Command {
    #[allow(deprecated)]
    {
        Command::cargo_bin("grove").expect("grove binary")
    }
}

#[test]
#[ignore = "requires built grove-daemon binary in target/"]
fn start_status_stop_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();

    grove()
        .current_dir(root)
        .args(["daemon", "start", "--detach"])
        .assert()
        .success();

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let out = grove()
            .current_dir(root)
            .args(["daemon", "status"])
            .output()
            .unwrap();
        if out.status.success()
            && String::from_utf8_lossy(&out.stdout).contains("status: ok")
        {
            break;
        }
        if Instant::now() > deadline {
            panic!(
                "daemon never reported healthy: stdout={:?}",
                String::from_utf8_lossy(&out.stdout)
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    grove()
        .current_dir(root)
        .args(["daemon", "stop"])
        .assert()
        .success()
        .stdout(predicate::str::contains("stopped"));
}
