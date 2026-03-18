# Configuration Reference

Grove is configured via a YAML file at `.grove/grove.yaml` in your project root. This file is created automatically by `grove init` and can be edited by hand.

A complete example with all fields and their defaults is in `templates/config/grove.example.yaml`.

---

## `project`

Basic project metadata.

```yaml
project:
  name: "my-project"        # Display name for this project
  default_branch: "main"    # Git branch to merge into on completion
```

---

## `runtime`

Controls how many agents run and for how long.

```yaml
runtime:
  max_agents: 3             # Maximum parallel agent sessions per run
  max_run_minutes: 60       # Hard wall-clock timeout for a run (minutes)
  max_concurrent_runs: 4    # Maximum runs active at once across all conversations
  log_level: "info"         # Logging verbosity: trace, debug, info, warn, error
  lock_wait_timeout_secs: 5 # Seconds to wait when acquiring a file ownership lock
```

---

## `providers`

Configure which AI provider powers agent sessions and how it is invoked.

```yaml
providers:
  default: "claude_code"    # Default provider: "claude_code" or "mock"

  claude_code:
    enabled: true
    command: "claude"                    # Path or name of the Claude Code CLI binary
    timeout_seconds: 28800               # Per-session timeout (8 hours)
    long_lived_run_host: false           # Keep a persistent claude process for the run
    permission_mode: "skip_all"          # skip_all | human_gate | autonomous_gate
    allowed_tools: []                    # Additional tools to allow (empty = use agent defaults)
    gatekeeper_model: null               # Model for autonomous_gate mode (null = same model)
    max_output_bytes: 10485760           # Maximum bytes of output to capture per session (10 MB)
    max_file_size_mb: null               # Limit on files the agent can read (null = unlimited)
    max_open_files: null                 # Limit on simultaneously open files (null = unlimited)

  mock:
    enabled: true                        # Enable the mock provider for testing
```

### Coding agent overrides

You can configure individual coding agent backends (Claude Code, Gemini, Codex, Aider, Cursor) under `providers.coding_agents`. This is an advanced option; the defaults work for most setups.

```yaml
providers:
  coding_agents:
    claude_code:
      enabled: true
      command: "claude"
      timeout_seconds: 28800
      auto_approve_flag: "--dangerously-skip-permissions"
      initial_prompt_flag: "--print"
      use_keystroke_injection: false
      use_pty: false
      model_flag: "--model"
      max_output_bytes: 10485760

    gemini:
      enabled: true
      command: "gemini"
      auto_approve_flag: "--yolo"
      initial_prompt_flag: "-i"
      model_flag: "--model"

    codex:
      enabled: true
      command: "codex"
      auto_approve_flag: "--full-auto"
      model_flag: "--model"

    aider:
      enabled: true
      command: "aider"
      auto_approve_flag: "--yes"
      initial_prompt_flag: "--message"
      model_flag: "--model"
```

### Agent-level model overrides

Assign specific models to specific agent types:

```yaml
providers:
  agent_models:
    models:
      build_prd: "claude-opus-4-6"
      plan_system_design: "claude-opus-4-6"
      builder: "claude-sonnet-4-6"
      reviewer: "claude-sonnet-4-6"
      judge: "claude-opus-4-6"
      default: "claude-sonnet-4-6"
```

---

## `budgets`

Control spending limits.

```yaml
budgets:
  default_run_usd: 5.0          # Default budget per run in USD
  warning_threshold_percent: 80  # Log a warning when this % is consumed
  hard_stop_percent: 100         # Terminate the run at this % consumed
```

Override the budget for a single run with `grove run --budget-usd <amount>`.

---

## `orchestration`

Control the orchestration engine behavior.

```yaml
orchestration:
  enforce_design_first: true    # Require PRD + design docs before building
  enable_retries: true          # Retry failed agent sessions
  max_retries_per_session: 2    # Maximum retry attempts per session
  enable_run_mcp: true          # Inject MCP server tools into agent sessions
  max_spawn_depth: 3            # Maximum nesting depth for spawned sub-runs
```

---

## `worktree`

Configure how git worktrees are created and managed.

```yaml
worktree:
  root: ".grove/worktrees"          # Directory where worktrees are created
  branch_prefix: "grove"            # Prefix for agent branch names (e.g. grove/abc123)
  fetch_before_run: true            # Run git fetch before starting a run
  sync_before_run: "merge"          # How to sync before a run: merge | rebase | none
  cleanup_on_success: false         # Auto-delete worktrees when a run completes successfully
  cleanup_remote_branches: false    # Also delete remote tracking branches on cleanup
  min_disk_bytes: 1073741824        # Minimum free disk space to allow a new worktree (1 GiB)
  pull_before_publish: true         # Pull latest before pushing results
  pull_before_publish_timeout_secs: 120

  # Files from .gitignore that should be copied into each worktree
  copy_ignored:
    - ".env"
    - ".env.*"
    - ".env.*.local"
    - ".envrc"
    - "docker-compose.override.yml"
```

The `copy_ignored` list copies secret files (like `.env`) into each worktree so agents have the credentials they need, without committing those files.

---

## `publish`

Configure how completed runs are published (PR creation, branch push).

```yaml
publish:
  enabled: true                    # Enable automatic publishing on run completion
  target: "github"                 # github | direct (direct push, no PR)
  remote: "origin"                 # Git remote to push to
  auto_on_success: true            # Publish automatically when a run succeeds
  pr_mode: "conversation"          # conversation (one PR per conversation) | run (one PR per run)
  retry_on_startup: true           # Retry any pending publishes when Grove starts
  comment_on_issue: true           # Post a comment on the linked issue when a PR is opened
  comment_on_pr: true              # Post a summary comment on the PR
```

---

## `merge`

Configure how agent branches are merged together.

```yaml
merge:
  strategy: "last_writer_wins"     # last_writer_wins | first_writer_wins
  conflict_strategy: "markers"     # markers | ours | theirs | abort
  conflict_timeout_secs: 300       # Seconds to wait for conflict resolution before aborting
  binary_strategy: "last_writer"   # How to handle binary file conflicts: last_writer | first_writer | abort
  lockfile_strategy: "regenerate"  # How to handle lockfile conflicts: regenerate | last_writer | abort

  # Commands to regenerate lockfiles after a conflict
  lockfile_commands:
    "package-lock.json": "npm install"
    "yarn.lock": "yarn install"
    "Cargo.lock": "cargo build"
    "poetry.lock": "poetry lock"

  # Per-agent-type priority (lower number = higher priority)
  priorities:
    build_prd: 0
    builder: 10
```

---

## `checkpoint`

Configure automatic checkpointing for crash recovery.

```yaml
checkpoint:
  enabled: true                    # Enable checkpointing
  save_on_stage_transition: true   # Save a checkpoint on every major state change
```

---

## `observability`

Configure logging and structured output.

```yaml
observability:
  emit_json_logs: true    # Emit structured JSON logs in addition to human-readable output
  redact_secrets: true    # Redact API keys and tokens from log output
```

---

## `network`

```yaml
network:
  allow_provider_network: false    # Allow agents to make outbound network calls to the AI provider
```

---

## `watchdog`

The watchdog monitors sessions for stalls and takes corrective action.

```yaml
watchdog:
  enabled: true
  stall_threshold_secs: 300        # Mark a session as stalled after this many seconds without a heartbeat
  check_interval_secs: 60          # How often to poll for stalled sessions
  action: "kill"                   # What to do when a stall is detected: kill | warn | ignore
```

---

## `hooks`

Run shell commands at Grove lifecycle events. Hooks are executed in the project root.

```yaml
hooks:
  pre_run: "echo 'starting run'"
  post_run: "notify-send 'Grove run complete'"
  pre_session: ""
  post_session: ""
```

---

## `tracker`

Configure external issue tracker integration. See [Integrations](integrations.md) for setup instructions.

```yaml
tracker:
  provider: "github"               # github | jira | linear | grove | none
  project_key: "owner/repo"        # Provider-specific project identifier
  auto_sync: false                 # Sync issues from the tracker before each run
  sync_interval_secs: 3600        # How often to sync if auto_sync is enabled
```

---

## `linter`

Configure linters that Grove runs and reports on.

```yaml
linter:
  enabled: false
  commands:
    - name: "eslint"
      command: "npx eslint . --format json"
      format: "eslint_json"
    - name: "clippy"
      command: "cargo clippy --message-format json"
      format: "cargo_json"
```

---

## `discipline`

Configure constraints on what agents are allowed to do.

```yaml
discipline:
  enforce_design_first: true       # Agents must produce design docs before writing code
  require_tests: false             # Reject completions without test files
  max_files_per_session: 50        # Maximum files an agent may touch in one session
```

---

## `notifications`

Desktop notifications for run events (macOS and Linux via libnotify).

```yaml
notifications:
  enabled: true
  on_run_complete: true
  on_run_failed: true
  on_budget_warning: true
```

---

## `token_filter`

Control token filtering and compression applied to agent inputs to reduce cost.

```yaml
token_filter:
  enabled: true
  compression_level: 1    # 0 = off, 1 = light, 2 = aggressive
  redact_secrets: true
```

---

## `retry`

Configure retry behavior for failed agent sessions.

```yaml
retry:
  max_attempts: 2
  initial_delay_secs: 5
  backoff_multiplier: 2.0
  max_delay_secs: 60
```

---

## `db`

SQLite database configuration.

```yaml
db:
  path: ".grove/grove.db"          # Path to the database file
  wal_checkpoint_interval_secs: 30 # WAL checkpoint interval
  max_connections: 4               # Connection pool size
```

---

## Environment variables

Some settings can be overridden with environment variables:

| Variable | Description |
|---|---|
| `GROVE_PROJECT` | Override the project path |
| `GROVE_LOG` | Log level (trace, debug, info, warn, error) |
| `GROVE_NO_COLOR` | Disable color output |
| `ANTHROPIC_API_KEY` | Anthropic API key (alternative to `grove auth set`) |
| `OPENAI_API_KEY` | OpenAI API key |

---

## Configuration lookup order

Grove resolves configuration in this order (later values override earlier ones):

1. Built-in defaults
2. `.grove/grove.yaml` in the project root
3. Environment variables
4. Command-line flags

This means you can set a project-wide budget in `grove.yaml` and override it for a single run with `--budget-usd`.
