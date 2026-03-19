# Configuration Reference

Grove is configured via a YAML file at `.grove/grove.yaml` in your project root. This file is created automatically by `grove init` and can be edited by hand.

Configuration is resolved in this order (later values override earlier ones):

1. Built-in defaults
2. `.grove/grove.yaml` in the project root
3. `GROVE_*` environment variables
4. Command-line flags

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
```

---

## `providers`

Configure which AI provider powers agent sessions and how it is invoked.

```yaml
providers:
  default: "claude_code"    # Default provider

  claude_code:
    enabled: true
    command: "claude"                    # Path or name of the Claude Code CLI binary
    timeout_seconds: 300                 # Per-session timeout
    permission_mode: "skip_all"          # skip_all | human_gate | autonomous_gate
    allowed_tools: []                    # Additional tools to allow
    max_output_bytes: 10485760           # Maximum bytes of output to capture per session (10 MB)

  mock:
    enabled: true                        # Enable the mock provider for testing
```

### Coding agent backends

Grove supports 15+ external coding agent CLIs. Configure them under `providers.coding_agents`:

```yaml
providers:
  coding_agents:
    claude_code:
      enabled: true
      command: "claude"
      timeout_seconds: 300
      auto_approve_flag: "--dangerously-skip-permissions"
      model_flag: "--model"
      max_output_bytes: 10485760

    gemini:
      enabled: true
      command: "gemini"
      timeout_seconds: 300
      auto_approve_flag: "--yolo"
      initial_prompt_flag: "-i"
      model_flag: "--model"

    codex:
      enabled: true
      command: "codex"
      timeout_seconds: 300
      auto_approve_flag: "--full-auto"
      model_flag: "--model"

    aider:
      enabled: true
      command: "aider"
      timeout_seconds: 300
      auto_approve_flag: "--yes"
      initial_prompt_flag: "--message"
      model_flag: "--model"

    cursor:
      enabled: true
      command: "cursor-agent"
      timeout_seconds: 300
      auto_approve_flag: "-f"

    copilot:
      enabled: true
      command: "copilot"
      timeout_seconds: 300
      auto_approve_flag: "--allow-all-tools"

    goose:
      enabled: true
      command: "goose"
      timeout_seconds: 300

    cline:
      enabled: true
      command: "cline"
      timeout_seconds: 300
      auto_approve_flag: "--yolo"

    kiro:
      enabled: true
      command: "kiro-cli"
      timeout_seconds: 300
      default_args: ["chat"]

    # Additional backends: qwen_code, opencode, kimi, amp, continue, auggie, kilocode
```

---

## `agent_models`

Assign specific LLM models to specific agent roles:

```yaml
agent_models:
  architect: "claude-opus-4-6"
  builder: "claude-sonnet-4-6"
  tester: "claude-haiku-4-5-20251001"
  documenter: "claude-haiku-4-5-20251001"
  security: "claude-opus-4-6"
  reviewer: "claude-sonnet-4-6"
  debugger: "claude-sonnet-4-6"
  refactorer: "claude-haiku-4-5-20251001"
  validator: "claude-sonnet-4-6"
  default: "claude-sonnet-4-6"
```

---

## `agents`

Per-agent-role configuration. Every role supports `timeout_secs`, `max_retries`, and `custom_instructions`. Some roles have additional fields.

```yaml
agents:
  architect:
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""

  builder:
    timeout_secs: 300
    max_retries: 2
    custom_instructions: ""

  tester:
    timeout_secs: 300
    max_retries: 2
    custom_instructions: ""

  reviewer:
    enabled: true
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    on_fail: "block"             # block | warn — action when review fails
    max_retry_cycles: 1

  debugger:
    enabled: true
    timeout_secs: 300
    max_retries: 2
    custom_instructions: ""
    trigger: "on_failure"        # when to activate: on_failure

  security:
    enabled: false
    timeout_secs: 600
    max_retries: 1
    custom_instructions: ""
    on_critical: "block"
    on_high: "warn"
    auto_tools: true

  refactorer:
    timeout_secs: 600
    max_retries: 3
    custom_instructions: ""
    verify_after_each_change: true

  documenter:
    enabled: false
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    update_readme: true
    update_changelog: true
    update_inline_comments: true

  validator:
    enabled: true
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    on_partial: "warn"
    on_failed: "fail"

  judge:
    enabled: false
    timeout_secs: 300
    max_retries: 1
    custom_instructions: ""
    on_needs_work: "warn"

  # Additional roles (disabled by default):
  # prd, spec, qa, devops, optimizer, accessibility, compliance,
  # dependency_manager, reporter, migration_planner, project_manager
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

---

## `orchestration`

Control the orchestration engine behavior.

```yaml
orchestration:
  enforce_design_first: true    # Require design docs before building
  enable_retries: true          # Retry failed agent sessions
  max_retries_per_session: 2    # Maximum retry attempts per session
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
  cleanup_remote_branches: false    # Also delete remote tracking branches on cleanup
  pull_before_publish: true         # Pull latest before pushing results

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
  boot_timeout_secs: 120          # Seconds to wait for agent to start producing output
  stale_threshold_secs: 300       # Mark a session as stalled after this many seconds without a heartbeat
  zombie_threshold_secs: 600      # Kill a session that has been stalled for this long
  max_agent_lifetime_secs: 3600   # Maximum lifetime for a single agent session (1 hour)
  max_run_lifetime_secs: 7200     # Maximum lifetime for an entire run (2 hours)
  poll_interval_secs: 30          # How often to poll for stalled sessions
```

---

## `hooks`

Run shell commands at Grove lifecycle events. Hooks are executed in the project root.

```yaml
hooks:
  post_run: []
  # Examples:
  #   post_run: ["npm install"]
  #   post_run: ["pip install -r requirements.txt"]
  #   post_run: ["cargo build"]
```

---

## `tracker`

Configure external issue tracker integration. See [Integrations](integrations.md) for setup instructions.

```yaml
tracker:
  mode: "disabled"               # disabled | external
  # external:
  #   provider: "github"
  #   create: "gh issue create --title '{title}' --body '{body}' --json number,title,state,labels"
  #   show: "gh issue view {id} --json number,title,state,labels,body"
  #   list: "gh issue list --state open --json number,title,state,labels"
  #   close: "gh issue close {id}"
  #   ready: "gh issue list --label ready --json number,title,state,labels"
```

---

## `webhook`

Configure webhook-triggered automation.

```yaml
webhook:
  enabled: false
  port: 8473
  secret: ""
```

---

## `notifications`

Notification hooks for run events.

```yaml
notifications:
  defaults:
    on_failure: []
    on_success: []
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
