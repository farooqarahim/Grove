# Core Concepts

This document explains the key concepts in Grove. Understanding these will help you use Grove effectively and interpret its output.

---

## Workspace

A **workspace** is the top-level Grove identity for your machine. It is created automatically on first use and holds:

- Global LLM provider selection and credentials
- Credit balance (if using Grove-hosted API keys)
- References to all registered projects

You typically have one workspace per machine. Manage it with `grove workspace show` and `grove workspace set-name`.

---

## Project

A **project** is a local directory registered with Grove. Most commonly it is a git repository, but Grove also supports SSH projects (remote shell access) and plain folder projects.

Each project stores:
- A display name
- The path on disk
- Default settings (pipeline, budget, provider, permission mode)
- Connections to external issue trackers

Initialize a project in a directory with `grove init`, or register an existing directory with `grove project open-folder <path>`.

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

A **session** is one agent instance within a run. A run can have multiple sessions — for example, a `standard` pipeline run has sessions for the PRD agent, the system design agent, one or more builder agents, the reviewer, and the judge.

Each session:
- Runs in its own isolated git **worktree**
- Has its own state: `queued`, `running`, `waiting`, `completed`, `failed`, `killed`
- Tracks its cost independently
- Has a heartbeat so Grove can detect stalled sessions

---

## Worktree

A **worktree** is an isolated checkout of your repository, created by `git worktree add`. Each agent session gets its own worktree so agents can work in parallel without touching each other's files or your working directory.

Worktrees are created under `.grove/worktrees/` by default. When a session completes, its worktree can be merged into the conversation branch and then cleaned up.

Manage worktrees manually with:

```bash
grove worktrees              # list all worktrees
grove worktrees --clean      # delete all finished worktrees
grove worktrees --delete <session-id>   # delete a specific worktree
```

### File ownership locks

To prevent two parallel agents from writing to the same file, Grove uses an **ownership lock** system. When a builder agent begins writing a file, it acquires an exclusive lock on that path for the duration of the run. Other agents that need the same file must wait.

View current locks with `grove ownership`.

---

## Agent types

Grove has two categories of agents:

### Pipeline agents

These execute in sequence as phases of a run:

| Agent | Role | Can write files | Can run commands |
|---|---|---|---|
| `build_prd` | Writes a product requirements doc from the objective | Yes | No |
| `plan_system_design` | Designs architecture, data models, and implementation plan | Yes | No |
| `builder` | Implements code, runs tests | Yes | Yes |
| `reviewer` | Audits changes, produces PASS/FAIL verdict | Yes | Yes |
| `judge` | Final quality arbiter: APPROVED / NEEDS_WORK / REJECTED | Yes | No |

### Graph agents

These power the graph-based orchestration mode (see [Graphs](#graphs) below):

| Agent | Role |
|---|---|
| `pre_planner` | Generates missing foundational docs (PRD, system design, guidelines) |
| `graph_creator` | Decomposes specs into phases and steps via MCP tools |
| `builder` | Implements each step (shared with pipeline mode) |
| `verdict` | Reviews builder output, runs tests/lints (read-only, can run commands) |
| `phase_validator` | Cross-step integration check, runs integration tests (read-only, can run commands) |
| `phase_judge` | Holistic phase grading (read-only, no commands) |

---

## Pipelines

A **pipeline** is the ordered sequence of agent phases that a run executes through. Named pipelines are on the roadmap and will let you pick a preset sequence (e.g., plan-only, build-only, full end-to-end) with a single flag.

> **Coming soon:** Named pipeline presets (`--pipeline bugfix`, `--pipeline security-audit`, etc.) are planned for an upcoming release.

### Phase gates

Pipelines support **phase gates** — pause points between stages where you can review the agent's output before it continues. When a gate is reached, the run moves to `waiting_for_gate` state and waits for approval.

```bash
grove status         # shows: waiting_for_gate
```

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

The graph system is used automatically by the `graph_creator` agent. You can also interact with it via the MCP server tools (`grove_create_graph`, `grove_add_phase`, `grove_add_step`, etc.).

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

Set a budget per run:

```bash
grove run "big refactor" --budget-usd 20.00
```

Or set a project default in `grove.yaml`:

```yaml
budgets:
  default_run_usd: 10.00
  warning_threshold_percent: 80
  hard_stop_percent: 100
```

Track spending across runs:

```bash
grove costs              # breakdown by agent type + recent runs
grove report <run-id>    # per-session cost for one run
```

---

## Merge queue

When multiple agent sessions produce changes (e.g., in a parallel-build pipeline), Grove merges their branches in order using a **merge queue**.

The default strategy is `last_writer_wins` — later agents overwrite earlier ones for the same file. You can change the strategy in `grove.yaml`:

```yaml
merge:
  strategy: last_writer_wins   # or: first_writer_wins
  conflict_strategy: markers   # or: ours, theirs, abort
```

If a merge conflict cannot be resolved automatically, Grove marks the run with a `conflict` status and records the conflicting files. Inspect and resolve them:

```bash
grove conflicts                        # list unresolved conflicts
grove conflicts --show path/to/file    # show conflict details
grove conflicts --resolve path/to/file # mark as resolved
```

---

## Checkpoints

Grove saves a **checkpoint** at every major stage transition (configurable). If a run is interrupted, `grove resume <run-id>` replays from the last checkpoint without re-running completed stages.

Checkpoints are stored in the local SQLite database and persist across machine restarts.

---

## Signals

Agents within the same run can send **signals** to each other via the signal system. This is used internally by the orchestrator but can also be used directly:

```bash
# Send a signal from one agent to another
grove signal send <run-id> architect builder status --payload '{"ready":true}'

# Check signals for an agent
grove signal check <run-id> builder

# List all signals for a run
grove signal list <run-id>
```

Signal priorities: `low`, `normal`, `high`, `urgent`.

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

## Automation

> **Experimental:** The automation system is under active development. APIs and behavior may change.

Grove's automation system lets you trigger agent runs automatically based on events — schedules, webhooks, file changes, or external signals — without having to invoke `grove run` manually.

Automations are defined as YAML files (or via the GUI) and support:
- **Schedules** — run an objective on a cron expression (e.g., nightly test runs)
- **Webhooks** — trigger a run when an external service posts to a Grove endpoint
- **Conditions** — only fire when specific criteria are met (e.g., branch name, file pattern)
- **Notifications** — send alerts when an automation fires or fails

This feature is currently experimental. We are actively working on it and the interface will stabilize in an upcoming release.

---

## Event log and audit trail

Every state change, agent action, and cost event is recorded in an **append-only event log** in the local SQLite database. The events table has database-level triggers that prevent updates or deletes, ensuring the log is tamper-evident.

Access the event log:

```bash
grove logs <run-id>          # last 200 events (default)
grove logs <run-id> --all    # all events for a run
```

A separate **audit log** records every state transition for runs and sessions, including old and new states with timestamps.
