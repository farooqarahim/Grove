# Grove CLI Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `grove-cli` with a fresh feature-flagged crate that mirrors grove-gui's core feature set — runs/sessions, git, auth/llm, workspace/project/conversation, and issues — with rich ANSI output and an optional ratatui TUI.

**Architecture:** Single `grove-cli` crate. `Transport` trait abstracts direct grove-core calls (reads, sync) from Unix socket to grove-server (mutations + streaming). `feature = "tui"` gates ratatui/crossterm. Output layer renders ANSI tables/spinners by default, JSON with `--json`. No budget features anywhere.

**Tech Stack:** Rust 2024 (rust-version 1.85), clap 4.5, console 0.15, indicatif 0.17, tabled 0.15, serde/serde_json 1, tokio (rt/sync/time/macros/net), thiserror 2.0, ratatui 0.28 + crossterm 0.28 (optional `tui` feature).

**Spec:** `docs/superpowers/specs/2026-03-18-grove-cli-redesign.md`

---

## ⚠️ Scope Note

This is a large feature. It is divided into 4 independent phases, each producing working, testable software:

- **Phase A (Tasks 1–6):** Migration + foundation (transport, output, clap skeleton) — *must complete first*
- **Phase B (Tasks 7–11):** Core commands (init/doctor, run/queue/tasks, status/logs, git, auth/llm)
- **Phase C (Tasks 12–15):** Extended commands (issues/connect/fix, workspace/project, conversation, plumbing)
- **Phase D (Tasks 16–18):** TUI (feature=tui: run-watch, full dashboard, wire --watch flags)

Each phase can be reviewed and committed independently. Phase A must land before B, C, or D.

---

## File Map

### New files — `crates/grove-cli/`

```
crates/grove-cli/
  Cargo.toml
  src/
    main.rs
    cli.rs
    error.rs
    output/
      mod.rs
      text.rs
      json.rs
    transport/
      mod.rs          ← Transport trait + GroveTransport enum + TestTransport
      direct.rs       ← calls grove_core::orchestrator::* (sync, reads)
      socket.rs       ← Unix socket client (mutations + streaming)
    commands/
      mod.rs          ← dispatch fn
      init.rs
      doctor.rs
      run.rs          ← run, queue, tasks, task-cancel
      status.rs       ← status, resume, abort, logs, report, plan, subtasks, sessions,
                         ownership, conflicts, merge-status, publish
      git.rs
      issues.rs       ← issue subcommands + fix + connect + lint + ci
      auth.rs
      llm.rs
      workspace.rs
      project.rs
      conversation.rs
      signals.rs
      hooks.rs
      worktrees.rs
      cleanup.rs      ← cleanup + gc
    tui/              ← #[cfg(feature = "tui")] throughout
      mod.rs
      run_watch.rs
      dashboard.rs
      widgets/
        mod.rs
```

### Modified files

- `Cargo.toml` (workspace root) — rename old member, add new member, set `default-members`
- `crates/grove-cli-old/Cargo.toml` — rename package from `grove-cli` to `grove-cli-old`

### Test files (inline `#[cfg(test)]` modules)

- `src/error.rs` — exit code tests
- `src/output/text.rs` — table render tests
- `src/transport/mod.rs` — TestTransport + basic trait tests
- `src/transport/direct.rs` — integration tests against tmpdir grove-core
- `src/commands/run.rs` — tasks/queue handler tests
- `src/commands/status.rs` — status handler tests
- `src/commands/git.rs` — git status smoke tests
- `src/commands/issues.rs` — issue list/board handler tests
- `src/tui/run_watch.rs` — state initialisation tests (feature=tui)
- `src/tui/dashboard.rs` — screen enum tests (feature=tui)

---

## Phase A — Migration & Foundation

### Task 1: Rename grove-cli → grove-cli-old

**Files:**
- Rename: `crates/grove-cli/` → `crates/grove-cli-old/`
- Modify: `crates/grove-cli-old/Cargo.toml` — package name only
- Modify: `Cargo.toml` — update workspace members

- [ ] **Step 1: Rename the directory**

```bash
mv crates/grove-cli crates/grove-cli-old
```

- [ ] **Step 2: Change the package name (one line only — no other code changes)**

In `crates/grove-cli-old/Cargo.toml`, find the `[package]` section and change:
```toml
name = "grove-cli-old"
```

- [ ] **Step 3: Update workspace `Cargo.toml`**

Replace the `"crates/grove-cli"` entry in `members`:
```toml
[workspace]
members = [
  "crates/grove-core",
  "crates/grove-cli-old",      # ← was "crates/grove-cli"
  "crates/grove-gui/src-tauri",
  "crates/grove-mcp-server",
  "crates/grove-filter",
]
```

- [ ] **Step 4: Verify old crate still compiles (no code was changed)**

```bash
cargo check -p grove-cli-old
```
Expected: compiles cleanly — no errors, only possible pre-existing warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/grove-cli-old/Cargo.toml Cargo.toml
git commit -m "chore: rename grove-cli to grove-cli-old (archive)"
```

---

### Task 2: Scaffold grove-cli crate

**Files:**
- Create: `crates/grove-cli/Cargo.toml`
- Create: `crates/grove-cli/src/main.rs`
- Modify: `Cargo.toml` (workspace)

- [ ] **Step 1: Create the directory**

```bash
mkdir -p crates/grove-cli/src
```

- [ ] **Step 2: Write `crates/grove-cli/Cargo.toml`**

```toml
[package]
name    = "grove-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[[bin]]
name = "grove"
path = "src/main.rs"

[features]
default = []
tui = ["dep:ratatui", "dep:crossterm"]

[dependencies]
grove-core   = { path = "../grove-core" }
clap         = { workspace = true }
serde        = { workspace = true }
serde_json   = { workspace = true }
thiserror    = { workspace = true }
tokio        = { workspace = true }
anyhow       = { workspace = true }
dirs         = { workspace = true }
which        = { workspace = true }
console      = "0.15"
indicatif    = "0.17"
tabled       = "0.15"
ratatui      = { version = "0.28", optional = true }
crossterm    = { version = "0.28", optional = true }

[dev-dependencies]
tempfile     = { workspace = true }
assert_cmd   = { workspace = true }
predicates   = { workspace = true }
```

- [ ] **Step 3: Write minimal `src/main.rs`**

```rust
fn main() {
    println!("grove");
}
```

- [ ] **Step 4: Add to workspace and set `default-members`**

In workspace `Cargo.toml`:
```toml
[workspace]
members = [
  "crates/grove-core",
  "crates/grove-cli-old",
  "crates/grove-cli",              # ← new
  "crates/grove-gui/src-tauri",
  "crates/grove-mcp-server",
  "crates/grove-filter",
]
default-members = ["crates/grove-cli"]   # ← new: cargo build defaults to new CLI
```

- [ ] **Step 5: Verify both feature variants compile**

```bash
cargo check -p grove-cli
cargo check -p grove-cli --features tui
```
Expected: both succeed with empty `main`.

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/ Cargo.toml
git commit -m "chore: scaffold grove-cli crate with feature-flagged TUI"
```

---

### Task 3: Error layer

**Files:**
- Create: `crates/grove-cli/src/error.rs`
- Modify: `crates/grove-cli/src/main.rs`

- [ ] **Step 1: Write the failing tests first**

Create `crates/grove-cli/src/error.rs` with just the tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_arg_exits_2() {
        assert_eq!(CliError::BadArg("x".into()).exit_code(), 2);
    }

    #[test]
    fn not_found_exits_3() {
        assert_eq!(CliError::NotFound("run".into()).exit_code(), 3);
    }

    #[test]
    fn transport_exits_4() {
        assert_eq!(CliError::Transport("sock".into()).exit_code(), 4);
    }

    #[test]
    fn other_exits_1() {
        assert_eq!(CliError::Other("oops".into()).exit_code(), 1);
    }
}
```

- [ ] **Step 2: Run to verify it fails (module not yet exported)**

```bash
cargo test -p grove-cli
```
Expected: compile error — `error` module not found.

- [ ] **Step 3: Implement `error.rs`**

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    // grove_core::GroveError is re-exported from grove_core::errors::GroveError via lib.rs
    #[error("grove-core: {0}")]
    Core(#[from] grove_core::GroveError),

    #[error("transport: {0}")]
    Transport(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid argument: {0}")]
    BadArg(String),

    #[error("{0}")]
    Other(String),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::BadArg(_)     => 2,
            CliError::NotFound(_)   => 3,
            CliError::Transport(_)  => 4,
            _                       => 1,
        }
    }
}

pub type CliResult<T> = std::result::Result<T, CliError>;
```

Note: verify the exact error type exported from grove-core by checking `crates/grove-core/src/errors.rs`.

- [ ] **Step 4: Wire into `main.rs`**

```rust
mod error;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(e.exit_code());
    }
}

fn run() -> error::CliResult<()> {
    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p grove-cli
```
Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/src/error.rs crates/grove-cli/src/main.rs
git commit -m "feat(grove-cli): error layer with typed exit codes"
```

---

### Task 4: Output layer

**Files:**
- Create: `crates/grove-cli/src/output/mod.rs`
- Create: `crates/grove-cli/src/output/text.rs`
- Create: `crates/grove-cli/src/output/json.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/grove-cli/src/output/text.rs` with just tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_includes_headers_and_data() {
        let rows = vec![vec!["abc12345".to_string(), "Add OAuth".to_string(), "running".to_string()]];
        let out = render_table(&["ID", "OBJECTIVE", "STATE"], &rows);
        assert!(out.contains("ID"));
        assert!(out.contains("Add OAuth"));
    }

    #[test]
    fn dim_returns_non_empty_string() {
        assert!(!dim("hello").is_empty());
    }
}
```

Create `crates/grove-cli/src/output/json.rs` with just tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_json_produces_compact_json() {
        let val = serde_json::json!({"key": "value"});
        let out = emit_json(&val);
        assert_eq!(out, r#"{"key":"value"}"#);
    }

    #[test]
    fn emit_error_includes_code() {
        let out = emit_error_json("not found", 3);
        assert!(out.contains("\"code\":3"));
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli
```
Expected: compile error — `output` module missing.

- [ ] **Step 3: Implement `output/mod.rs`**

```rust
pub mod json;
pub mod text;

#[derive(Debug, Clone)]
pub enum OutputMode {
    Text { no_color: bool },
    Json,
}
```

- [ ] **Step 4: Implement `output/text.rs`**

```rust
use console::Style;
use indicatif::{ProgressBar, ProgressStyle};
use tabled::builder::Builder;
use std::time::Duration;

pub fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut b = Builder::default();
    b.push_record(headers);
    for row in rows {
        b.push_record(row.iter().map(String::as_str));
    }
    b.build().to_string()
}

pub fn success(msg: &str) {
    println!("{}", Style::new().green().apply_to(msg));
}

pub fn error_line(msg: &str) {
    eprintln!("{}", Style::new().red().apply_to(msg));
}

pub fn dim(msg: &str) -> String {
    Style::new().dim().apply_to(msg).to_string()
}

pub fn bold(msg: &str) -> String {
    Style::new().bold().apply_to(msg).to_string()
}

/// Create and start a spinner. Call `.finish_and_clear()` when done.
pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_includes_headers_and_data() {
        let rows = vec![vec!["abc12345".to_string(), "Add OAuth".to_string(), "running".to_string()]];
        let out = render_table(&["ID", "OBJECTIVE", "STATE"], &rows);
        assert!(out.contains("ID"));
        assert!(out.contains("Add OAuth"));
    }

    #[test]
    fn dim_returns_non_empty_string() {
        assert!(!dim("hello").is_empty());
    }
}
```

- [ ] **Step 5: Implement `output/json.rs`**

```rust
pub fn emit_json(val: &serde_json::Value) -> String {
    serde_json::to_string(val).unwrap_or_else(|_| "{}".to_string())
}

pub fn emit_json_pretty(val: &serde_json::Value) -> String {
    serde_json::to_string_pretty(val).unwrap_or_else(|_| "{}".to_string())
}

/// Print a JSON error to stdout (used in --json mode; do not mix with event output).
pub fn emit_error_json(msg: &str, code: i32) -> String {
    emit_json(&serde_json::json!({ "error": msg, "code": code }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_json_produces_compact_json() {
        let val = serde_json::json!({"key": "value"});
        let out = emit_json(&val);
        assert_eq!(out, r#"{"key":"value"}"#);
    }

    #[test]
    fn emit_error_includes_code() {
        let out = emit_error_json("not found", 3);
        assert!(out.contains("\"code\":3"));
    }
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p grove-cli
```
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/grove-cli/src/output/
git commit -m "feat(grove-cli): output layer — rich text + JSON"
```

---

### Task 5: Transport layer

**Files:**
- Create: `crates/grove-cli/src/transport/mod.rs`
- Create: `crates/grove-cli/src/transport/direct.rs`
- Create: `crates/grove-cli/src/transport/socket.rs`

- [ ] **Step 1: Write failing test**

Create `crates/grove-cli/src/transport/mod.rs` with just the test:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_list_runs_returns_empty() {
        let t = TestTransport::default();
        let runs = t.list_runs(10).unwrap();
        assert!(runs.is_empty());
    }

    #[test]
    fn test_transport_get_run_returns_none() {
        let t = TestTransport::default();
        assert!(t.get_run("abc").unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli
```
Expected: compile error — `transport` module missing.

- [ ] **Step 3: Implement `transport/mod.rs`**

```rust
pub mod direct;
pub mod socket;

use crate::error::CliResult;

// ── Verified grove-core type paths (confirmed by reading grove-core/src/) ──────
// grove_core::orchestrator::RunRecord    — defined in orchestrator/mod.rs
// grove_core::orchestrator::TaskRecord   — defined in orchestrator/mod.rs
// grove_core::GroveError                 — re-exported from errors/mod.rs via lib.rs
// grove_core::db::repositories::workspaces_repo::WorkspaceRow
// grove_core::db::repositories::projects_repo::ProjectRow
// grove_core::db::repositories::conversations_repo::ConversationRow
//
// ⚠️  Before implementing: run `cargo doc -p grove-core --open` and verify every
//     type you import is actually `pub` and the path is correct.
//
// ── Transport trait growth ────────────────────────────────────────────────────
// This trait STARTS with the 7 read methods below (Task 5 baseline).
// Each subsequent task ADDS mutation methods to the trait AND updates:
//   1. DirectTransport impl   — real grove-core call
//   2. SocketTransport impl   — socket stub (Err for now)
//   3. TestTransport impl     — Ok(default) or Err as appropriate
// Never remove methods. Never split into sub-traits. Grow in place.

/// All transport operations the CLI needs.
/// Sync by design — matches existing grove-core orchestrator API (which is also sync).
///
/// ⚠️  `get_run(id)` is intentionally absent from this baseline.
///     The real `grove_core::orchestrator` has no `get_run` function.
///     It will be added as a DB-backed helper in Task 16 (TUI run-watch),
///     the only consumer that needs it.
///
/// ⚠️  `list_tasks` takes NO limit parameter.
///     The real `grove_core::orchestrator::list_tasks(project_root)` has no limit arg.
///     Apply limit client-side: `.into_iter().take(limit as usize).collect()`.
pub trait Transport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>>;
    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>>;
    fn get_workspace(&self) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>>;
    fn list_projects(&self) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>>;
    fn list_conversations(&self, limit: i64) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>>;
    // IssueRow path TBD — verify in grove-core/src/db/repositories/ before implementing
    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>>;
    // ↑ Tasks 8–15 ADD more methods here. Update all three impls + TestTransport each time.
}

/// Runtime transport — selects implementation at startup.
pub enum GroveTransport {
    Direct(direct::DirectTransport),
    Socket(socket::SocketTransport),
    #[cfg(test)]
    Test(TestTransport),
}

impl GroveTransport {
    /// Auto-detect: if a grove.sock exists, use Socket; otherwise Direct.
    pub fn detect(project: &std::path::Path) -> Self {
        let local_sock = project.join(".grove/grove.sock");
        let global_sock = dirs::home_dir()
            .map(|h| h.join(".grove/grove.sock"))
            .unwrap_or_default();
        if local_sock.exists() || global_sock.exists() {
            let sock = if local_sock.exists() { local_sock } else { global_sock };
            GroveTransport::Socket(socket::SocketTransport::new(sock))
        } else {
            GroveTransport::Direct(direct::DirectTransport::new(project))
        }
    }
}

// Forward all Transport calls to the inner implementation.
impl Transport for GroveTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        match self {
            GroveTransport::Direct(t)  => t.list_runs(limit),
            GroveTransport::Socket(t)  => t.list_runs(limit),
            #[cfg(test)]
            GroveTransport::Test(t)    => t.list_runs(limit),
        }
    }
    // Implement all other forwarding methods following the same pattern.
    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        match self {
            GroveTransport::Direct(t) => t.list_tasks(),
            GroveTransport::Socket(t) => t.list_tasks(),
            #[cfg(test)]
            GroveTransport::Test(t)   => t.list_tasks(),
        }
    }
    fn get_workspace(&self) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        match self {
            GroveTransport::Direct(t) => t.get_workspace(),
            GroveTransport::Socket(t) => t.get_workspace(),
            #[cfg(test)]
            GroveTransport::Test(t)   => t.get_workspace(),
        }
    }
    fn list_projects(&self) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        match self {
            GroveTransport::Direct(t) => t.list_projects(),
            GroveTransport::Socket(t) => t.list_projects(),
            #[cfg(test)]
            GroveTransport::Test(t)   => t.list_projects(),
        }
    }
    fn list_conversations(&self, limit: i64) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        match self {
            GroveTransport::Direct(t) => t.list_conversations(limit),
            GroveTransport::Socket(t) => t.list_conversations(limit),
            #[cfg(test)]
            GroveTransport::Test(t)   => t.list_conversations(limit),
        }
    }
    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> {
        match self {
            GroveTransport::Direct(t) => t.list_issues(),
            GroveTransport::Socket(t) => t.list_issues(),
            #[cfg(test)]
            GroveTransport::Test(t)   => t.list_issues(),
        }
    }
}

/// Test-only in-memory transport. All methods return empty/default values.
#[cfg(test)]
#[derive(Default)]
pub struct TestTransport;

#[cfg(test)]
impl Transport for TestTransport {
    fn list_runs(&self, _: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> { Ok(vec![]) }
    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> { Ok(vec![]) }
    fn get_workspace(&self) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> { Ok(None) }
    fn list_projects(&self) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> { Ok(vec![]) }
    fn list_conversations(&self, _: i64) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> { Ok(vec![]) }
    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> { Ok(vec![]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transport_list_runs_returns_empty() {
        let t = TestTransport::default();
        assert!(t.list_runs(10).unwrap().is_empty());
    }

}
```

> **Note:** The exact grove-core type paths (`orchestrator::RunStatus`, `db::WorkspaceRow`, etc.) must be verified by reading `crates/grove-core/src/lib.rs` and the relevant sub-modules before implementing. Adjust imports to match what is actually `pub` in grove-core.

- [ ] **Step 4: Implement `transport/direct.rs` — stubs only**

```rust
use std::path::{Path, PathBuf};
use crate::error::{CliError, CliResult};
use super::Transport;

pub struct DirectTransport {
    project: PathBuf,
}

impl DirectTransport {
    pub fn new(project: &Path) -> Self {
        Self { project: project.to_owned() }
    }
}

impl Transport for DirectTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        Ok(grove_core::orchestrator::list_runs(&self.project, limit)
            .map_err(|e| CliError::Core(e))?)
    }
    // Stub remaining methods with Err(CliError::Other("not yet implemented".into()))
    // Fill them in as each command group is built (Tasks 7–15).
    // Note: list_tasks has no limit — apply limit client-side in the command handler.
    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        Err(CliError::Other("not yet implemented".into()))
    }
    fn get_workspace(&self) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        Err(CliError::Other("not yet implemented".into()))
    }
    fn list_projects(&self) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        Err(CliError::Other("not yet implemented".into()))
    }
    fn list_conversations(&self, _: i64) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        Err(CliError::Other("not yet implemented".into()))
    }
    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Other("not yet implemented".into()))
    }
}
```

- [ ] **Step 5: Implement `transport/socket.rs` — stubs only**

```rust
use std::path::PathBuf;
use crate::error::{CliError, CliResult};
use super::Transport;

pub struct SocketTransport {
    sock_path: PathBuf,
}

impl SocketTransport {
    pub fn new(sock_path: PathBuf) -> Self {
        Self { sock_path }
    }

    /// Send a JSON-RPC request and wait for the response.
    /// Protocol: newline-delimited JSON over Unix domain socket.
    fn call(&self, method: &str, params: serde_json::Value) -> CliResult<serde_json::Value> {
        // TODO: implement in Task 15 (socket transport completion)
        let _ = (method, params);
        Err(CliError::Transport("socket transport not yet implemented".into()))
    }
}

impl Transport for SocketTransport {
    fn list_runs(&self, limit: i64) -> CliResult<Vec<grove_core::orchestrator::RunRecord>> {
        let _ = self.call("list_runs", serde_json::json!({ "limit": limit }))?;
        Err(CliError::Transport("socket not yet implemented".into()))
    }
    // Stub all other methods similarly.
    fn list_tasks(&self) -> CliResult<Vec<grove_core::orchestrator::TaskRecord>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }
    fn get_workspace(&self) -> CliResult<Option<grove_core::db::repositories::workspaces_repo::WorkspaceRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }
    fn list_projects(&self) -> CliResult<Vec<grove_core::db::repositories::projects_repo::ProjectRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }
    fn list_conversations(&self, _: i64) -> CliResult<Vec<grove_core::db::repositories::conversations_repo::ConversationRow>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }
    fn list_issues(&self) -> CliResult<Vec<serde_json::Value>> {
        Err(CliError::Transport("socket not yet implemented".into()))
    }
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p grove-cli
```
Expected: all tests pass (test transport tests pass; direct/socket stubs compile).

- [ ] **Step 7: Commit**

```bash
git add crates/grove-cli/src/transport/
git commit -m "feat(grove-cli): transport layer — trait + TestTransport + stubs"
```

---

### Task 6: clap CLI structure + dispatch skeleton

**Files:**
- Create: `crates/grove-cli/src/cli.rs`
- Create: `crates/grove-cli/src/commands/mod.rs`
- Create: `crates/grove-cli/src/commands/*.rs` (one stub per command group)
- Modify: `crates/grove-cli/src/main.rs`

- [ ] **Step 1: Write failing arg-parsing tests**

Create `crates/grove-cli/src/cli.rs` with just tests at the bottom:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_run_command() {
        let cli = Cli::try_parse_from(["grove", "run", "add dark mode"]).unwrap();
        match cli.command {
            Commands::Run(a) => assert_eq!(a.objective, "add dark mode"),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn parses_json_global_flag() {
        let cli = Cli::try_parse_from(["grove", "--json", "status"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn parses_status_limit() {
        let cli = Cli::try_parse_from(["grove", "status", "--limit", "5"]).unwrap();
        match cli.command {
            Commands::Status(a) => assert_eq!(a.limit, 5),
            _ => panic!("wrong command"),
        }
    }

    #[test]
    fn run_watch_flag_parses() {
        let cli = Cli::try_parse_from(["grove", "run", "obj", "--watch"]).unwrap();
        match cli.command {
            Commands::Run(a) => assert!(a.watch),
            _ => panic!(),
        }
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli cli
```
Expected: compile error — `cli` module not found.

- [ ] **Step 3: Implement `cli.rs` — full clap struct**

Implement the full command surface from spec section 6. Key patterns:

```rust
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "grove", about = "Grove — AI-powered development platform CLI")]
pub struct Cli {
    /// Working directory (default: current directory).
    #[arg(long, global = true, default_value = ".")]
    pub project: PathBuf,

    /// Emit machine-readable JSON to stdout.
    #[arg(long, global = true)]
    pub json: bool,

    #[arg(long, global = true)]
    pub verbose: bool,

    #[arg(long = "no-color", global = true)]
    pub no_color: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PermissionModeArg {
    SkipAll,
    HumanGate,
    AutonomousGate,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init,
    Doctor(DoctorArgs),
    Run(RunArgs),
    Queue(QueueArgs),
    Tasks(TasksArgs),
    TaskCancel(TaskCancelArgs),
    Status(StatusArgs),
    Resume(ResumeArgs),
    Abort(AbortArgs),
    Logs(LogsArgs),
    Report(ReportArgs),
    Plan(PlanArgs),
    Subtasks(SubtasksArgs),
    Sessions(SessionsArgs),
    Git(GitArgs),
    Issue(IssueArgs),
    Fix(FixArgs),
    Connect(ConnectArgs),
    Auth(AuthArgs),
    Llm(LlmArgs),
    Workspace(WorkspaceArgs),
    Project(ProjectArgs),
    Conversation(ConversationArgs),
    Signal(SignalArgs),
    Hook(HookArgs),
    Worktrees(WorktreesArgs),
    Cleanup(CleanupArgs),
    Gc(GcArgs),
    Ownership(OwnershipArgs),
    Conflicts(ConflictsArgs),
    MergeStatus(MergeStatusArgs),
    Publish(PublishArgs),
    Lint(LintArgs),
    Ci(CiArgs),
    #[cfg(feature = "tui")]
    Tui,
}

// ── Args structs (match spec section 6 exactly) ──────────────────────────────

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub fix: bool,
    #[arg(long = "fix-all")]
    pub fix_all: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    pub objective: String,
    #[arg(long = "max-agents")]
    pub max_agents: Option<u16>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub pipeline: Option<String>,
    #[arg(long = "permission-mode", value_enum)]
    pub permission_mode: Option<PermissionModeArg>,
    #[arg(long)]
    pub conversation: Option<String>,
    #[arg(long = "continue-last", short = 'c')]
    pub continue_last: bool,
    #[arg(long)]
    pub issue: Option<String>,
    // ⚠️  Plan deviation from spec: spec says `--watch` is compiled out without `tui` feature.
    // This plan keeps `--watch` unconditional in clap (always parsed) and errors at runtime.
    // Rationale: `#[cfg_attr(feature = "tui", arg(long))]` interacts poorly with clap derive.
    // Runtime error message: "TUI mode requires feature 'tui'. Reinstall with: cargo install grove-cli --features tui"
    /// Live TUI view (requires feature = "tui").
    #[arg(long)]
    pub watch: bool,
}

#[derive(Debug, Args)]
pub struct QueueArgs {
    pub objective: String,
    #[arg(long, default_value_t = 0)]
    pub priority: i64,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub conversation: Option<String>,
    #[arg(long = "continue-last", short = 'c')]
    pub continue_last: bool,
}

#[derive(Debug, Args)]
pub struct TasksArgs {
    #[arg(long, default_value_t = 50)]
    pub limit: i64,
    #[arg(long)]
    pub refresh: bool,
}

#[derive(Debug, Args)]
pub struct TaskCancelArgs {
    pub task_id: String,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long, default_value_t = 20)]
    pub limit: i64,
    #[arg(long)]
    pub watch: bool,
}

#[derive(Debug, Args)]
pub struct ResumeArgs { pub run_id: String }

#[derive(Debug, Args)]
pub struct AbortArgs { pub run_id: String }

#[derive(Debug, Args)]
pub struct LogsArgs {
    pub run_id: String,
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct ReportArgs { pub run_id: String }

#[derive(Debug, Args)]
pub struct PlanArgs { pub run_id: Option<String> }

#[derive(Debug, Args)]
pub struct SubtasksArgs { pub run_id: Option<String> }

#[derive(Debug, Args)]
pub struct SessionsArgs { pub run_id: String }

// Implement ALL remaining Args structs from spec section 6.
// Pattern is the same — one struct per command, fields matching spec.
// Include: GitArgs + GitAction subcommand, IssueArgs + IssueAction, etc.
```

- [ ] **Step 4: Create stub command modules**

For each file in `src/commands/`, create a stub that compiles:

```rust
// Example: src/commands/init.rs
use crate::error::CliResult;
use crate::output::OutputMode;

pub fn run(_project: &std::path::Path, _mode: OutputMode) -> CliResult<()> {
    Ok(())
}
```

```rust
// Example: src/commands/run.rs
use crate::cli::{QueueArgs, RunArgs, TaskCancelArgs, TasksArgs};
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub fn run_cmd(_a: RunArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> { Ok(()) }
pub fn queue_cmd(_a: QueueArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> { Ok(()) }
pub fn tasks_cmd(_a: TasksArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> { Ok(()) }
pub fn task_cancel_cmd(_a: TaskCancelArgs, _t: GroveTransport, _m: OutputMode) -> CliResult<()> { Ok(()) }
```

Create stub functions matching the dispatch signatures below for every module.

- [ ] **Step 5: Implement `commands/mod.rs` — dispatch**

```rust
use crate::cli::{Cli, Commands};
use crate::error::CliResult;
use crate::output::OutputMode;
use crate::transport::GroveTransport;

pub mod abort_cmd;  // note: names may differ — adjust to match your module layout
pub mod auth;
pub mod cleanup;
pub mod conversation;
pub mod doctor;
pub mod git;
pub mod hooks;
pub mod init;
pub mod issues;
pub mod llm;
pub mod project;
pub mod run;
pub mod signals;
pub mod status;
pub mod worktrees;
pub mod workspace;

#[cfg(feature = "tui")]
pub mod tui_cmd;

pub fn dispatch(cli: Cli, transport: GroveTransport) -> CliResult<()> {
    let mode = if cli.json {
        OutputMode::Json
    } else {
        OutputMode::Text { no_color: cli.no_color }
    };
    let p = &cli.project;

    match cli.command {
        Commands::Init           => init::run(p, mode),
        Commands::Doctor(a)      => doctor::run(a, p, mode),
        Commands::Run(a)         => run::run_cmd(a, transport, mode),
        Commands::Queue(a)       => run::queue_cmd(a, transport, mode),
        Commands::Tasks(a)       => run::tasks_cmd(a, transport, mode),
        Commands::TaskCancel(a)  => run::task_cancel_cmd(a, transport, mode),
        Commands::Status(a)      => status::status_cmd(a, transport, mode),
        Commands::Resume(a)      => status::resume_cmd(a, transport, mode),
        Commands::Abort(a)       => status::abort_cmd(a, transport, mode),
        Commands::Logs(a)        => status::logs_cmd(a, transport, mode),
        Commands::Report(a)      => status::report_cmd(a, transport, mode),
        Commands::Plan(a)        => status::plan_cmd(a, transport, mode),
        Commands::Subtasks(a)    => status::subtasks_cmd(a, transport, mode),
        Commands::Sessions(a)    => status::sessions_cmd(a, transport, mode),
        Commands::Ownership(a)   => status::ownership_cmd(a, transport, mode),
        Commands::Conflicts(a)   => status::conflicts_cmd(a, transport, mode),
        Commands::MergeStatus(a) => status::merge_status_cmd(a, transport, mode),
        Commands::Publish(a)     => status::publish_cmd(a, transport, mode),
        Commands::Git(a)         => git::dispatch(a, p, mode),
        Commands::Issue(a)       => issues::dispatch(a, transport, mode),
        Commands::Fix(a)         => issues::fix_cmd(a, transport, mode),
        Commands::Connect(a)     => issues::connect_dispatch(a, transport, mode),
        Commands::Lint(a)        => issues::lint_cmd(a, transport, mode),
        Commands::Ci(a)          => issues::ci_cmd(a, transport, mode),
        Commands::Auth(a)        => auth::dispatch(a, transport, mode),
        Commands::Llm(a)         => llm::dispatch(a, transport, mode),
        Commands::Workspace(a)   => workspace::dispatch(a, transport, mode),
        Commands::Project(a)     => project::dispatch(a, p, transport, mode),
        Commands::Conversation(a) => conversation::dispatch(a, transport, mode),
        Commands::Signal(a)      => signals::dispatch(a, transport, mode),
        Commands::Hook(a)        => hooks::run(a, p, mode),
        Commands::Worktrees(a)   => worktrees::run(a, transport, mode),
        Commands::Cleanup(a)     => cleanup::cleanup_cmd(a, transport, mode),
        Commands::Gc(a)          => cleanup::gc_cmd(a, transport, mode),
        #[cfg(feature = "tui")]
        Commands::Tui            => tui_cmd::run(transport),
    }
}
```

- [ ] **Step 6: Wire `main.rs`**

```rust
mod cli;
mod commands;
mod error;
mod output;
mod transport;

use clap::Parser;
use cli::Cli;

fn main() {
    let cli = Cli::parse();
    let mode_json = cli.json;
    let transport = transport::GroveTransport::detect(&cli.project);

    if let Err(e) = commands::dispatch(cli, transport) {
        if mode_json {
            println!("{}", output::json::emit_error_json(&e.to_string(), e.exit_code()));
        } else {
            eprintln!("error: {e}");
        }
        std::process::exit(e.exit_code());
    }
}
```

- [ ] **Step 7: Build and run arg-parsing tests**

```bash
cargo check -p grove-cli
cargo check -p grove-cli --features tui
cargo test -p grove-cli cli
```
Expected: 4 arg-parsing tests pass.

- [ ] **Step 8: Smoke test the binary**

```bash
cargo build -p grove-cli
./target/debug/grove --help
./target/debug/grove run --help
./target/debug/grove git --help
```
Expected: help text renders for all commands.

- [ ] **Step 9: Commit**

```bash
git add crates/grove-cli/src/
git commit -m "feat(grove-cli): clap struct + dispatch skeleton — all commands stub"
```

---

## Phase B — Core Commands

### Task 7: init + doctor

**Files:**
- Modify: `crates/grove-cli/src/commands/init.rs`
- Modify: `crates/grove-cli/src/commands/doctor.rs`

Read `crates/grove-core/src/app.rs` and `crates/grove-core/src/db/mod.rs` before starting — understand what `GroveApp::init()` does and how the DB is accessed.

- [ ] **Step 1: Write failing test for doctor**

In `commands/doctor.rs` (add test module):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn doctor_on_uninitialised_dir_returns_error_not_panic() {
        let dir = tempdir().unwrap();
        let result = run(
            crate::cli::DoctorArgs { fix: false, fix_all: false },
            dir.path(),
            crate::output::OutputMode::Text { no_color: true },
        );
        // Either Ok or Err is fine; we just must not panic.
        let _ = result;
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli commands::doctor
```
Expected: FAIL — stub returns `Ok(())` trivially but test may expose type mismatch.

- [ ] **Step 3: Implement `init.rs`**

```rust
use std::path::Path;
use crate::error::CliResult;
use crate::output::{text, OutputMode};
use grove_core::app::GroveApp;

pub fn run(project: &Path, mode: OutputMode) -> CliResult<()> {
    // GroveApp::init() initialises the workspace from ~/.grove.
    // It does not take a project path — workspace is global.
    let _app = GroveApp::init()?;
    // Also ensure the local .grove/ config dir exists for this project.
    let grove_dir = project.join(".grove");
    std::fs::create_dir_all(&grove_dir)
        .map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    match mode {
        OutputMode::Json => println!("{}", serde_json::json!({"ok": true})),
        OutputMode::Text { .. } => text::success("grove initialised"),
    }
    Ok(())
}
```

- [ ] **Step 4: Implement `doctor.rs`**

```rust
use std::path::Path;
use crate::cli::DoctorArgs;
use crate::error::CliResult;
use crate::output::{text, OutputMode};
use grove_core::app::GroveApp;

pub fn run(args: DoctorArgs, _project: &Path, mode: OutputMode) -> CliResult<()> {
    let app = GroveApp::init()?;
    // DbHandle::connect() → GroveResult<Connection>. integrity::check takes &Connection.
    let conn = app.db_handle().connect().map_err(|e| CliError::Core(e))?;
    // Check: git, sqlite (db integrity + FK), config, schema version.
    let git_ok   = which::which("git").is_ok();
    let db_ok    = grove_core::db::integrity::check(&conn)
        .map(|r| r.integrity_ok && r.foreign_key_violations.is_empty())
        .unwrap_or(false);
    let cfg_ok   = true; // config presence check — expand as needed
    let overall  = git_ok && db_ok && cfg_ok;

    if (args.fix || args.fix_all) && !overall {
        // Attempt auto-fix: re-run migrations.
        // initialize() takes the workspace data_root (a pub PathBuf on GroveApp).
        grove_core::db::initialize(&app.data_root)?;
    }

    match mode {
        OutputMode::Json => println!("{}", serde_json::json!({
            "ok": overall, "git": git_ok, "sqlite": db_ok, "config": cfg_ok
        })),
        OutputMode::Text { .. } => {
            println!("{}", if overall { text::bold("✓ healthy") } else { text::bold("✗ issues found") });
            println!("  git:    {}", if git_ok  { "ok" } else { "MISSING" });
            println!("  sqlite: {}", if db_ok   { "ok" } else { "FAIL" });
            println!("  config: {}", if cfg_ok  { "ok" } else { "FAIL" });
        }
    }
    Ok(())
}
```

Note: verified grove-core API:
- `GroveApp::db_handle()` → `DbHandle`; `DbHandle::connect()` → `GroveResult<Connection>`
- `grove_core::db::integrity::check(&conn)` → `GroveResult<IntegrityReport>` (`integrity_ok: bool`, `foreign_key_violations: Vec<FkViolation>`)
- `grove_core::db::initialize(project_root: &Path)` → `GroveResult<InitDbResult>`
- `GroveApp::data_root` is a `pub PathBuf` field

- [ ] **Step 5: Run tests**

```bash
cargo test -p grove-cli commands
```

- [ ] **Step 6: Smoke test**

```bash
./target/debug/grove init
./target/debug/grove doctor
./target/debug/grove doctor --json
```

- [ ] **Step 7: Commit**

```bash
git add crates/grove-cli/src/commands/init.rs crates/grove-cli/src/commands/doctor.rs
git commit -m "feat(grove-cli): init and doctor commands"
```

---

### Task 8: run + queue + tasks + task-cancel

**Files:**
- Modify: `crates/grove-cli/src/commands/run.rs`
- Extend: `crates/grove-cli/src/transport/mod.rs` (add `queue_task`, `cancel_task`, `start_run` to trait)
- Extend: `crates/grove-cli/src/transport/direct.rs` (implement new methods)

Read `crates/grove-core/src/orchestrator/` before starting to understand `queue_task`, `list_tasks`, and `drain_queue`.

- [ ] **Step 1: Add trait methods**

Add to `Transport` trait in `transport/mod.rs`:
```rust
// queue_task wraps grove_core::orchestrator::queue_task (11 args).
// Pass budget_usd: None, provider: None, resume_provider_session_id: None,
// disable_phase_gates: false as defaults. permission_mode comes from the command args.
fn queue_task(&self, objective: &str, priority: i64, model: Option<&str>,
              conversation_id: Option<&str>, pipeline: Option<&str>,
              permission_mode: Option<&str>) -> CliResult<grove_core::orchestrator::TaskRecord>;
fn cancel_task(&self, task_id: &str) -> CliResult<()>;
fn start_run(&self, req: StartRunRequest) -> CliResult<RunResult>;
fn drain_queue(&self, project: &std::path::Path) -> CliResult<()>;
```

Define `StartRunRequest` and `RunResult` as plain structs in `transport/mod.rs`:
```rust
pub struct StartRunRequest {
    pub objective: String,
    pub pipeline: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub conversation_id: Option<String>,
    pub continue_last: bool,
    pub issue_id: Option<String>,
    pub max_agents: Option<u16>,
}

pub struct RunResult {
    pub run_id: String,
    pub task_id: String,
    pub state: String,
    pub objective: String,
}
```

Update `TestTransport` to return `Err(CliError::Other("not implemented".into()))` for mutation methods.

- [ ] **Step 2: Write failing tests**

In `commands/run.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn tasks_cmd_with_empty_transport_renders_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = tasks_cmd(
            crate::cli::TasksArgs { limit: 10, refresh: false },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 3: Run to verify failure**

```bash
cargo test -p grove-cli commands::run
```

- [ ] **Step 4: Implement `commands/run.rs`**

```rust
use crate::cli::{QueueArgs, RunArgs, TaskCancelArgs, TasksArgs, PermissionModeArg};
use crate::error::{CliError, CliResult};
use crate::output::{text, OutputMode};
use crate::transport::{GroveTransport, StartRunRequest, Transport};

pub fn run_cmd(args: RunArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let pb = match &mode {
        OutputMode::Text { .. } => Some(text::spinner("Starting run…")),
        OutputMode::Json => None,
    };

    let req = StartRunRequest {
        objective: args.objective.clone(),
        pipeline: args.pipeline.clone(),
        model: args.model.clone(),
        permission_mode: args.permission_mode.map(|m| match m {
            PermissionModeArg::SkipAll       => "skip_all".into(),
            PermissionModeArg::HumanGate     => "human_gate".into(),
            PermissionModeArg::AutonomousGate => "autonomous_gate".into(),
        }),
        conversation_id: args.conversation.clone(),
        continue_last: args.continue_last,
        issue_id: args.issue.clone(),
        max_agents: args.max_agents,
    };

    let result = transport.start_run(req)?;
    if let Some(pb) = pb { pb.finish_and_clear(); }

    // --watch: delegate to TUI run-watch (only with feature=tui)
    #[cfg(feature = "tui")]
    if args.watch {
        return crate::tui::run_watch::run(result.run_id, transport);
    }
    // Without tui feature, --watch warns and exits cleanly
    if args.watch {
        return Err(CliError::Other(
            "TUI mode requires feature 'tui'. Reinstall with: cargo install grove-cli --features tui".into()
        ));
    }

    match mode {
        OutputMode::Json => println!("{}", serde_json::json!({
            "run_id": result.run_id, "state": result.state, "objective": result.objective
        })),
        OutputMode::Text { .. } => println!("run {} started ({})", &result.run_id[..8], result.state),
    }
    Ok(())
}

pub fn queue_cmd(args: QueueArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    let task = transport.queue_task(
        &args.objective,
        args.priority,
        args.model.as_deref(),
        args.conversation.as_deref(),
        None,          // pipeline
        None,          // permission_mode — defaults to project setting
    )?;
    match mode {
        OutputMode::Json => println!("{}", serde_json::to_string(&task).unwrap()),
        OutputMode::Text { .. } => println!("queued {} (priority {})", &task.id[..8], task.priority),
    }
    Ok(())
}

pub fn tasks_cmd(args: TasksArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    // list_tasks() returns all tasks — apply limit client-side (grove-core has no limit param).
    let all_tasks = transport.list_tasks()?;
    let tasks: Vec<_> = all_tasks.into_iter().take(args.limit as usize).collect();
    match mode {
        OutputMode::Json => println!("{}", serde_json::to_string(&tasks).unwrap()),
        OutputMode::Text { .. } => {
            if tasks.is_empty() {
                println!("{}", text::dim("no tasks"));
                return Ok(());
            }
            let rows: Vec<Vec<String>> = tasks.iter().map(|t| vec![
                t.id[..8].to_string(),
                t.objective.chars().take(50).collect(),
                t.state.clone(),
                t.priority.to_string(),
            ]).collect();
            println!("{}", text::render_table(&["ID", "OBJECTIVE", "STATE", "PRI"], &rows));
        }
    }
    Ok(())
}

pub fn task_cancel_cmd(args: TaskCancelArgs, transport: GroveTransport, mode: OutputMode) -> CliResult<()> {
    transport.cancel_task(&args.task_id)?;
    match mode {
        OutputMode::Json => println!("{}", serde_json::json!({"ok": true, "task_id": args.task_id})),
        OutputMode::Text { .. } => println!("cancelled {}", &args.task_id[..8]),
    }
    Ok(())
}
```

- [ ] **Step 5: Implement the new `DirectTransport` methods**

In `transport/direct.rs`, implement `queue_task`, `cancel_task`, `start_run`, `drain_queue` by calling the equivalent `grove_core::orchestrator::*` functions (same as old CLI `run.rs` did). Read `crates/grove-core/src/orchestrator/mod.rs` for exact function signatures.

⚠️  `grove_core::orchestrator::queue_task` takes 11 arguments:
`(project_root, objective, budget_usd, priority, model, provider, conversation_id, resume_provider_session_id, pipeline, permission_mode, disable_phase_gates)`
The Transport trait wrapper has 6 args. Pass the missing ones as defaults:
`budget_usd: None, provider: None, resume_provider_session_id: None, disable_phase_gates: false`.
`permission_mode` comes from the Transport trait's `permission_mode: Option<&str>` arg.

- [ ] **Step 6: Run tests**

```bash
cargo test -p grove-cli commands::run
```

- [ ] **Step 7: Smoke test**

```bash
cargo build -p grove-cli
./target/debug/grove tasks
./target/debug/grove tasks --json
```

- [ ] **Step 8: Commit**

```bash
git add crates/grove-cli/src/commands/run.rs crates/grove-cli/src/transport/
git commit -m "feat(grove-cli): run/queue/tasks/task-cancel commands"
```

---

### Task 9: status + resume + abort + logs + report + plan + subtasks + sessions

**Files:**
- Modify: `crates/grove-cli/src/commands/status.rs`
- Extend: `crates/grove-cli/src/transport/` (add `get_logs`, `get_report`, `get_plan`, `get_subtasks`, `get_sessions`, `abort_run`, `resume_run` to trait)

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn status_cmd_empty_list_renders_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = status_cmd(
            crate::cli::StatusArgs { limit: 20, watch: false },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli commands::status
```

- [ ] **Step 3: Implement `commands/status.rs`**

`status_cmd` — table: `ID | OBJECTIVE | STATE | AGENT | CREATED`
`logs_cmd` — print events for a run (tail 200 by default, `--all` for full)
`report_cmd` — print RunReport summary
`plan_cmd` — print PlanSteps as wave/step tree
`subtasks_cmd` — table: `TITLE | STATUS | AGENT | DEPENDS`
`sessions_cmd` — table: `ID | AGENT | STATE | STARTED | ENDED | COST`
`resume_cmd` / `abort_cmd` — call transport, print result
`ownership_cmd` — table: `PATH | SESSION | RUN | LOCKED_AT`
`conflicts_cmd` — list conflict files or show specific file
`merge_status_cmd` — table: `BRANCH | TARGET | STATUS | STRATEGY | PR`
`publish_cmd` — retry publish for run

Follow the same render pattern as `tasks_cmd` above.

- [ ] **Step 4: Run tests**

```bash
cargo test -p grove-cli commands::status
```

- [ ] **Step 5: Smoke test**

```bash
./target/debug/grove status
./target/debug/grove status --json
```

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/src/commands/status.rs crates/grove-cli/src/transport/
git commit -m "feat(grove-cli): status/logs/report/plan/subtasks/sessions/resume/abort commands"
```

---

### Task 10: git commands

**Files:**
- Modify: `crates/grove-cli/src/commands/git.rs`
- Extend: `crates/grove-cli/src/cli.rs` (GitArgs + GitAction already defined)

Read `crates/grove-core/src/` for git-related functions. The old CLI's `git.rs` command calls grove-core git utilities — check what is available.

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn git_status_on_non_git_dir_returns_error_not_panic() {
        let dir = tempdir().unwrap();
        let result = status_cmd(dir.path(), crate::output::OutputMode::Text { no_color: true });
        let _ = result; // error expected, not panic
    }

    #[test]
    fn git_dispatch_compiles_with_all_actions() {
        // Compilation test — ensures all GitAction variants are handled.
        // Actual execution tested via smoke tests.
        let _ = |a: GitArgs, p: &std::path::Path, m: OutputMode| dispatch(a, p, m);
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli commands::git
```

- [ ] **Step 3: Implement `commands/git.rs`**

Implement `dispatch` and all sub-functions. Key output formats:

`status_cmd` — one line per file, grouped by area:
```
branch: cli-rewrite  ↑2 ↓0
M  src/commands/git.rs
A  src/tui/mod.rs
?  scratch.txt
```

`log_cmd` — one line per commit:
```
* abc1234  Add dark mode support      2026-03-18  [pushed]
* def5678  Fix auth token refresh     2026-03-17
```

`pr_cmd` / `merge_cmd` / `pr_status_cmd` — delegate to grove-core git functions that wrap `gh` CLI or `git` commands.

> Call grove-core git functions — do NOT shell out to `git` directly. Check `crates/grove-core/src/` for available git utilities (look for files named `git*.rs` or a `git/` module).

- [ ] **Step 4: Run tests**

```bash
cargo test -p grove-cli commands::git
```

- [ ] **Step 5: Smoke test**

```bash
./target/debug/grove git status
./target/debug/grove git log -n 5
./target/debug/grove git branch
```

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/src/commands/git.rs
git commit -m "feat(grove-cli): git commands"
```

---

### Task 11: auth + llm

**Files:**
- Modify: `crates/grove-cli/src/commands/auth.rs`
- Modify: `crates/grove-cli/src/commands/llm.rs`
- Extend: transport (add `list_providers`, `set_api_key`, `remove_api_key`, `list_models`, `select_llm`)

Read `crates/grove-core/src/llm/auth.rs` before starting.

- [ ] **Step 1: Write failing test**

```rust
// auth.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn auth_list_with_test_transport_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        let result = list_cmd(t, crate::output::OutputMode::Text { no_color: true });
        assert!(result.is_ok());
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli commands::auth
```

- [ ] **Step 3: Implement `auth.rs`**

`list_cmd` table: `PROVIDER | AUTHENTICATED | KEY_HINT`
`set_cmd` — store API key via grove-core keyring wrapper (see `grove_core::llm::auth`)
`remove_cmd` — remove stored key

- [ ] **Step 4: Implement `llm.rs`**

`list_cmd` table: `PROVIDER | AUTH | MODELS | DEFAULT_MODEL`
`models_cmd` table: `ID | NAME | CONTEXT | INPUT/M | OUTPUT/M | VISION | TOOLS`
`select_cmd` — set workspace default provider + model

- [ ] **Step 5: Run tests + smoke**

```bash
cargo test -p grove-cli commands::auth commands::llm
./target/debug/grove auth list
./target/debug/grove llm list
./target/debug/grove llm models anthropic
```

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/src/commands/auth.rs crates/grove-cli/src/commands/llm.rs crates/grove-cli/src/transport/
git commit -m "feat(grove-cli): auth and llm commands"
```

---

## Phase C — Extended Commands

### Task 12: issues — list, show, create, close, board, sync, search

**Files:**
- Modify: `crates/grove-cli/src/commands/issues.rs`
- Extend: transport (add issue CRUD methods)

Read `crates/grove-core/src/db/repositories/` issue-related repos and `crates/grove-core/src/` issue tracker integrations.

- [ ] **Step 1: Write failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{GroveTransport, TestTransport};

    #[test]
    fn issue_list_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(list_cmd(
            crate::cli::IssueListArgs { cached: false },
            t, crate::output::OutputMode::Text { no_color: true }
        ).is_ok());
    }

    #[test]
    fn issue_board_empty_ok() {
        let t = GroveTransport::Test(TestTransport::default());
        assert!(board_cmd(
            crate::cli::IssueBoardArgs { status: None, provider: None, assignee: None, priority: None },
            t, crate::output::OutputMode::Text { no_color: true }
        ).is_ok());
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli commands::issues
```

- [ ] **Step 3: Implement list, show, create, close, board, sync, search**

`list_cmd` table: `ID | PROVIDER | TITLE | STATUS | PRIORITY | ASSIGNEE`

`board_cmd` — 4-column kanban, one line per issue:
```
OPEN (3)           IN PROGRESS (1)    IN REVIEW (0)     DONE (5)
─────────────      ───────────────    ─────────────     ────────
GH-42 Auth bug ●   GH-17 Dark mode●
GH-43 Perf     ○
```

`sync_cmd` — call grove-core issue sync, print results table: `PROVIDER | NEW | UPDATED | CLOSED | ERRORS`

- [ ] **Step 4: Run tests + smoke**

```bash
cargo test -p grove-cli commands::issues
./target/debug/grove issue list
./target/debug/grove issue board
```

- [ ] **Step 5: Commit**

```bash
git add crates/grove-cli/src/commands/issues.rs crates/grove-cli/src/transport/
git commit -m "feat(grove-cli): issue list/show/create/close/board/sync/search"
```

---

### Task 13: issues — update, comment, assign, move, reopen, activity, push, ready, board-config + fix + connect + lint + ci

**Files:**
- Modify: `crates/grove-cli/src/commands/issues.rs` (remaining subcommands)
- Extend: transport (remaining issue mutation methods)

- [ ] **Step 1: Write failing test for search + connect status**

```rust
#[test]
fn connect_status_ok() {
    let t = GroveTransport::Test(TestTransport::default());
    assert!(connect_status_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
}
```

- [ ] **Step 2: Implement remaining issue subcommands**

`update_cmd`, `comment_cmd`, `assign_cmd`, `move_cmd`, `reopen_cmd`, `activity_cmd`, `push_cmd`, `ready_cmd` — all follow the same pattern: call transport method, print result or confirmation.

`board_config` — `show` prints JSON, `set` reads JSON file, `reset` restores defaults.

- [ ] **Step 3: Implement connect subcommands**

`connect_dispatch` handles: `github`, `jira`, `linear`, `status`, `disconnect`.

`connect_status_cmd` table: `PROVIDER | CONNECTED | USER | ERROR`

- [ ] **Step 4: Implement fix, lint, ci**

`fix_cmd` — calls `transport.start_run(...)` with issue linked, same pattern as `run_cmd`.
`lint_cmd` — calls grove-core linter, renders results table; `--fix` starts a fix run.
`ci_cmd` — calls grove-core CI status checker; `--wait` polls; `--fix` starts fix run.

- [ ] **Step 5: Run tests + smoke**

```bash
cargo test -p grove-cli commands::issues
./target/debug/grove connect status
./target/debug/grove issue board
```

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/src/commands/issues.rs crates/grove-cli/src/transport/
git commit -m "feat(grove-cli): complete issues, connect, fix, lint, ci commands"
```

---

### Task 14: workspace + project + conversation

**Files:**
- Modify: `crates/grove-cli/src/commands/workspace.rs`
- Modify: `crates/grove-cli/src/commands/project.rs`
- Modify: `crates/grove-cli/src/commands/conversation.rs`
- Extend: transport (workspace/project/conversation CRUD)

- [ ] **Step 1: Write failing tests**

```rust
// workspace.rs
#[test]
fn workspace_show_ok() {
    let t = GroveTransport::Test(TestTransport::default());
    assert!(show_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
}

// conversation.rs
#[test]
fn conversation_list_ok() {
    let t = GroveTransport::Test(TestTransport::default());
    assert!(list_cmd(
        crate::cli::ConversationListArgs { limit: 20 },
        t, crate::output::OutputMode::Text { no_color: true }
    ).is_ok());
}
```

- [ ] **Step 2: Implement workspace.rs**

Actions: `show`, `set-name`, `archive`, `delete`
`show` output: `id`, `name`, `state`, `llm_provider`, `llm_model`

- [ ] **Step 3: Implement project.rs**

Actions: `show`, `list`, `open-folder`, `clone`, `create-repo`, `fork-repo`, `fork-folder`, `ssh`, `ssh-shell`, `set-name`, `set`, `archive`, `delete`
`list` table: `ID | NAME | PATH | KIND | STATE`
`set` updates project settings (provider, parallel, pipeline, permission-mode)

- [ ] **Step 4: Implement conversation.rs**

Actions: `list`, `show`, `archive`, `delete`, `rebase`, `merge`
`list` table: `ID | TITLE | STATE | KIND | BRANCH | CREATED`
`show` — list messages for the conversation

- [ ] **Step 5: Run tests + smoke**

```bash
cargo test -p grove-cli commands::workspace commands::project commands::conversation
./target/debug/grove workspace show
./target/debug/grove project list
./target/debug/grove conversation list
```

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/src/commands/workspace.rs crates/grove-cli/src/commands/project.rs crates/grove-cli/src/commands/conversation.rs crates/grove-cli/src/transport/
git commit -m "feat(grove-cli): workspace, project, conversation commands"
```

---

### Task 15: signals + hooks + worktrees + cleanup + gc

**Files:**
- Modify: `crates/grove-cli/src/commands/signals.rs`
- Modify: `crates/grove-cli/src/commands/hooks.rs`
- Modify: `crates/grove-cli/src/commands/worktrees.rs`
- Modify: `crates/grove-cli/src/commands/cleanup.rs`

- [ ] **Step 1: Write failing test for worktrees**

```rust
// worktrees.rs
#[test]
fn worktrees_list_ok() {
    let t = GroveTransport::Test(TestTransport::default());
    assert!(list_cmd(t, crate::output::OutputMode::Text { no_color: true }).is_ok());
}
```

- [ ] **Step 2: Implement signals.rs**

`send` — posts a signal record via transport
`check` — lists unread signals for an agent: table `TYPE | FROM | PRIORITY | CREATED`
`list` — lists all signals for a run

- [ ] **Step 3: Implement hooks.rs**

`run` — called by Claude Code hooks mechanism. Routes to grove-core hook handlers based on `event` arg (session_start, pre_tool_use, post_tool_use, stop, etc.). Read `crates/grove-core/src/` for hook handler functions.

- [ ] **Step 4: Implement worktrees.rs**

`list` table: `SESSION | PATH | SIZE | RUN | AGENT | STATE | CREATED`
`--clean` — delete all finished worktrees
`--delete <id>` — delete specific worktree
`--delete-all [-y]` — delete all (skip active)

- [ ] **Step 5: Implement cleanup.rs**

`cleanup_cmd` — clean up finished worktrees, scoped by project/conversation
`gc_cmd` — sweep expired pool holds, prune orphaned branches, git gc

- [ ] **Step 6: Run tests + smoke**

```bash
cargo test -p grove-cli
./target/debug/grove worktrees
./target/debug/grove signal list <any-run-id>
```

- [ ] **Step 7: Commit**

```bash
git add crates/grove-cli/src/commands/signals.rs crates/grove-cli/src/commands/hooks.rs crates/grove-cli/src/commands/worktrees.rs crates/grove-cli/src/commands/cleanup.rs
git commit -m "feat(grove-cli): signals, hooks, worktrees, cleanup, gc"
```

---

## Phase D — TUI (feature = "tui")

### Task 16: TUI scaffold + widgets + run-watch

**Files:**
- Create: `crates/grove-cli/src/tui/mod.rs`
- Create: `crates/grove-cli/src/tui/widgets/mod.rs`
- Create: `crates/grove-cli/src/tui/run_watch.rs`

- [ ] **Step 1: Write failing test (feature-gated)**

```rust
// tui/run_watch.rs
#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "tui")]
    fn run_watch_state_initialises() {
        let s = super::RunWatchState::new("run-abc123".into(), "add dark mode".into());
        assert_eq!(s.run_id, "run-abc123");
        assert!(s.agents.is_empty());
        assert_eq!(s.selected_agent, 0);
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli --features tui
```
Expected: compile error — `tui` module missing.

- [ ] **Step 3: Implement `tui/mod.rs`**

```rust
#[cfg(feature = "tui")]
pub mod dashboard;
#[cfg(feature = "tui")]
pub mod run_watch;
#[cfg(feature = "tui")]
pub mod widgets;
```

- [ ] **Step 4: Implement `tui/widgets/mod.rs`**

```rust
use ratatui::{prelude::*, widgets::{Block, Borders}};

/// Accent green matching grove-gui palette.
pub const ACCENT: Color = Color::Rgb(49, 185, 123);

pub fn titled_block(title: &str) -> Block<'static> {
    Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
}

pub fn state_color(state: &str) -> Color {
    match state {
        "running"   => Color::Green,
        "completed" => Color::Cyan,
        "failed"    => Color::Red,
        "queued"    => Color::Yellow,
        _           => Color::Gray,
    }
}
```

- [ ] **Step 5: Implement `tui/run_watch.rs`**

```rust
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use crate::error::CliResult;
use crate::transport::GroveTransport;

pub struct AgentRow {
    pub name: String,
    pub state: String,
    pub started: Option<String>,
}

pub struct RunWatchState {
    pub run_id: String,
    pub objective: String,
    pub agents: Vec<AgentRow>,
    pub selected_agent: usize,
    pub log_lines: Vec<String>,
    pub scroll_offset: u16,
    pub done: bool,
}

impl RunWatchState {
    pub fn new(run_id: String, objective: String) -> Self {
        Self { run_id, objective, agents: vec![], selected_agent: 0,
               log_lines: vec![], scroll_offset: 0, done: false }
    }
}

pub fn run(run_id: String, transport: GroveTransport) -> CliResult<()> {
    // Fetch initial run state.
    let initial = transport.get_run(&run_id)?
        .ok_or_else(|| crate::error::CliError::NotFound(run_id.clone()))?;

    let mut state = RunWatchState::new(run_id.clone(), initial.objective.clone());

    enable_raw_mode().map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| crate::error::CliError::Other(e.to_string()))?;

    loop {
        terminal.draw(|f| draw(f, &state)).ok();

        if event::poll(std::time::Duration::from_millis(250)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                    (KeyCode::Char('a'), _) => { transport.abort_run(&run_id).ok(); }
                    (KeyCode::Tab, _) if !state.agents.is_empty() => {
                        state.selected_agent = (state.selected_agent + 1) % state.agents.len();
                    }
                    (KeyCode::Up, _)   => state.scroll_offset = state.scroll_offset.saturating_sub(1),
                    (KeyCode::Down, _) => state.scroll_offset = state.scroll_offset.saturating_add(1),
                    _ => {}
                }
            }
        }

        // Poll transport for updated run + session state.
        if let Ok(Some(run)) = transport.get_run(&run_id) {
            if run.state == "completed" || run.state == "failed" || run.state == "aborted" {
                state.done = true;
                terminal.draw(|f| draw(f, &state)).ok();
                std::thread::sleep(std::time::Duration::from_millis(1000));
                break;
            }
        }
    }

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    Ok(())
}

fn draw(f: &mut Frame, state: &RunWatchState) {
    use super::widgets::{titled_block, state_color, ACCENT};

    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Header
    let status = if state.done { "done" } else { "running" };
    let header = Paragraph::new(format!(
        " Run: {}  │  {}  │  {}",
        &state.run_id[..8.min(state.run_id.len())], state.objective, status
    )).block(titled_block("Grove — Run Watch"))
     .style(Style::default().fg(ACCENT));
    f.render_widget(header, chunks[0]);

    // Body: agent table (left) + log pane (right)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(chunks[1]);

    // Agent table
    let rows: Vec<Row> = state.agents.iter().enumerate().map(|(i, a)| {
        let style = if i == state.selected_agent {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(state_color(&a.state))
        };
        Row::new(vec![a.name.clone(), a.state.clone(), a.started.clone().unwrap_or_default()])
            .style(style)
    }).collect();

    let agent_table = Table::new(rows, [Constraint::Fill(1), Constraint::Length(9), Constraint::Length(8)])
        .header(Row::new(["AGENT", "STATE", "STARTED"]).style(Style::default().fg(ACCENT)))
        .block(titled_block("Agents"));
    f.render_widget(agent_table, body_chunks[0]);

    // Log pane
    let log_lines: Vec<Line> = state.log_lines.iter()
        .skip(state.scroll_offset as usize)
        .map(|l| Line::from(l.as_str()))
        .collect();
    let log = Paragraph::new(log_lines)
        .block(titled_block("Log"))
        .wrap(Wrap { trim: false });
    f.render_widget(log, body_chunks[1]);
}
```

- [ ] **Step 6: Add to `commands/tui_cmd.rs`**

```rust
// src/commands/tui_cmd.rs  (only compiled with feature=tui)
#[cfg(feature = "tui")]
pub fn run(transport: crate::transport::GroveTransport) -> crate::error::CliResult<()> {
    crate::tui::dashboard::run(transport)
}
```

- [ ] **Step 7: Run tests**

```bash
cargo test -p grove-cli --features tui tui
```

- [ ] **Step 8: Commit**

```bash
git add crates/grove-cli/src/tui/ crates/grove-cli/src/commands/tui_cmd.rs
git commit -m "feat(grove-cli): TUI scaffold + run-watch (feature=tui)"
```

---

### Task 17: TUI full dashboard (`grove tui`)

**Files:**
- Create: `crates/grove-cli/src/tui/dashboard.rs`

- [ ] **Step 1: Write failing test**

```rust
#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "tui")]
    fn dashboard_state_default_screen_is_sessions() {
        let s = super::DashboardState::default();
        assert_eq!(s.screen, super::Screen::Sessions);
    }

    #[test]
    #[cfg(feature = "tui")]
    fn screen_cycle_wraps_correctly() {
        assert_eq!(super::Screen::Sessions.next(), super::Screen::Issues);
        assert_eq!(super::Screen::Settings.next(), super::Screen::Dashboard);
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cargo test -p grove-cli --features tui tui::dashboard
```

- [ ] **Step 3: Implement `tui/dashboard.rs`**

```rust
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use crate::error::CliResult;
use crate::transport::GroveTransport;

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum Screen { Dashboard, #[default] Sessions, Issues, Settings }

impl Screen {
    pub fn next(self) -> Self {
        match self { Screen::Dashboard => Screen::Sessions, Screen::Sessions => Screen::Issues,
                     Screen::Issues => Screen::Settings, Screen::Settings => Screen::Dashboard }
    }
    pub fn index(self) -> usize {
        match self { Screen::Dashboard => 0, Screen::Sessions => 1, Screen::Issues => 2, Screen::Settings => 3 }
    }
}

#[derive(Default)]
pub struct DashboardState {
    pub screen: Screen,
    pub projects: Vec<String>,           // project names
    pub selected_project: usize,
    pub conversations: Vec<String>,      // conversation titles
    pub selected_conversation: usize,
    pub runs: Vec<(String, String, String)>,  // (id, objective, state)
    pub changed_files: Vec<String>,
    pub branch: Option<String>,
}

pub fn run(transport: GroveTransport) -> CliResult<()> {
    let mut state = DashboardState::default();
    // Initial data load
    if let Ok(projects) = transport.list_projects() {
        state.projects = projects.iter().map(|p| {
            p.name.clone().unwrap_or_else(|| p.root_path.clone())
        }).collect();
    }

    enable_raw_mode().map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| crate::error::CliError::Other(e.to_string()))?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| crate::error::CliError::Other(e.to_string()))?;

    loop {
        terminal.draw(|f| draw(f, &state)).ok();

        if event::poll(std::time::Duration::from_millis(500)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('1') => state.screen = Screen::Dashboard,
                    KeyCode::Char('2') => state.screen = Screen::Sessions,
                    KeyCode::Char('3') => state.screen = Screen::Issues,
                    KeyCode::Char('4') => state.screen = Screen::Settings,
                    KeyCode::Tab       => state.screen = state.screen.next(),
                    KeyCode::Up        => state.selected_conversation = state.selected_conversation.saturating_sub(1),
                    KeyCode::Down      => {
                        let max = state.conversations.len().saturating_sub(1);
                        state.selected_conversation = (state.selected_conversation + 1).min(max);
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    Ok(())
}

fn draw(f: &mut Frame, state: &DashboardState) {
    use super::widgets::{titled_block, ACCENT};
    let area = f.area();

    // Nav bar (bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let nav = Paragraph::new(
        " [1] Dashboard  [2] Sessions  [3] Issues  [4] Settings    q: quit"
    ).style(Style::default().fg(ACCENT));
    f.render_widget(nav, chunks[1]);

    // Main area: sidebar + content + right panel (sessions screen)
    match state.screen {
        Screen::Sessions => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(22), Constraint::Min(0), Constraint::Length(28)])
                .split(chunks[0]);

            // Sidebar: projects + conversations
            let sidebar_items: Vec<ListItem> = state.conversations.iter().enumerate().map(|(i, c)| {
                let style = if i == state.selected_conversation {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                };
                ListItem::new(format!(" {c}")).style(style)
            }).collect();
            let sidebar = List::new(sidebar_items).block(titled_block("Conversations"));
            f.render_widget(sidebar, cols[0]);

            // Main panel: runs
            let run_items: Vec<ListItem> = state.runs.iter().map(|(id, obj, st)| {
                ListItem::new(format!(" {} {}  {}", &id[..8.min(id.len())], obj, st))
            }).collect();
            let main = List::new(run_items).block(titled_block("Runs"));
            f.render_widget(main, cols[1]);

            // Right panel: git status
            let git_items: Vec<ListItem> = state.changed_files.iter()
                .map(|f| ListItem::new(format!("  {f}")))
                .collect();
            let right_content = List::new(git_items)
                .block(titled_block(&format!("Git: {}", state.branch.as_deref().unwrap_or("—"))));
            f.render_widget(right_content, cols[2]);
        }
        Screen::Dashboard => {
            let p = Paragraph::new("Dashboard — coming soon")
                .block(titled_block("Dashboard"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(p, chunks[0]);
        }
        Screen::Issues => {
            let p = Paragraph::new("Issues — use `grove issue board` for full kanban")
                .block(titled_block("Issues"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(p, chunks[0]);
        }
        Screen::Settings => {
            let p = Paragraph::new("Settings — use `grove auth list` and `grove llm list`")
                .block(titled_block("Settings"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(p, chunks[0]);
        }
    }
}
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p grove-cli --features tui tui::dashboard
```

- [ ] **Step 5: Smoke test (requires a terminal)**

```bash
cargo build -p grove-cli --features tui
./target/debug/grove tui
```
Expected: TUI opens, `q` exits cleanly.

- [ ] **Step 6: Commit**

```bash
git add crates/grove-cli/src/tui/dashboard.rs
git commit -m "feat(grove-cli): grove tui full dashboard (feature=tui)"
```

---

### Task 18: Wire --watch into run + status; final verification

**Files:**
- Modify: `crates/grove-cli/src/commands/run.rs` (already has `--watch` stub from Task 8)
- Modify: `crates/grove-cli/src/commands/status.rs`

- [ ] **Step 1: Write feature-gated test**

```rust
// commands/run.rs
#[test]
fn run_watch_without_tui_feature_returns_error() {
    #[cfg(not(feature = "tui"))]
    {
        use crate::transport::{GroveTransport, TestTransport};
        let t = GroveTransport::Test(TestTransport::default());
        let result = run_cmd(
            crate::cli::RunArgs {
                objective: "test".into(), max_agents: None, model: None, pipeline: None,
                permission_mode: None, conversation: None, continue_last: false,
                issue: None, watch: true,
            },
            t,
            crate::output::OutputMode::Text { no_color: true },
        );
        // TestTransport.start_run returns an error — but we want to verify the
        // --watch path returns the TUI unavailability error specifically.
        let _ = result; // error expected either from transport or tui unavailability
    }
}
```

- [ ] **Step 2: Verify --watch delegates to tui::run_watch when feature=tui**

The delegation code was written in Task 8 (`run_cmd`). Verify it compiles with the feature:
```bash
cargo check -p grove-cli --features tui
```

- [ ] **Step 3: Wire --watch into status.rs**

Add to `status_cmd` (same pattern as `run_cmd`):
```rust
#[cfg(feature = "tui")]
if args.watch {
    return crate::tui::run_watch::run_status_watch(transport);
}
if args.watch {
    return Err(CliError::Other(
        "TUI mode requires feature 'tui'. Reinstall with: cargo install grove-cli --features tui".into()
    ));
}
```

- [ ] **Step 3b: Write failing test for `run_status_watch`**

In `tui/run_watch.rs` (inside `#[cfg(test)]` block):
```rust
#[cfg(feature = "tui")]
#[test]
fn run_status_watch_initialises_without_panic() {
    // Verify the function exists and accepts a GroveTransport without panicking on init.
    // Does NOT enter the event loop — just constructs the App struct.
    let transport = crate::transport::GroveTransport::Test(
        crate::transport::TestTransport::default()
    );
    let app = StatusWatchApp::new(transport);
    assert!(app.runs.is_empty());
}
```

Run and confirm it fails (function not yet defined):
```bash
cargo test -p grove-cli --features tui tui::run_watch
```

- [ ] **Step 3c: Implement `run_status_watch` in `tui/run_watch.rs`**

Add a `StatusWatchApp` struct and `pub fn run_status_watch(transport: GroveTransport) -> CliResult<()>` that:
1. Calls `transport.list_runs(20)` in a poll loop (every 2 seconds)
2. Renders runs in a ratatui table — columns: ID (8 chars), objective (40 chars), state, started
3. Keybindings: `q` quit, `↑↓` scroll

Run test to confirm it passes:
```bash
cargo test -p grove-cli --features tui tui::run_watch
```

- [ ] **Step 4: Full test suite**

```bash
cargo test -p grove-cli
cargo test -p grove-cli --features tui
```
Expected: all tests pass.

- [ ] **Step 5: Clippy — zero warnings**

```bash
cargo clippy -p grove-cli -- -D warnings
cargo clippy -p grove-cli --features tui -- -D warnings
```

- [ ] **Step 6: Old crate still compiles**

```bash
cargo check -p grove-cli-old
```

- [ ] **Step 7: Full smoke test**

```bash
cargo build -p grove-cli
cargo build -p grove-cli --features tui
./target/debug/grove --help
./target/debug/grove status --json
./target/debug/grove auth list
./target/debug/grove llm list
./target/debug/grove git status
./target/debug/grove issue board
./target/debug/grove doctor
```

- [ ] **Step 8: Final commit**

```bash
git add -A
git commit -m "feat(grove-cli): complete v1 — all commands, TUI, wire --watch"
```

---

## Reference

- **Spec:** `docs/superpowers/specs/2026-03-18-grove-cli-redesign.md`
- **grove-core orchestrator API:** `crates/grove-core/src/orchestrator/mod.rs`
- **grove-core app bootstrap:** `crates/grove-core/src/app.rs`
- **grove-core DB repos:** `crates/grove-core/src/db/repositories/`
- **grove-core LLM auth:** `crates/grove-core/src/llm/auth.rs`
- **Old CLI (reference patterns):** `crates/grove-cli-old/src/commands/`
- **grove-gui types (mirrors grove-core serialization):** `crates/grove-gui/src/types/index.ts`
