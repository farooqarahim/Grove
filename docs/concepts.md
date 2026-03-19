# Core Concepts

This document explains the key concepts in Grove. Understanding these will help you use Grove effectively and interpret its output.

---

## Workspace

A **workspace** is the top-level Grove identity for your machine. It is created automatically on first use and holds:

- Global LLM provider selection and credentials
- References to all registered projects

You typically have one workspace per machine. Manage it with `grove workspace show` and `grove workspace set-name`.

---

## Project

A **project** is a local directory registered with Grove. Most commonly it is a git repository, but Grove also supports SSH projects (remote shell access) and plain folder projects.

Each project stores:
- A display name
- The path on disk
- Default settings (pipeline, provider, permission mode, parallel agent limit)
- Connections to external issue trackers

Initialize a project in a directory with `grove init`, or register an existing directory with `grove project open-folder <path>`.

### Project sources

Grove supports several ways to create projects:

```bash
grove project open-folder <path>              # register an existing local directory
grove project clone <repo-url> <path>         # clone a git repository
grove project create-repo <repo> <path>       # create a new repository
grove project fork-repo <src> <target> <repo> # fork a repo to a remote
grove project fork-folder <src> <target>      # copy a local folder
grove project ssh <host> <remote-path>        # connect to a remote machine via SSH
```

---

## Conversation

A **conversation** is a persistent thread that groups related runs. Think of it like a GitHub branch + PR discussion thread combined.

When you run `grove run`, Grove either creates a new conversation or adds the run to an existing one (if you pass `--conversation <id>` or `--continue-last`). Each conversation gets its own git branch (e.g., `grove/add-pagination-a1b2c3d4`) that accumulates changes across runs.

Conversations let you iterate on a feature across multiple runs without merging to `main` between each one. When you're happy with the result, use `grove conversation merge <id>` to land it.

Key operations:

```bash
grove conversation list          # list conversations for the current project
grove conversation show <id>     # show messages and run history
grove conversation rebase <id>   # rebase the conversation branch onto main
grove conversation merge <id>    # merge into the default branch
```

---

## Run

A **run** is a single execution of an objective. It is the primary unit of work in Grove.

A run has:
- An **objective** — a free-text description of what should be accomplished
- A **state** — one of: `created`, `planning`, `executing`, `waiting_for_gate`, `verifying`, `publishing`, `merging`, `completed`, `failed`, `paused`
- A **budget** — maximum USD to spend (default: $5.00)
- A **pipeline** — the sequence of agents to execute
- A **conversation** — the thread it belongs to
- An **event log** — an append-only record of everything that happened

Runs transition through states automatically. If a run fails, it can be resumed from the last checkpoint.

---

## Session

A **session** is one agent instance within a run. A run can have multiple sessions — for example, a standard pipeline run has sessions for the architect, one or more builders, the reviewer, and the tester.

Each session:
- Runs in its own isolated git **worktree**
- Has its own state: `queued`, `running`, `waiting`, `completed`, `failed`, `killed`
- Tracks its cost independently
- Has a heartbeat so Grove can detect stalled sessions

---

## Worktree

A **worktree** is an isolated checkout of your repository, created by `git worktree add`. Each agent session gets its own worktree so agents can work in parallel without touching each other's files or your working directory.

Worktrees are created under `.grove/worktrees/` by default. When a session completes, its worktree can be merged into the conversation branch and then cleaned up.

Manage worktrees with:

```bash
grove worktrees              # list all worktrees
grove worktrees --clean      # delete all finished worktrees
grove worktrees --delete <session-id>   # delete a specific worktree
grove worktrees --delete-all            # delete all agent worktrees
```

### File ownership locks

To prevent two parallel agents from writing to the same file, Grove uses an **ownership lock** system. When a builder agent begins writing a file, it acquires an exclusive lock on that path for the duration of the run. Other agents that need the same file must wait.

View current locks with `grove ownership`.

---

## Agent roles

Grove defines 20+ specialized agent roles. Each role has its own timeout, retry limits, and custom instructions, all configurable in `grove.yaml`.

### Core roles

| Agent | Role | Default model |
|---|---|---|
| `architect` | Reads the codebase and produces a requirements/design document | claude-opus-4-6 |
| `builder` | Implements code, runs tests | claude-sonnet-4-6 |
| `tester` | Validates changes, writes/runs tests | claude-haiku-4-5 |
| `reviewer` | Audits code quality, produces pass/fail verdict | claude-sonnet-4-6 |
| `security` | Security audit of changes | claude-opus-4-6 |
| `debugger` | Diagnoses and fixes failures (triggered on error) | claude-sonnet-4-6 |
| `refactorer` | Refactoring and code cleanup | claude-haiku-4-5 |
| `documenter` | Updates README, changelog, inline comments | claude-haiku-4-5 |
| `validator` | Cross-step integration checks | claude-sonnet-4-6 |

### Extended roles

| Agent | Role |
|---|---|
| `prd` | Writes product requirements docs |
| `spec` | Writes technical specifications |
| `judge` | Final quality arbiter: APPROVED / NEEDS_WORK |
| `qa` | Quality assurance testing |
| `devops` | Infrastructure and deployment |
| `optimizer` | Performance optimization |
| `accessibility` | Accessibility compliance checks |
| `compliance` | Regulatory compliance checks |
| `dependency_manager` | Dependency updates and CVE fixes |
| `reporter` | Run summary and reporting |
| `migration_planner` | Database/API migration planning |
| `project_manager` | Project coordination |

### Graph agents

These power the graph-based orchestration mode (see [Graphs](#graphs) below):

| Agent | Role |
|---|---|
| `pre_planner` | Generates missing foundational docs (PRD, system design) |
| `graph_creator` | Decomposes specs into phases and steps via MCP tools |
| `builder` | Implements each step (shared with pipeline mode) |
| `verdict` | Reviews builder output, runs tests/lints |
| `phase_validator` | Cross-step integration check, runs integration tests |
| `phase_judge` | Holistic phase grading |

---

## Coding agent backends

Grove orchestrates agent sessions by spawning external coding agent CLIs. The default is Claude Code, but you can route runs through any supported backend:

| Backend | Command | Auto-approve flag |
|---|---|---|
| Claude Code | `claude` | `--dangerously-skip-permissions` |
| Gemini | `gemini` | `--yolo` |
| Codex | `codex` | `--full-auto` |
| Aider | `aider` | `--yes` |
| Cursor | `cursor-agent` | `-f` |
| Copilot | `copilot` | `--allow-all-tools` |
| Qwen Code | `qwen` | `--yolo` |
| OpenCode | `opencode` | (keystroke injection) |
| Kimi | `kimi` | `--yolo` |
| Amp | `amp` | — |
| Goose | `goose` | — |
| Cline | `cline` | `--yolo` |
| Continue | `cn` | — |
| Kiro | `kiro-cli` | — |
| Auggie | `auggie` | — |
| Kilocode | `kilocode` | `--auto` |

Set the default backend in `grove.yaml` under `providers.default`.

---

## Pipelines

A **pipeline** is the ordered sequence of agent phases that a run executes through. The default pipeline runs: architect → builder(s) → reviewer → tester.

### Phase gates

Pipelines support **phase gates** — pause points between stages where you can review the agent's output before it continues. When a gate is reached, the run moves to `waiting_for_gate` state and waits for approval.

Gate approval is available via the desktop GUI.

---

## Graphs

The **graph system** is an alternative to linear pipelines for complex, multi-phase projects. A graph organizes work as a DAG (directed acyclic graph) of **phases**, each containing **steps**.

```
Graph
├── Phase 1: Set up database schema
│   ├── Step 1: Create migrations
│   └── Step 2: Add seed data
├── Phase 2: Build API (depends on Phase 1)
│   ├── Step 1: User endpoints
│   └── Step 2: Auth middleware
└── Phase 3: Frontend (depends on Phase 2)
    ├── Step 1: Login page
    └── Step 2: Dashboard
```

Each step goes through a mini-pipeline: Builder → Verdict → (Phase) Validator → Judge.

The graph system is used automatically by the `graph_creator` agent. You can also interact with it via the MCP server tools.

---

## Tasks and the task queue

Grove includes a **task queue** for scheduling work. Instead of running an objective immediately, you can add it to the queue:

```bash
grove queue "add dark mode support" --priority 10
grove queue "fix flaky tests" --priority 5
```

The queue processor starts a run as soon as the previous one completes. Higher priority values run first; ties are broken by queue time (FIFO).

Manage the queue:

```bash
grove tasks                         # list queued, running, completed
grove task-cancel <task-id>         # cancel a queued task
grove tasks --refresh               # reconcile stale tasks (after a crash)
```

---

## Budget controls

Every run has a **budget** — a maximum USD amount to spend. The budget is enforced in real time based on token usage reported by the AI provider.

| Threshold | Behavior |
|---|---|
| 80% used (warning) | Grove logs a warning; the run continues |
| 100% used (hard stop) | Grove terminates the current session and marks the run failed |

The default budget is $5.00 per run. Configure it in `grove.yaml`:

```yaml
budgets:
  default_run_usd: 10.00
  warning_threshold_percent: 80
  hard_stop_percent: 100
```

Track spending per run:

```bash
grove report <run-id>    # per-session cost for a completed run
```

---

## Merge queue

When multiple agent sessions produce changes (e.g., in a parallel-build pipeline), Grove merges their branches in order using a **merge queue**.

The default strategy is `last_writer_wins` — later agents overwrite earlier ones for the same file. You can change the strategy in `grove.yaml`:

```yaml
merge:
  strategy: last_writer_wins   # or: first_writer_wins
```

If a merge conflict cannot be resolved automatically, Grove records the conflicting files. Inspect them:

```bash
grove conflicts                        # list unresolved conflicts
grove conflicts --show <run-id>        # filter by run
grove merge-status <conversation-id>   # view merge queue for a conversation
```

---

## Checkpoints

Grove saves a **checkpoint** at every major stage transition (configurable). If a run is interrupted, `grove resume <run-id>` replays from the last checkpoint without re-running completed stages.

Checkpoints are stored in the local SQLite database and persist across machine restarts.

---

## Signals

Agents within the same run can send **signals** to each other via the signal system. This is used internally by the orchestrator but can also be used directly:

```bash
grove signal send <run-id> <from> <to> <type> --payload '{"ready":true}'
grove signal check <run-id> <agent>
grove signal list <run-id>
```

---

## Permission modes

Grove supports three **permission modes** that control how agent tool calls are approved:

| Mode | Description |
|---|---|
| `skip_all` | Auto-approve all tool calls (default) — fastest, no interruption |
| `human_gate` | Pause at each tool call and ask you to approve via TTY |
| `autonomous_gate` | Spawn a gatekeeper Claude instance to approve each tool call |

Set per-run:

```bash
grove run "risky refactor" --permission-mode human_gate
```

Set as project default in `grove.yaml`:

```yaml
providers:
  claude_code:
    permission_mode: skip_all
```

---

## Event log and audit trail

Every state change, agent action, and cost event is recorded in an **append-only event log** in the local SQLite database. The events table has database-level triggers that prevent updates or deletes, ensuring the log is tamper-evident.

Access the event log:

```bash
grove logs <run-id>          # recent events (default)
grove logs <run-id> --all    # all events for a run
```
