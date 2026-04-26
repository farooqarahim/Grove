# Grove

**Run parallel AI coding agents on your machine. Describe what you want, Grove handles the rest.**

You give Grove an objective — `"add pagination to the API"` or `"fix the open GitHub issues marked ready"` — and it plans the work, spawns isolated agent sessions in git worktrees, merges their output, and opens a pull request. All state lives in a local SQLite database. Nothing leaves your machine.

```bash
grove run "add input validation to the signup form"
```

Grove plans the work, spins up agents in isolated worktrees, reviews their output, merges the branches, and creates a PR. You watch from the terminal or the desktop GUI.

---

## How it works

```
                        grove run "your objective"
                                  |
                    +-------------+-------------+
                    |                           |
               Plan Mode                   Build Mode
            PRD -> Design              Builder -> Reviewer -> Judge
                    |                           |
                    +-------------+-------------+
                                  |
                    Spawn agents in git worktrees
                    (your working tree is never touched)
                                  |
                    Merge branches in order
                    Detect & resolve conflicts
                                  |
                    Create PR or push branch
```

1. You run `grove run "your objective"` (or start a run from the GUI)
2. Grove plans the work using a configurable pipeline of agent roles
3. Each agent gets its own isolated git worktree — they can't interfere with each other or your working tree
4. When agents finish, Grove merges their branches in order, detects conflicts, and either resolves them automatically or flags them for you
5. On success, Grove creates a pull request or pushes the branch directly

See [docs/concepts.md](docs/concepts.md) for a deeper walkthrough of pipelines, graphs, and worktrees.

---

## Quick start

```bash
# Install from source
git clone https://github.com/farooqarahim/Grove.git
cd Grove
cargo install --path crates/grove-cli

# Verify everything is set up
grove doctor

# Go to any git repo
cd ~/my-project

# Initialize Grove
grove init

# Set your API key
grove auth set anthropic sk-ant-...

# Run your first objective
grove run "add input validation to the signup form"

# Watch progress in real-time
grove status --watch

# View the full report
grove report <run-id>
```

Full walkthrough: [docs/getting-started.md](docs/getting-started.md).

---

## Features

| Feature | Description |
|---|---|
| **Worktree isolation** | Each agent runs in its own git worktree. Your working directory is never touched. |
| **Pipeline orchestration** | Configurable agent pipelines: PRD writer, system designer, builder, reviewer, judge |
| **Graph orchestration** | DAG-based execution for complex, multi-phase projects with dependency tracking |
| **15+ agent backends** | Claude Code, Gemini, Codex, Aider, Cursor, Copilot, Goose, Cline, Kiro, and more |
| **Issue tracker integration** | GitHub Issues, Jira, Linear. `grove fix PROJ-123` to auto-fix issues. |
| **Merge queue** | Automatic merge ordering with conflict detection and configurable resolution |
| **Crash recovery** | Checkpoint every stage transition. `grove resume <run-id>` picks up where you left off. |
| **CI integration** | `grove ci --fix` watches CI and spawns agents to fix failures |
| **Budget controls** | Per-run USD budget with real-time enforcement and warning thresholds |
| **MCP server** | Expose Grove as MCP tools for agents to self-coordinate |
| **Desktop GUI** | Tauri-based native app with full feature parity |
| **Conversations** | Persistent threads linking multiple runs on the same feature branch |
| **Daemon mode** | Optional long-lived background daemon keeps the orchestrator warm across CLI commands |

---

## Documentation

| Document | Contents |
|---|---|
| [Getting Started](docs/getting-started.md) | Installation, first run, understanding output |
| [Concepts](docs/concepts.md) | Runs, sessions, pipelines, graphs, worktrees, conversations, merge queue, budget |
| [CLI Reference](docs/cli-reference.md) | Every command and flag |
| [Configuration](docs/configuration.md) | Full `grove.yaml` reference, merge strategies, budget controls, env vars |
| [Integrations](docs/integrations.md) | LLM providers, coding agent backends, issue trackers, MCP server, CI |
| [Workflows](docs/workflows.md) | End-to-end recipes: bug fixes, features, parallel issues, conversations, CI, budgets |
| [Desktop GUI](docs/gui.md) | Run vs Graph mode, Agent Studio, terminal, git integration |
| [Daemon](docs/daemon.md) | Optional background daemon — lifecycle, file locations, troubleshooting |
| [Troubleshooting](docs/troubleshooting.md) | Common errors, crash recovery, merge conflicts, debugging |
| [Releasing](docs/RELEASING.md) | Release process for maintainers |

---

## Project structure

```
Grove/
├── crates/
│   ├── grove-core/        # Core orchestration engine
│   ├── grove-cli/         # CLI binary (`grove`)
│   ├── grove-daemon/      # Optional long-lived background daemon
│   ├── grove-gui/         # Tauri desktop application
│   ├── grove-mcp-server/  # MCP protocol server
│   ├── grove-filter/      # Token filtering & budget tracking
│   └── grove-db-lookup/   # Database explorer (dev tool)
├── docs/                  # Documentation
├── skills/                # Agent role definitions
├── templates/             # Config templates
└── scripts/               # Dev scripts
```

---

## Prerequisites

| Dependency | Version | Notes |
|---|---|---|
| [Rust](https://rustup.rs/) | 1.85+ | Managed via `rust-toolchain.toml` |
| Git | 2.30+ | Required for worktree support |
| [Node.js](https://nodejs.org/) | 18+ | GUI only |
| A coding agent CLI | latest | Claude Code (default), Codex, Gemini, etc. |

---

## Development

```bash
# Format
cargo fmt

# Lint
cargo clippy

# Test
cargo test

# Run all checks
./scripts/dev.sh --check

# Launch GUI in dev mode
./scripts/dev.sh
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contribution guide.

---

## License

Apache License 2.0 — see [LICENSE](LICENSE).
