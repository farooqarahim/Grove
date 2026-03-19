# Grove Desktop GUI

Grove ships a native desktop app built on Tauri v2 (React + TypeScript frontend, Rust backend). It has full feature parity with the CLI -- every command you can run from `grove` on the command line has a corresponding GUI operation.

The GUI crate lives at `crates/grove-gui/`.

---

## Prerequisites

| Tool | Minimum version | Install |
|------|-----------------|---------|
| **Rust** | 1.85+ | [rustup.rs](https://rustup.rs) |
| **Node.js** | 18+ | [nodejs.org](https://nodejs.org) |
| **Tauri CLI** | 2.x | `cargo install tauri-cli --version '^2'` |

The dev script checks all of these on startup and will tell you what's missing.

---

## Running in dev mode

The fastest way to launch the full stack:

```bash
./scripts/dev.sh
```

This single command:

1. Verifies Rust, Node, and the Tauri CLI are installed.
2. Installs npm dependencies if `node_modules/` is missing.
3. Builds companion binaries (`grove-mcp-server`).
4. Launches `cargo tauri dev` inside `crates/grove-gui/`.

The Vite dev server starts on `http://127.0.0.1:1420` with hot module replacement. Edit any `.tsx` file and the change reflects instantly. Rust backend changes trigger a recompile and the window restarts.

Other dev script flags:

```bash
./scripts/dev.sh --build    # production build (native .app / .dmg bundle)
./scripts/dev.sh --check    # run clippy, cargo test, tsc --noEmit
./scripts/dev.sh --admin    # launch grove-db-lookup (DB explorer on :3741/:5173)
./scripts/dev.sh --kill     # kill running grove-gui processes
```

---

## Building for production

```bash
./scripts/dev.sh --build
```

Or manually:

```bash
cd crates/grove-gui
npm run build          # TypeScript + Vite
cargo tauri build      # Rust backend + native bundle
```

The resulting `.app` bundle lands in `crates/grove-gui/src-tauri/target/release/bundle/`. The bundle includes auto-update support via `tauri-plugin-updater` -- update artifacts are signed and endpoints are configured in `tauri.conf.json`.

---

## App architecture

```
+-------------------------------------------+
|  Title bar (draggable, Grove logo)         |
+-----+----------+-----------+--------------+
| Nav |          |           |              |
| Rail| Sidebar  | Main Panel| Right Panel  |
|     | (convs,  | (runs,    | (files, git  |
|  52px projects)| terminal, |  status,     |
|     |          | thread)   |  workspace)  |
+-----+----------+-----------+--------------+
```

The layout uses a three-column resizable split. The NavRail on the far left switches between screens. The Sidebar lists conversations and projects. The MainPanel shows the active content. The RightPanel shows file changes, git status, and workspace info for the selected context.

**Keyboard shortcuts:**

| Shortcut | Action |
|----------|--------|
| `Cmd+1` through `Cmd+5` | Switch screens (Home, Sessions, Issues, Automations, Settings) |
| `Cmd+N` | New session |
| `Cmd+K` | Focus search |
| `Escape` | Close active modal |

---

## Screens overview

### Home (Dashboard)

The landing screen. Shows the workspace name, active project count, and a project list. Three action buttons let you create a new session, register a new project, or file a new issue without navigating away.

### Sessions

The main workspace. A three-panel layout:

- **Sidebar** -- project switcher, conversation list with search, project settings toggle.
- **Main Panel** -- for `run` conversations, shows run cards with pipeline visualization, agent status, phase gates, and QA cards. For `cli` conversations, shows an embedded terminal. For `hive_loom` conversations, shows the Graph panel. Supports a thread view toggle that renders the full agent activity feed (tool use, questions, scope checks, verdicts, artifacts).
- **Right Panel** -- changed files tree, git branch status, commit/push/PR actions.

Three conversation kinds exist:

| Kind | What it does |
|------|-------------|
| `run` | Headless agent pipeline. Grove orchestrates agents in sequence, each in its own worktree. |
| `cli` | Interactive terminal session. A real PTY is spawned running the selected coding agent's CLI. You get tabbed terminals -- tab 0 is the agent, additional tabs are plain shells. |
| `hive_loom` | Graph-mode execution. A DAG of phases and steps runs with an orchestration loop. |

### Issues

A kanban issue board. Covered in detail below.

### Automations

Automation rule management. Covered in detail below.

### Settings

Multi-section settings screen with its own left nav:

| Section | What you configure |
|---------|-------------------|
| **General** | Workspace name, ID, root path, metadata. |
| **Coding Agents** | Enable/disable installed agent CLIs (Claude Code, Codex, Gemini CLI, Aider, etc.), pick a default. |
| **Agent Studio** | Create and edit custom agent definitions, pipelines, and skills. |
| **LLM Providers** | Set API keys, choose workspace default provider/model, view available models with capabilities (vision, tools, reasoning). |
| **Editors** | Detect installed coding CLIs on PATH, toggle which ones appear in the New Task dropdown. |
| **Connections** | Connect GitHub, Jira, Linear for issue sync and PR creation. |
| **Projects** | List, rename, archive, delete projects. Each project expands to show per-project settings. |
| **Worktrees** | View agent worktree pool -- active count, inactive count, disk usage. Clean up stale worktrees globally or scoped to a project. |
| **Hooks & Guards** | Read-only view of event hooks and policy guards from `grove.yaml`. |
| **About** | Version, license, links. |

---

## Two execution modes

### Run mode (Pipeline)

When you create a `run` conversation, Grove executes a **pipeline** -- a linear sequence of agents with optional quality gates between them.

The default pipeline is `build_validate_judge`:

```
PRD Writer -> System Designer -> Builder -> Code Reviewer -> Judge
```

If your objective looks like a bugfix (contains words like "bug", "fix", "error", "failing test"), Grove auto-selects the `bugfix` pipeline instead.

Each agent in the pipeline:

1. Gets its own isolated git worktree.
2. Receives the objective plus upstream artifacts (PRD document, design doc, etc.).
3. Runs to completion or hits a phase gate.

**Phase gates** are checkpoints between agents. When a gate fires, the run pauses and you can approve, reject, or provide feedback before the next agent starts. Gates can be disabled per-run with the `disablePhaseGates` toggle.

The pipeline visualization renders as a horizontal chain of status badges showing each agent's state (pending, executing, completed, failed). Run cards show real-time cost tracking, elapsed time, and the agent's streaming output.

**Permission modes** control how agents interact with tools:

| Mode | Behavior |
|------|----------|
| `skip_all` | Auto-approve all tool calls (fastest). |
| `human_gate` | Ask the user before each tool call. |
| `autonomous_gate` | An AI gatekeeper reviews each tool call. |

### Graph mode (DAG)

When you create a `hive_loom` conversation, Grove uses **Graph mode** -- a directed acyclic graph of phases and steps.

A graph has:

- **Phases** -- ordered groups of work (e.g., "Planning", "Implementation", "Testing").
- **Steps** -- individual tasks within a phase. Steps can have dependencies on other steps.
- **Execution modes** -- `auto` (runs everything it can), `step` (pauses after each step), `phase` (pauses after each phase).

The Graph panel shows:

- A tree view of phases and steps with status indicators and progress bars.
- A control bar with Play, Pause, Abort, and Restart buttons.
- Step detail drawers with feedback, artifacts, and re-run options.
- Phase validation drawers for reviewing completed phases.
- A document editor panel for viewing and editing generated documents (PRDs, specs).

Graphs support clarification questions -- if an agent needs more info, a modal pops up and the graph pauses until you answer.

You can create graphs three ways:

1. **From a prompt** -- describe what you want and Grove generates the phase/step structure.
2. **From a spec** -- paste a specification document and Grove decomposes it.
3. **Simple mode** -- provide a title and description for a quick single-phase graph.

The graph orchestration loop runs on the backend (`grove_core::grove_graph`). The GUI polls for state changes and receives real-time events via Tauri's event system.

---

## Agent Studio

Found under **Settings > Agent Studio**. Three tabs:

### Agents tab

Define custom agent roles. Each agent config has:

- **Name and description** -- what this agent does.
- **Permissions** -- `can_write` (file system access), `can_run_commands` (shell access).
- **Artifact** -- optional output filename the agent produces (e.g., `prd.md`, `design.md`).
- **Allowed tools** -- restrict which tools the agent can use.
- **Skills** -- attach skill definitions that get injected into the agent's prompt.
- **Upstream artifacts** -- declare which other agents' outputs this agent reads.
- **Prompt** -- the system prompt sent to the LLM.

You can preview the fully rendered prompt (with skill content injected) before saving.

### Pipelines tab

Define custom pipelines as ordered sequences of agents with gates between them. Each pipeline has:

- A list of agent IDs (execution order).
- A list of gate positions (which agent transitions trigger a human review).
- Aliases for easy reference.
- A YAML content field for the full pipeline definition.

### Skills tab

Define reusable skill blocks that agents reference. Each skill has a name, description, an `applies_to` list (which agents use it), and the skill content (markdown/text injected into agent prompts).

---

## Issue board

The issue board is a unified kanban across multiple providers. Access it from the **Issues** screen in the nav rail.

### Sources

The board aggregates issues from:

- **Grove** -- native issues stored locally in the Grove database.
- **GitHub** -- synced via the GitHub API.
- **Jira** -- synced via the Jira REST API.
- **Linear** -- synced via the Linear API.

Filter by source using the tab bar at the top. Filter by priority using the dropdown.

### Board layouts

Two layout modes:

- **Project Board** -- columns use the project's configured canonical statuses (Backlog, Todo, In Progress, In Review, Done, Cancelled). Raw provider statuses are mapped to these columns. Edit the board to customize column names and status mappings.
- **Provider Statuses** -- columns mirror the raw statuses from a specific provider. Useful when you want to see exactly how issues are categorized on the provider side.

### Board editor

Click "Edit Board" to open the board editor modal. You can:

- Add, remove, and reorder columns.
- Set which canonical status each column maps to.
- Configure per-provider status mappings.
- Save the board config to the project settings.

### Issue actions

Click an issue card to open the detail drawer. From there you can:

- View and edit the issue title and description.
- Change assignee.
- Add comments.
- View activity history.
- Link the issue to a Grove run.
- Move the issue between columns.
- Push a local Grove issue to an external provider (GitHub, Jira, Linear).
- Delete or reopen issues.

### Sync

Each connected provider shows its sync status (last synced time, error state). Hit "Sync All" or sync individual providers. Sync pulls new issues and updates existing ones.

### Starting runs from issues

From the New Run modal, select a connector source (GitHub, Jira, Linear, or Grove Issues). Pick an issue and its title becomes the objective. When the run completes, Grove can automatically transition the issue status via workflow write-back rules (configured per-project in Settings > Projects > Configure).

---

## Terminal and PTY

Grove embeds a real terminal using xterm.js on the frontend and native PTY sessions on the backend.

### How it works

The Rust backend (`crates/grove-gui/src-tauri/src/pty/`) manages PTY sessions via `fork`/`openpty`. Each session gets:

- A pseudo-terminal pair (master/slave).
- A reader thread that forwards output to the frontend via Tauri events (`grove://pty-output`).
- Write, resize, and close commands exposed as Tauri IPC handlers.

The frontend (`src/components/terminal/`) renders using `@xterm/xterm` with fit and web-links addons.

### Tab system

Each CLI conversation gets a tabbed terminal:

- **Tab 0 (Agent)** -- runs the selected coding agent's CLI (e.g., `claude`, `codex`, `aider`). The command is resolved from the conversation's provider and model settings.
- **Tab 1+ (Shell)** -- plain login shells. Click "+" to open a new shell tab. Close tabs by clicking "x" (agent tab cannot be closed).

Terminal sessions persist across tab switches -- switching back to a conversation restores its terminal state without respawning.

### SSH support

For SSH projects, the PTY spawns an SSH connection directly:

```
ssh -t user@host "cd /remote/path && exec $SHELL -l"
```

No local agent runs are supported for SSH projects -- only interactive shell access.

### Agent output streaming

For `run` conversations (headless pipelines), agent output streams in real-time through the `TauriStreamSink`. The frontend receives events like `assistant_text`, `tool_use`, `tool_result`, `phase_start`, `phase_gate`, `question`, and `scope_violation`. These render in the conversation thread as an activity feed.

---

## Git integration

### Right panel

The right panel shows the git state of the active context (run worktree or project root):

- **Changed files** -- grouped by area (staged, unstaged, untracked, committed).
- **Branch status** -- current branch, ahead/behind counts, remote tracking info.
- **Actions** -- Stage All, Review Changes, Commit.

### Review view

Click "Review Changes" to open the full-screen diff viewer. Features:

- File tree on the left with status indicators (A = added, M = modified, D = deleted).
- Split by area tabs (Unstaged, Staged, Committed, All).
- Unified diff view with syntax-highlighted lines, line numbers, and hunk headers.
- Stage/unstage individual files or all files.
- Revert changes (per-file or all).
- Search within the diff.

### Commit modal

The commit modal offers three publishing tiers:

| Action | What happens |
|--------|-------------|
| **Commit** | Creates a local commit with your message. |
| **Commit and Push** | Commits and pushes to the remote branch. |
| **Commit and Create PR** | Commits, pushes, and opens a pull request. |

For PRs, you can auto-generate the title and body from the diff using the "Generate" button (calls the backend which summarizes the changes).

The `includeUnstaged` toggle controls whether unstaged changes are staged before committing.

### PR status

Once a PR exists, the right panel shows its status (open, merged, closed), review state, and CI check results. You can merge PRs directly from the GUI.

### Worktree-based git operations

All run-scoped git operations target the agent's worktree, not the main project checkout. This means you can review, commit, and create PRs from an agent's changes without touching your working tree. The "Fork Worktree" action creates a new worktree from a run's branch for manual inspection.

---

## Automation rules

The Automations screen lets you create rules that trigger Grove runs automatically.

### Trigger types

| Trigger | How it fires |
|---------|-------------|
| **Cron** | On a schedule (cron expression). The `CronScheduler` polls every 60 seconds for due automations. |
| **Webhook** | When an HTTP request hits the webhook server. Requires `webhook.enabled: true` in `grove.yaml`. Supports a shared secret for signature verification. |
| **Manual** | Click "Run Now" in the GUI. |
| **Event** | On internal Grove events (run completed, run failed, etc.). Dispatched through the `EventBus`. |
| **Issue** | When an issue reaches a configured status (e.g., "ready for dev"). |

### Automation structure

Each automation has:

- **Name and description**.
- **Trigger config** -- the trigger type and its parameters (cron expression, webhook path, event type, etc.).
- **Defaults** -- default provider, model, pipeline, budget, and permission mode for runs spawned by this automation.
- **Session mode** -- `new` (fresh conversation per trigger) or `dedicated` (reuses a pinned conversation).
- **Steps** -- a sequence of actions. Each step can be a Grove run, a shell command, or a notification. Steps form a DAG managed by the `WorkflowEngine`.
- **Notifications** -- optional alerting on completion/failure.

### Detail view

Click an automation to see its detail page with three tabs:

- **Steps** -- view and edit the step sequence. Add steps via the modal (run step, shell step, notification step).
- **Runs** -- history of automation runs with status, duration, and step-level results.
- **Config** -- edit trigger config, defaults, and session mode.

### Background services

The GUI spawns four background services on startup:

1. **WorkflowEngine event loop** -- listens for `TaskFinished` events and advances DAG execution.
2. **CronScheduler** -- polls every 60 seconds for automations with due cron triggers.
3. **Notifier** -- dispatches notifications on automation run completion or failure.
4. **WebhookServer** -- HTTP server for incoming webhook triggers (only when `webhook.enabled` is set).

### Importing automations

You can import automation definitions from YAML files in your project via the "Import from Files" action. This reads `.grove/automations/*.yaml` and creates automation records in the database.

---

## Tauri commands reference

The backend exposes IPC commands organized by domain. Here are the main groups:

| Domain | Commands | Description |
|--------|----------|-------------|
| **Bootstrap** | `get_bootstrap_data` | Single call that returns workspace, projects, conversations, runs, issue count, agent catalog, and connection status. |
| **Projects** | `create_project`, `list_projects`, `archive_project`, `delete_project`, `update_project_name`, `get_project_settings`, `update_project_settings` | Project CRUD and per-project configuration. |
| **Conversations** | `create_conversation`, `list_conversations`, `get_conversation`, `update_conversation_title`, `archive_conversation`, `delete_conversation`, `merge_conversation`, `rebase_conversation_sync` | Conversation lifecycle. |
| **Runs** | `start_run`, `abort_run`, `resume_run`, `list_runs`, `get_run`, `start_run_from_issue` | Run execution and monitoring. |
| **Queue** | `queue_task`, `cancel_task`, `delete_task`, `clear_queue`, `retry_task`, `refresh_queue` | Task queue management with per-conversation and global concurrency limits. |
| **Streaming** | `get_stream_events`, `send_agent_message`, `list_qa_messages` | Real-time agent output and interactive Q&A. |
| **Git** | `git_status_detailed`, `git_stage_files`, `git_commit`, `git_push`, `git_create_pr`, `publish_changes`, `git_merge_pr`, `git_generate_pr_content` | Full git workflow (run-scoped and project-scoped variants). |
| **Issues** | `issue_board`, `issue_create_native`, `issue_move`, `issue_sync_all`, `issue_sync_provider`, `start_run_from_issue`, `push_issue_to_provider` | Issue management and cross-provider sync. |
| **Graph** | `create_graph`, `start_graph_loop`, `pause_graph`, `resume_graph`, `abort_graph`, `rerun_step`, `rerun_phase`, `submit_clarification_answer` | Graph mode DAG lifecycle. |
| **Agent Studio** | `list_agent_configs`, `save_agent_config`, `list_pipeline_configs`, `save_pipeline_config`, `list_skill_configs`, `save_skill_config`, `preview_agent_prompt` | Custom agent/pipeline/skill CRUD. |
| **Automations** | `create_automation`, `list_automations`, `toggle_automation`, `trigger_automation_manually`, `add_automation_step`, `list_automation_runs` | Automation rule management. |
| **PTY** | `pty_open`, `pty_write_new`, `pty_resize_new`, `pty_close_new` | Terminal session management. |
| **Config** | `get_config`, `list_providers`, `set_api_key`, `set_llm_selection`, `get_agent_catalog`, `set_default_provider`, `set_agent_enabled` | Workspace and provider configuration. |

---

## Frontend stack

| Layer | Technology |
|-------|-----------|
| Framework | React 18 |
| Build | Vite 6 |
| Styling | Tailwind CSS + inline styles via a `C` theme object |
| State | TanStack Query v5 (polling + event-driven invalidation) |
| IPC | `@tauri-apps/api` invoke + event listeners |
| Terminal | `@xterm/xterm` with fit and web-links addons |
| UI primitives | Radix UI (Dialog, Tabs, Tooltip, ScrollArea, Progress, Radio Group) |
| Panels | `react-resizable-panels` |
| Icons | Lucide React + custom SVG components |

The frontend uses lazy-loaded screen components (`React.lazy` + `Suspense`) for the Dashboard, Issue Board, Settings, and Automations screens. The Sessions screen loads eagerly since it's the default view.

Data flows through TanStack Query with a dual refresh strategy:

1. **Polling** -- queries refetch on 30-60 second intervals for background freshness.
2. **Event-driven** -- the Rust backend emits Tauri events (`grove://run-changed`, `grove://tasks-changed`, `grove://automations-changed`, `grove://pty-output`) which trigger immediate query invalidation. This gives sub-second UI updates without aggressive polling.

---

## Environment and PATH resolution

macOS GUI apps launch with a minimal system PATH, which means agent CLIs installed via Homebrew, npm, Cargo, or pipx won't be found. The backend solves this by:

1. Spawning the user's shell in interactive-login mode to capture the full `$PATH`.
2. Prepending well-known tool directories (`~/.cargo/bin`, `~/.local/bin`, `~/.bun/bin`, `/opt/homebrew/bin`, etc.).
3. Caching the resolved PATH for the process lifetime.

This runs once at startup and ensures CLIs like `claude`, `codex`, `aider`, and `gemini` are discoverable regardless of how they were installed.
