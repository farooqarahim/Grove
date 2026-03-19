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

1. **You run `grove run "your objective"`** (or start a run from the GUI)
2. Grove plans the work using a configurable pipeline of agent roles
3. Each agent gets its own isolated git worktree — they can't interfere with each other or your working tree
4. When agents finish, Grove merges their branches in order, detects conflicts, and either resolves them automatically or flags them for you
5. On success, Grove creates a pull request or pushes the branch directly

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
| **Audit log** | Append-only SQLite event log for every run |
| **Conversations** | Persistent threads linking multiple runs on the same feature branch |

---

## Pipelines

Grove has three pipeline modes that control which agents run and in what order:

| Mode | Agents | Use case |
|---|---|---|
| **Autonomous** (default) | PRD Writer → System Designer → Builder → Reviewer → Judge | Full end-to-end — describe a feature, get a PR |
| **Plan** | PRD Writer → System Designer | Requirements + design only, no code changes |
| **Build** | Builder → Reviewer → Judge | Skip planning — use when you already have a plan |

```bash
# Full autonomous pipeline (default)
grove run "add dark mode support"

# Plan only — generate requirements and design docs
grove run "add dark mode support" --pipeline plan

# Build only — you already know what to build
grove run "add dark mode support" --pipeline build
```

---

## Graph orchestration

For complex objectives that span multiple phases, Grove decomposes work into a **DAG (directed acyclic graph)** of phases and steps.

```
              Graph: "Build authentication system"
              ┌──────────────────────────────────┐
              │  Phase 1: Foundation             │
              │  ├── Step 1: DB schema           │
              │  ├── Step 2: User model          │
              │  └── Step 3: Config setup        │ ← parallel execution
              ├──────────────────────────────────┤
              │  Phase 2: Implementation         │
              │  ├── Step 1: Login endpoint      │
              │  ├── Step 2: Signup endpoint      │
              │  └── Step 3: Token refresh       │ ← parallel execution
              ├──────────────────────────────────┤
              │  Phase 3: Hardening              │
              │  ├── Step 1: Rate limiting       │
              │  └── Step 2: Input validation    │
              └──────────────────────────────────┘
                           │
                   Phase Validator
                   (integration checks)
                           │
                      Phase Judge
                   (holistic grading)
```

**How it works:**

1. A **PrePlanner** generates any missing requirements or design docs
2. A **GraphCreator** decomposes the objective into phases and steps via MCP
3. Steps within a phase run **in parallel** — each in its own worktree with its own agent
4. A **PhaseValidator** runs read-only integration checks after each phase completes
5. A **PhaseJudge** grades the phase holistically before the next phase begins
6. Phases execute sequentially — each phase depends on the previous one

Graph mode is available from the **desktop GUI** where you can visualize the DAG, monitor step progress, and inspect results per-phase.

---

## Supported agents

Grove is agent-agnostic. Use whichever coding agent you prefer — or mix them within a single run.

| Agent | CLI | Auto-approve | Model selection |
|---|---|---|---|
| **Claude Code** (default) | `claude` | `--dangerously-skip-permissions` | Opus, Sonnet, Haiku |
| **Codex** (OpenAI) | `codex` | `--full-auto` | O4-mini, O3, GPT-4.1 |
| **Gemini** (Google) | `gemini` | `--yolo` | 2.5 Pro, 2.5 Flash, 2.0 Flash |
| **Aider** | `aider` | `--yes` | Claude Sonnet, GPT-4o, Gemini |
| **Cursor** | `cursor-agent` | `-f` | Agent-managed |
| **GitHub Copilot** | `copilot` | `--allow-all-tools` | Agent-managed |
| **Qwen Code** | `qwen` | `--yolo` | Qwen3 Coder |
| **Kimi** | `kimi` | `--yolo` | Kimi K2 |
| **Amp** | `amp` | — | Agent-managed |
| **Goose** | `goose` | — | Agent-managed |
| **Cline** | `cline` | `--yolo` | Agent-managed |
| **Continue** | `cn` | — | Agent-managed |
| **Kiro** (AWS) | `kiro-cli` | — | Agent-managed |
| **Auggie** | `auggie` | — | Agent-managed |
| **Kilocode** | `kilocode` | `--auto` | Agent-managed |

Switch providers per-project:

```bash
grove project set --provider gemini
grove project set --provider codex
```

---

## Issue tracker integration

Connect your issue tracker and let Grove fix issues automatically.

```bash
# GitHub (uses existing `gh` auth)
grove connect github

# Jira
grove connect jira --site https://myco.atlassian.net --email me@myco.com --token <token>

# Linear
grove connect linear --token <token>

# Check connection status
grove connect status

# Fix a specific issue
grove fix PROJ-123

# Fix all issues marked "ready"
grove fix --ready

# Fix up to 5 issues in parallel
grove fix --ready --max 5 --parallel

# Browse issues
grove issue list
grove issue board
grove issue search "login bug"
```

---

## CI integration

Grove watches your CI pipeline and auto-fixes failures.

```bash
# Watch CI on current branch and fix failures
grove ci --fix

# Watch a specific branch
grove ci main --fix

# Wait for CI to finish before fixing
grove ci --wait --fix

# Set a timeout (seconds)
grove ci --wait --timeout 600 --fix
```

---

## Conversations

Conversations let you iterate on a feature across multiple runs. Each run in a conversation builds on the previous one's branch — so you can refine, extend, and fix without starting over.

```bash
# First run — start a new feature
grove run "add user authentication with JWT"

# Continue the same conversation — Grove picks up where the last run left off
grove run "add refresh token rotation" -c

# Or reference a specific conversation
grove run "add rate limiting to auth endpoints" --conversation conv_abc123

# List all conversations
grove conversation list

# See the runs in a conversation
grove conversation show conv_abc123

# Merge a completed conversation into your main branch
grove conversation merge conv_abc123

# Rebase a conversation branch onto latest main
grove conversation rebase conv_abc123
```

Each conversation tracks its own feature branch. When you're done iterating, `grove conversation merge` brings everything into your main branch through the merge queue.

---

## Configuration

Grove creates `.grove/grove.yaml` in your project on `grove init`. Key options:

```yaml
# Provider & model
providers:
  default: "claude_code"
  claude_code:
    enabled: true
    permission_mode: "skip_all"

# Budget per run (USD)
budgets:
  default_run_usd: 5.0
  warning_threshold_percent: 80

# Pipeline mode
orchestration:
  enforce_design_first: true
  enable_retries: true
  max_retries_per_session: 2

# Worktree settings
worktree:
  root: ".grove/worktrees"
  branch_prefix: "grove"
  cleanup_on_success: false

# Auto-publish
publish:
  enabled: true
  target: "github"
  auto_on_success: true

# Merge strategy
merge:
  strategy: "last_writer_wins"
  conflict_strategy: "markers"
  lockfile_strategy: "regenerate"

# Runtime limits
runtime:
  max_agents: 3
  max_run_minutes: 60
  max_concurrent_runs: 4
```

Environment variable overrides:

```bash
GROVE_PROVIDER=gemini grove run "fix the bug"
GROVE_BUDGET_USD=10 grove run "refactor auth module"
GROVE_MAX_AGENTS=5 grove run "implement feature"
```

---

## CLI reference

### Core workflow

| Command | Description |
|---|---|
| `grove init` | Initialize Grove in a project |
| `grove doctor` | Preflight health check (use `--fix` to auto-repair) |
| `grove run "objective"` | Execute an objective with agents |
| `grove queue "objective"` | Queue an objective for later execution |
| `grove status` | Show recent runs (use `--watch` for live TUI) |
| `grove resume <run-id>` | Resume an interrupted run |
| `grove abort <run-id>` | Stop a running execution |
| `grove report <run-id>` | View detailed run report |
| `grove logs <run-id>` | Stream agent output logs |
| `grove sessions <run-id>` | List sessions within a run |
| `grove plan [run-id]` | View the plan for a run |
| `grove subtasks [run-id]` | View subtasks for a run |

### Issues & CI

| Command | Description |
|---|---|
| `grove fix [issue-id]` | Auto-fix an issue or all ready issues |
| `grove issue list` | List issues from connected trackers |
| `grove issue board` | Kanban board view across providers |
| `grove issue search "query"` | Search issues |
| `grove issue create "title"` | Create a new issue |
| `grove issue sync` | Sync issues from remote trackers |
| `grove connect github` | Connect GitHub Issues |
| `grove connect jira` | Connect Jira |
| `grove connect linear` | Connect Linear |
| `grove ci --fix` | Watch CI and fix failures |

### Git operations

| Command | Description |
|---|---|
| `grove git status` | Show git status |
| `grove git commit -m "msg"` | Commit changes |
| `grove git push` | Push to remote |
| `grove git pr --title "PR title"` | Create a pull request |
| `grove git pr-status` | Check PR status |
| `grove git merge` | Merge current PR |
| `grove git log -n 20` | View recent commits |
| `grove git undo` | Undo last operation |

### Conversations

| Command | Description |
|---|---|
| `grove conversation list` | List feature branch conversations |
| `grove conversation show <id>` | Show runs in a conversation |
| `grove conversation merge <id>` | Merge a conversation branch |
| `grove conversation rebase <id>` | Rebase a conversation branch |
| `grove run "objective" -c` | Continue the last conversation |

### Project & workspace

| Command | Description |
|---|---|
| `grove project show` | Show current project info |
| `grove project list` | List all registered projects |
| `grove project set --provider gemini` | Set project defaults |
| `grove workspace show` | Show workspace info |
| `grove auth set <provider> <key>` | Store an API key |
| `grove auth list` | List configured providers |
| `grove llm list` | List available providers |
| `grove llm models <provider>` | List models for a provider |

### Maintenance

| Command | Description |
|---|---|
| `grove worktrees` | List active worktrees |
| `grove worktrees --clean` | Clean up stale worktrees |
| `grove cleanup` | Clean old runs and sessions |
| `grove gc` | Garbage collection |
| `grove lint` | Run code quality checks |
| `grove ownership [run-id]` | View file ownership locks |
| `grove conflicts --show <id>` | View merge conflicts |
| `grove merge-status <conv-id>` | Check merge queue status |

### Global flags

```bash
grove --json <command>       # JSON output
grove --verbose <command>    # Verbose logging
grove --no-color <command>   # Disable color output
grove --project <path> <cmd> # Target a different project
```

---

## Desktop GUI

Grove ships a Tauri-based native desktop app. Everything you can do from the CLI, you can do from the GUI — plus visual features that don't translate to a terminal.

```bash
# Launch in dev mode with hot reload
./scripts/dev.sh

# Or build for production
cd crates/grove-gui && npm run tauri build
```

### Two execution modes

The GUI exposes both execution modes side by side:

| Mode | What it does | Best for |
|---|---|---|
| **Run** | Pipeline execution (PRD → Design → Build → Review → Judge) | Single-objective tasks, bug fixes, features |
| **Graph** | DAG-based multi-phase execution with parallel steps | Complex projects, large refactors, multi-system changes |

In **Run mode**, you type an objective and watch agents execute through the pipeline with live output streaming.

In **Graph mode**, you see the full DAG — phases, steps, dependencies — and can monitor parallel agents executing steps within each phase, inspect per-step results, and review phase validation outcomes.

### GUI features

- **Run dashboard** — live agent output, session logs, cost tracking
- **Graph visualizer** — DAG view of phases and steps with real-time progress
- **Issue board** — unified kanban across GitHub, Jira, and Linear
- **Agent studio** — customize which agent and model each role uses
- **Terminal emulator** — full PTY support for interactive agent sessions
- **Git integration** — diff viewer, file explorer, PR creation
- **Automation rules** — configure CI triggers, webhooks, and scheduled runs
- **Project management** — register projects, clone repos, manage workspaces

---

## Core concepts

| Concept | What it is |
|---|---|
| **Workspace** | Top-level identity for your machine. Holds global credentials and project references. |
| **Project** | A local git repository registered with Grove. |
| **Run** | A single execution of an objective through the pipeline. |
| **Session** | One agent instance within a run, assigned to its own worktree. |
| **Worktree** | An isolated git worktree where a session does its work. |
| **Conversation** | A persistent thread linking multiple runs on the same feature branch. |
| **Pipeline** | The sequence of agent roles a run passes through (plan, build, or autonomous). |
| **Graph** | A DAG-based execution plan for complex multi-phase work with dependency tracking. |

See [docs/concepts.md](docs/concepts.md) for details.

---

## Merge strategies

When agents finish, Grove merges their branches in order. Configure how conflicts are handled:

| Strategy | Behavior |
|---|---|
| `last_writer_wins` | Last agent's changes take precedence |
| `first_writer_wins` | First agent's changes take precedence |
| `markers` | Leave conflict markers for manual resolution |
| `ours` / `theirs` | Standard git merge strategies |
| `abort` | Abort merge on any conflict |

Lockfile handling is automatic — Grove regenerates `Cargo.lock`, `package-lock.json`, `yarn.lock`, and `poetry.lock` after merges.

---

## Budget controls

Set per-run spending limits to avoid surprise costs.

```yaml
# grove.yaml
budgets:
  default_run_usd: 5.0
  warning_threshold_percent: 80
  hard_stop_percent: 100
```

- Grove meters cost after each agent response
- Warns at 80% of budget
- Hard stops at 100%
- Override per-run: `GROVE_BUDGET_USD=10 grove run "..."`

---

## MCP server

Expose Grove's orchestration as MCP tools so agents can self-coordinate.

```bash
# Start the MCP server
cargo run --bin grove-mcp-server
```

This allows agents within a run to invoke Grove tools — decomposing work into sub-steps, checking other agents' progress, and coordinating complex multi-phase execution.

---

## Project structure

```
Grove/
├── crates/
│   ├── grove-core/        # Core orchestration engine
│   ├── grove-cli/         # CLI binary (`grove`)
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

## Documentation

| Document | Contents |
|---|---|
| [Getting Started](docs/getting-started.md) | Installation, first run, understanding output |
| [Concepts](docs/concepts.md) | Runs, sessions, pipelines, graphs, worktrees, budget |
| [CLI Reference](docs/cli-reference.md) | Every command and flag |
| [Configuration](docs/configuration.md) | Full `grove.yaml` reference |
| [Integrations](docs/integrations.md) | LLM providers, issue trackers, MCP server, CI |
| [Desktop GUI](docs/gui.md) | Screens, Run vs Graph mode, Agent Studio, terminal, git integration |
| [Workflows](docs/workflows.md) | End-to-end recipes: fix bugs, build features, parallel issues, CI, budgets |
| [Troubleshooting](docs/troubleshooting.md) | Common errors, crash recovery, merge conflicts, debugging |

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
