# Grove CLI Redesign — Spec

**Date:** 2026-03-18
**Status:** Approved
**Scope:** v1

---

## 1. Overview

Replace the existing `grove-cli` crate with a new, ground-up implementation that has full parity with the grove-gui for core workflows. The old crate is archived as `grove-cli-old` (no code changes). grove-gui and grove-cli are co-equal interfaces to the same grove-core backend — neither replaces the other.

---

## 2. Goals

- Full CLI parity with grove-gui for: runs/sessions, conversations, git, auth/llm, workspace/project, and issues.
- Rich terminal output: colored tables, spinners, progress indicators (`--json` flag for machine-readable output).
- TUI mode (`feature = "tui"`): live run-watch view (`grove run --watch`, `grove status --watch`) and a full dashboard (`grove tui`).
- Hybrid transport: direct grove-core linkage for reads; Unix socket to grove-server for mutations and streaming.
- Lean default binary (no TUI deps); full binary with `--features tui` for dev workstations.
- No budget features — removed entirely.

---

## 3. Non-Goals (v1)

- Grove Graph commands (`grove graph …`)
- Automations commands (`grove automation …`)
- HiveLoom conversation management
- AI code review (`grove review`)
- Agent Studio / catalog management

---

## 4. Migration

### 4.1 Rename old crate

```toml
# crates/grove-cli-old/Cargo.toml
[package]
name = "grove-cli-old"   # was "grove-cli" — only change
```

No code changes to `grove-cli-old`. It is preserved as an archive.

### 4.2 Workspace update

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
  "crates/grove-cli-old",
  "crates/grove-cli",
  # … existing members unchanged
]
```

`default-members` switches to `grove-cli`.

---

## 5. Crate Structure

```
crates/grove-cli/
  Cargo.toml
  src/
    main.rs
    cli.rs            # clap structs: Commands enum + all Args
    error.rs          # CliError, exit codes
    output/
      mod.rs
      text.rs         # colored tables, spinners (console + indicatif + tabled)
      json.rs         # --json serializer
    transport/
      mod.rs          # Transport trait + GroveTransport enum
      direct.rs       # grove-core direct calls (reads)
      socket.rs       # Unix socket client (mutations + streaming)
    commands/
      run.rs          # run, queue, tasks, task-cancel, resume, abort
      status.rs       # status, logs, report, plan, subtasks, sessions
      git.rs          # git subcommands
      issues.rs       # issue subcommands + fix + connect
      auth.rs         # auth set/remove/list
      llm.rs          # llm list/models/select
      workspace.rs    # workspace subcommands
      project.rs      # project subcommands
      conversation.rs # conversation subcommands
      doctor.rs       # doctor
      init.rs         # init
      hooks.rs        # hook (internal plumbing)
      signals.rs      # signal send/check/list
      worktrees.rs    # worktrees list/clean/delete
      cleanup.rs      # cleanup, gc
    tui/              # compiled only with feature = "tui"
      mod.rs
      dashboard.rs    # grove tui — full three-pane dashboard
      run_watch.rs    # grove run --watch / grove status --watch
      widgets/        # reusable ratatui widgets
```

---

## 6. Command Surface

Global flags on every command:
```
--project <path>    working directory (default: .)
--json              machine-readable output to stdout
--verbose           extra logging
--no-color          disable ANSI colors
```

### 6.1 Bootstrap

```
grove init
grove doctor [--fix] [--fix-all]
```

### 6.2 Runs

```
grove run <objective>
  --max-agents <n>
  --model <id>
  --pipeline <name>
  --permission-mode skip_all|human_gate|autonomous_gate
  --conversation <id>
  --continue-last / -c
  --issue <id>
  --watch                  # live TUI (feature=tui)

grove queue <objective>
  --priority <n>
  --model <id>
  --conversation <id>
  --continue-last / -c

grove tasks [--limit n] [--refresh]
grove task-cancel <id>
grove status [--limit n] [--watch] [--json]
grove resume <run-id>
grove abort <run-id>
grove logs <run-id> [--all] [--json]
grove report <run-id> [--json]
grove plan [run-id] [--json]
grove subtasks [run-id] [--json]
grove sessions <run-id> [--json]
```

### 6.3 Git

```
grove git status [--json]
grove git stage [paths…]
grove git unstage [paths…]
grove git revert [paths…] [--all]
grove git commit [-m msg] [-a] [--push]
grove git push
grove git pull
grove git branch
grove git log [-n n]
grove git undo
grove git pr [--title] [--body] [--base] [--push]
grove git pr-status [--json]
grove git merge [--strategy squash|merge|rebase] [--admin]
```

### 6.4 Issues

```
grove issue list [--cached] [--json]
grove issue show <id>
grove issue create <title> [--body] [--labels] [--priority]
grove issue close <id>
grove issue update <id> [--title] [--status] [--label] [--assignee] [--priority]
grove issue comment <id> <body>
grove issue assign <id> <assignee>
grove issue move <id> <status>
grove issue reopen <id>
grove issue search <query> [--limit] [--provider]
grove issue sync [--provider] [--full]
grove issue board [--status] [--provider] [--assignee] [--priority] [--json]
grove issue board-config show|set --file <path>|reset
grove issue activity <id>
grove issue ready
grove issue push <id> --to <provider>

grove fix [issue-id] [--prompt] [--ready] [--max] [--parallel]

grove connect github [--token]
grove connect jira --site --email --token
grove connect linear --token
grove connect status
grove connect disconnect <provider>
```

### 6.5 Auth & LLM

```
grove auth set <provider> <api-key>
grove auth remove <provider>
grove auth list

grove llm list
grove llm models <provider>
grove llm select <provider> [model] [--own-key | --workspace-credits]
```

### 6.6 Workspace & Project

```
grove workspace show
grove workspace set-name <name>
grove workspace archive <id>
grove workspace delete <id>

grove project show
grove project list
grove project open-folder <path> [--name]
grove project clone <repo> <path> [--name]
grove project create-repo <repo> <path> [--provider] [--visibility] [--gitignore]
grove project fork-repo <src> <target> <repo> [--provider]
grove project fork-folder <src> <target> [--preserve-git]
grove project ssh <host> <remote-path> [--user] [--port]
grove project ssh-shell [id]
grove project set-name <name>
grove project set [--provider] [--parallel] [--pipeline] [--permission-mode] [--reset]
grove project archive [id]
grove project delete [id]
```

### 6.7 Conversations

```
grove conversation list [--limit] [--json]
grove conversation show <id> [--limit]
grove conversation archive <id>
grove conversation delete <id>
grove conversation rebase <id>
grove conversation merge <id>
```

### 6.8 Plumbing

```
grove signal send <run-id> <from> <to> <type> [--payload] [--priority]
grove signal check <run-id> <agent>
grove signal list <run-id>

grove hook <event> <agent-type> [--run-id] [--session-id] [--tool] [--file-path]

grove worktrees [--clean] [--delete id] [--delete-all] [-y]
grove cleanup [--project] [--conversation] [--dry-run] [-y] [--force]
grove gc [--dry-run]

grove ownership [run-id]
grove conflicts [--show path] [--resolve path]
grove merge-status <conversation-id>
grove publish retry <run-id>
grove lint [--fix] [--model]
grove ci [branch] [--wait] [--timeout] [--fix] [--model]
```

### 6.9 TUI (feature = "tui" only)

```
grove tui     # full three-pane dashboard: sidebar + main + git panel
              # screens: [1] dashboard  [2] sessions  [3] issues  [4] settings
              # keybindings: N new run, F fix issue, C commit, P push, q quit
```

---

## 7. Output Design

### 7.1 Rich text (default)

Dependencies: `console` (ANSI), `indicatif` (spinners/progress), `tabled` (tables).

- Color palette mirrors grove-gui: accent `#31B97B`, muted grays for secondary, red for errors.
- `--no-color` strips all ANSI codes.
- `--json` sends structured JSON to stdout; human text goes to stderr.
- Errors in `--json` mode: `{ "error": "…", "code": <exit_code> }`.

### 7.2 TUI — run watch

Built on `ratatui` + `crossterm`. Layout:
- Header: run ID, objective, pipeline
- Agent table: name, state, started, duration
- Log pane: scrollable live output for selected agent
- Keybindings: `q` quit, `Tab` cycle agents, `↑↓` scroll, `a` abort

### 7.3 TUI — full dashboard (`grove tui`)

Three-pane layout:
- **Left sidebar:** project list + conversation list
- **Main panel:** run list for selected conversation, new-run / fix-issue actions
- **Right panel:** git status, changed files, branch info, commit/push actions
- **Nav bar:** `[1]` Dashboard `[2]` Sessions `[3]` Issues `[4]` Settings

Keybindings match grove-gui: `Cmd/Ctrl+1-4` switch screens, `N` new run, `F` fix issue, `C` commit, `P` push, `q` quit.

---

## 8. Transport Architecture

### 8.1 Transport trait

```rust
pub trait Transport: Send + Sync {
    fn runs(&self, limit: i64) -> Result<Vec<RunRecord>>;
    fn start_run(&self, req: StartRunRequest) -> Result<RunStream>;
    fn abort_run(&self, run_id: &str) -> Result<()>;
    fn list_issues(&self, opts: IssueListOpts) -> Result<Vec<Issue>>;
    // one method per grove-core operation
}

pub enum GroveTransport {
    Direct(DirectTransport),   // grove-core in-process (reads)
    Socket(SocketTransport),   // Unix socket → grove-server (mutations + streaming)
}
```

### 8.2 Read vs mutation split

| Category | Transport | Examples |
|---|---|---|
| Reads | DirectTransport | list_runs, get_report, list_issues, git status |
| Mutations | SocketTransport | start_run, abort_run, queue_task, git commit |
| Streaming | SocketTransport | run --watch, status --watch, live log tail |

### 8.3 Auto-detection

At startup the CLI checks for a socket at `~/.grove/grove.sock` or `.grove/grove.sock`. If found, `SocketTransport` handles mutations. If not found, `DirectTransport` handles everything (single-user local mode).

### 8.4 Unix socket protocol

Newline-delimited JSON over a Unix domain socket:

```jsonc
// Request
{ "id": "r1", "method": "start_run", "params": { "objective": "…", "pipeline": "standard" } }

// Streaming response events
{ "id": "r1", "event": "agent_started",  "payload": { "agent": "architect" } }
{ "id": "r1", "event": "agent_log",      "payload": { "agent": "builder-1", "line": "…" } }
{ "id": "r1", "event": "run_completed",  "payload": { "state": "completed" } }
```

---

## 9. Feature Flags

```toml
[features]
default = []
tui = ["dep:ratatui", "dep:crossterm"]

[dependencies]
grove-core  = { path = "../grove-core" }
clap        = { features = ["derive"] }
console     = "0.15"
indicatif   = "0.17"
tabled      = "0.15"
serde_json  = "1"
tokio       = { features = ["full"] }
thiserror   = "1"
ratatui     = { version = "0.28", optional = true }
crossterm   = { version = "0.28", optional = true }
```

- **CI/server install:** `cargo install grove-cli` — lean binary, no TUI deps
- **Dev install:** `cargo install grove-cli --features tui` — full binary with dashboard

---

## 10. Error Handling

```rust
#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error("grove-core: {0}")]       Core(#[from] grove_core::Error),
    #[error("transport: {0}")]        Transport(String),
    #[error("not found: {0}")]        NotFound(String),
    #[error("invalid argument: {0}")] BadArg(String),
    #[error("{0}")]                   Other(String),
}
```

Exit codes:
- `0` — success
- `1` — general error
- `2` — bad arguments
- `3` — not found
- `4` — transport / connection error

---

## 11. Testing Strategy

| Layer | What | Tool |
|---|---|---|
| Unit | output renderers, arg parsing, transport trait | `cargo test` |
| Integration | each command against a test grove-core instance | `cargo test` + tmpdir fixtures |
| Contract | JSON output shape doesn't regress | snapshot tests (`tests/contract/`) |
| E2E | `grove run`, `grove issue board`, `grove git commit` full flows | `tests/e2e/` |
