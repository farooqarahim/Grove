# Grove

**Grove is a local, single-user orchestration engine for running parallel AI coding agent sessions on your own machine.**

You give Grove an objective — *"add pagination to the API"* or *"fix the open GitHub issues marked ready"* — and it automatically plans the work, spawns isolated agent sessions in git worktrees, merges their output, and opens a pull request. All state lives in a local SQLite database. Nothing is sent to a remote server.

---

## How it works

1. **You run `grove run "your objective"`**
2. Grove plans the work using a configurable pipeline of agent roles: PRD writer → system designer → builder(s) → reviewer → judge
3. Each agent gets its own isolated git worktree so they can't interfere with each other or your working tree
4. When the agents finish, Grove merges their branches in order, detects conflicts, and either resolves them automatically or flags them for you
5. On success, Grove creates a pull request or pushes the branch directly, depending on your configuration

---

## Features

| Feature | Description |
|---|---|
| **Worktree isolation** | Each agent session runs in a dedicated git worktree. Your working directory is never touched. |
| **Graph orchestration** | DAG-based agentic loops for complex, multi-phase projects with dependency tracking |
| **Issue tracker integration** | Connect to GitHub Issues, Jira, or Linear. Use `grove fix` to automatically work issues. |
| **Multi-agent support** | Claude Code (default), Gemini, Codex, Aider, Cursor — all configurable |
| **Merge queue** | Automatic merge ordering with conflict detection and configurable resolution strategies |
| **Crash recovery** | Checkpoint every stage transition. Resume any interrupted run with `grove resume <run-id>`. |
| **CI integration** | `grove ci --fix` watches CI and spawns agents to fix failures |
| **MCP server** | Expose Grove's orchestration as MCP tools for agents to self-coordinate |
| **Desktop GUI** | Tauri-based native desktop application |
| **Audit log** | Append-only SQLite event log of everything that happens in every run |

---

## Prerequisites

| Dependency | Minimum version | Notes |
|---|---|---|
| [Rust](https://rustup.rs/) | 1.85 | Managed via `rust-toolchain.toml` |
| Git | 2.30 | Required for worktree support |
| [Node.js](https://nodejs.org/) | 18 | GUI only — not needed for CLI-only use |
| [Claude Code](https://claude.ai/code) (`claude` CLI) | latest | Default agent provider |

---

## Installation

### From source

```bash
git clone https://github.com/farooqarahim/Grove.git
cd grove

# Verify toolchain and dependencies
./scripts/bootstrap.sh

# Build and install the CLI
cargo install --path crates/grove-cli
```

### Verify installation

```bash
grove --version
grove doctor
```

`grove doctor` runs a full preflight check and reports any missing dependencies or configuration problems.

---

## Quick start

```bash
# 1. Go to any git repository
cd ~/my-project

# 2. Initialize Grove
grove init

# 3. Set your Anthropic API key
grove auth set anthropic sk-ant-...

# 4. Run your first objective
grove run "add input validation to the signup form"

# 5. Watch progress
grove status

# 6. View the full report when done
grove report <run-id>
```

---

## Core concepts

| Concept | What it is |
|---|---|
| **Run** | A single execution of an objective through the pipeline |
| **Session** | One agent instance within a run, assigned to a worktree |
| **Worktree** | An isolated git worktree where a session does its work |
| **Conversation** | A persistent thread linking multiple runs on the same feature branch |
| **Pipeline** | The sequence of agent roles a run executes through (named presets coming soon) |
| **Graph** | A DAG-based execution plan for complex multi-phase work |

See [docs/concepts.md](docs/concepts.md) for a full explanation of each concept.

---

## Documentation

| Document | Contents |
|---|---|
| [Getting Started](docs/getting-started.md) | Installation, first run, understanding output |
| [Concepts](docs/concepts.md) | Runs, sessions, pipelines, graphs, worktrees, budget |
| [CLI Reference](docs/cli-reference.md) | Every command and flag |
| [Configuration](docs/configuration.md) | Full `grove.yaml` reference |
| [Integrations](docs/integrations.md) | LLM providers, GitHub, Jira, Linear |

---

## Connecting issue trackers

```bash
# GitHub (uses existing `gh` auth)
grove connect github

# Jira
grove connect jira --site https://myco.atlassian.net --email me@myco.com --token <token>

# Linear
grove connect linear --token <token>

# Fix a specific issue
grove fix PROJ-123

# Fix all issues marked "ready"
grove fix --ready
```

---

## Development

```bash
# Launch GUI in dev mode with hot reload
./scripts/dev.sh

# Run all checks (clippy + tests + tsc)
./scripts/dev.sh --check

# Format code
cargo fmt

# Lint
cargo clippy

# Run tests
cargo test
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contribution guide.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE).
