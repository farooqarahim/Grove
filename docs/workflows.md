# Workflows

End-to-end recipes for common Grove tasks. Each workflow shows exactly what to run, what happens behind the scenes, and what to do next.

Prerequisites: you have `grove init` done, `grove doctor` passing, and auth configured. If not, see [Getting Started](getting-started.md).

---

## 1. Fix a single bug

You know the bug. You want a PR. Let Grove handle the rest.

```bash
grove run "fix: the /users endpoint returns 500 when the email field is null"
```

Grove kicks off the default pipeline: architect reads your codebase and scopes the fix, builder implements it in an isolated worktree, tester validates the change, reviewer audits the diff. When everything passes, Grove pushes a branch and opens a PR.

**Monitor while it runs:**

```bash
grove status                 # quick overview of recent runs
grove logs <run-id>          # stream the event log
grove plan <run-id>          # see the structured plan and current stage
grove sessions <run-id>      # list agent sessions and their states
```

**When it finishes:**

```bash
grove report <run-id>        # cost breakdown per agent
```

The PR is already open. Review it, merge it, done.

**If it fails mid-run:**

```bash
grove logs <run-id>          # check what went wrong
grove resume <run-id>        # pick up from the last checkpoint
```

**Clean up after yourself:**

```bash
grove worktrees --clean      # delete finished worktrees
```

---

## 2. Build a feature end-to-end

Bigger than a bug fix. You want Grove to plan, build in phases, and let you review before it charges ahead.

```bash
grove run "add pagination to all list endpoints with cursor-based navigation" \
  --permission-mode human-gate
```

With `human-gate`, Grove pauses at each pipeline stage and waits for your approval in the desktop GUI before continuing. The architect produces a design doc, you review it, approve or reject, then builders start implementing.

**For fully autonomous runs** (you trust the pipeline):

```bash
grove run "add pagination to all list endpoints with cursor-based navigation"
```

No gates, no pauses. Architect, builders, tester, and reviewer run sequentially without interruption.

**Use a specific model for the heavy lifting:**

```bash
grove run "add pagination to all list endpoints" --model claude-opus-4-6
```

**Want multiple builders working in parallel?**

```bash
grove run "add pagination to all list endpoints" --max-agents 4
```

Grove splits the work across up to 4 parallel builder sessions, each in its own worktree. File ownership locks prevent two agents from writing the same file simultaneously.

**Track progress:**

```bash
grove status --watch         # live TUI view (requires tui feature)
grove plan <run-id>          # see wave/step breakdown
grove subtasks <run-id>      # see sub-task decomposition
grove ownership <run-id>     # see which agent owns which files
```

**After the run completes:**

```bash
grove report <run-id>        # cost breakdown
grove conversation list      # see the conversation thread
```

Grove already opened a PR. If you want to iterate, see workflow #4.

---

## 3. Fix all ready issues in parallel

You have a backlog of issues labeled "ready" in GitHub/Jira/Linear. Let Grove chew through them.

```bash
grove fix --ready --parallel --max 5
```

This fetches every issue marked "ready" from your connected tracker, queues up to 5 of them as parallel tasks, and starts running. Each issue gets its own conversation, its own branch, its own PR.

**Step by step:**

```bash
# First, see what "ready" issues exist
grove issue ready

# Fix them all (sequentially, one at a time)
grove fix --ready

# Fix them in parallel, capped at 5
grove fix --ready --parallel --max 5

# Fix a specific issue with extra context
grove fix PROJ-42 --prompt "the root cause is in the caching layer, not the API handler"
```

**Monitor the queue:**

```bash
grove tasks                  # see queued, running, and completed tasks
grove status                 # see active runs
```

**If one task gets stuck:**

```bash
grove abort <run-id>         # kill the stuck run
grove tasks --refresh        # reconcile stale tasks and restart the queue
```

**When everything finishes:**

Each issue gets its own PR. Grove comments on the original issue with a link to the PR if `publish.comment_on_issue` is enabled in `grove.yaml`.

---

## 4. Use conversations to iterate

Conversations are persistent threads. Run once, review the output, then continue the same conversation with follow-up instructions. All changes accumulate on one branch, one PR.

**First run:**

```bash
grove run "add a /health endpoint that returns service status and uptime"
```

Grove creates a new conversation with a branch like `grove/add-health-endpoint-a1b2c3d4`.

**Review and continue:**

```bash
# See what conversations exist
grove conversation list

# Continue the most recent conversation
grove run "also add a /ready endpoint that checks database connectivity" -c

# Or continue a specific conversation by ID
grove run "add rate limiting to both endpoints" --conversation conv_abc123
```

Each follow-up run builds on the previous one. The branch accumulates all changes. No need to merge between iterations.

**Check the conversation history:**

```bash
grove conversation show <conversation-id>
```

**Rebase onto main if it has drifted:**

```bash
grove conversation rebase <conversation-id>
```

If there are conflicts, Grove reports them and leaves the branch unchanged. Fix conflicts manually, then continue.

**When you are happy with the result:**

```bash
grove conversation merge <conversation-id>
```

This merges the conversation branch into your default branch (usually `main`). The PR (if one was opened) gets closed automatically.

---

## 5. Graph mode for complex projects

Pipelines are linear: architect, builder, tester, reviewer, done. Graphs are DAGs: multiple phases with dependencies, each phase containing multiple steps, executed in the right order with validation between phases.

**When to use graphs vs pipelines:**

| Use pipeline when... | Use graph when... |
|---|---|
| Single bug fix | Multi-phase feature spanning several subsystems |
| Small feature (1-3 files) | Work with natural dependency ordering (DB before API before UI) |
| Quick iteration | You want per-phase validation and integration tests |
| You want speed | You need a pre-planner to generate PRD and system design docs |

**How graph mode works:**

The graph system uses specialized agents:

1. **Pre-planner** generates foundational docs (PRD, system design) if they do not exist
2. **Graph creator** decomposes the spec into phases and steps using MCP tools
3. **Builder** implements each step (same agent as pipeline mode)
4. **Verdict** reviews each step's output and runs tests/lints
5. **Phase validator** runs cross-step integration checks after all steps in a phase complete
6. **Phase judge** gives a holistic grade for the phase

**Setting up graph mode:**

Graph mode is configured through the desktop GUI, where you can visually define phases, steps, and dependencies. The graph creator agent can also generate the graph structure automatically from a specification.

Behind the scenes, the MCP server exposes tools like `grove_create_graph`, `grove_add_phase`, and `grove_add_step` that agents use to build and navigate the graph.

**Monitoring graph execution:**

```bash
grove plan <run-id>          # see phases, steps, and their statuses
grove subtasks <run-id>      # detailed step breakdown
grove sessions <run-id>      # see which agents are active
grove logs <run-id>          # event stream
```

Each step goes through a mini-pipeline: Builder, Verdict, (Phase) Validator, Judge. If the verdict agent fails a step, the builder retries. If the phase judge fails a phase, the whole phase can be re-run.

---

## 6. Auto-fix CI failures

Your branch is failing CI. Let Grove read the failure logs and fix the code.

```bash
# Check CI status for the current branch
grove ci

# Wait for CI to finish, then report
grove ci --wait

# If CI is failing, spawn an agent run to fix the failures
grove ci --fix

# Wait for CI, then auto-fix failures with a specific model
grove ci --wait --fix --model claude-sonnet-4-6

# Check a specific branch
grove ci feature/auth-refactor --fix
```

When you pass `--fix`, Grove reads the CI failure logs from GitHub Actions (via the `gh` CLI), extracts the specific errors, and spawns a run whose objective is to fix those failures. The agent gets the full error output as context.

**Typical workflow after a PR review:**

```bash
# You pushed a PR, CI is running
grove ci --wait

# CI failed on lint and two test files
grove ci --fix

# Wait for the fix run to complete
grove status

# CI re-runs on the new push — check again
grove ci --wait
```

**Combine with lint fixing:**

```bash
# Run linters first, fix issues, then check CI
grove lint --fix
grove ci --wait --fix
```

---

## 7. Multi-agent with different providers

Not every agent role needs the same model. Use Opus for architecture, Sonnet for building, Haiku for testing. Or use Gemini for some roles and Claude for others.

**Configure per-role models in `grove.yaml`:**

```yaml
agent_models:
  architect: "claude-opus-4-6"
  builder: "claude-sonnet-4-6"
  tester: "claude-haiku-4-5-20251001"
  reviewer: "claude-sonnet-4-6"
  security: "claude-opus-4-6"
  debugger: "claude-sonnet-4-6"
  refactorer: "claude-haiku-4-5-20251001"
  documenter: "claude-haiku-4-5-20251001"
  default: "claude-sonnet-4-6"
```

**Switch the default coding agent backend:**

```yaml
# grove.yaml
providers:
  default: "gemini"    # route all sessions through Gemini CLI
```

**Configure multiple backends side by side:**

```yaml
providers:
  coding_agents:
    claude_code:
      enabled: true
      command: "claude"
      timeout_seconds: 300
      auto_approve_flag: "--dangerously-skip-permissions"
    gemini:
      enabled: true
      command: "gemini"
      timeout_seconds: 300
      auto_approve_flag: "--yolo"
    aider:
      enabled: true
      command: "aider"
      timeout_seconds: 300
      auto_approve_flag: "--yes"
      initial_prompt_flag: "--message"
      model_flag: "--model"
```

**Override the model on a single run:**

```bash
grove run "refactor the auth module" --model claude-opus-4-6
```

This overrides the default model for all agents in that run. The per-role `agent_models` config in `grove.yaml` takes precedence for individual roles when no `--model` flag is passed.

**Check what providers are available:**

```bash
grove llm list               # all providers, auth status, model counts
grove llm models anthropic   # list Anthropic models
grove llm models openai      # list OpenAI models
grove auth list              # which providers have stored keys
```

---

## 8. Budget-conscious runs

Every run has a budget. Default is $5. You can tune it globally, per-run, and by picking cheaper models.

**Set the global default in `grove.yaml`:**

```yaml
budgets:
  default_run_usd: 2.00             # $2 per run
  warning_threshold_percent: 80     # warn at 80% consumed
  hard_stop_percent: 100            # kill the run at 100%
```

**Pick cheaper models to stretch the budget:**

```yaml
agent_models:
  architect: "claude-sonnet-4-6"        # Sonnet instead of Opus for planning
  builder: "claude-sonnet-4-6"          # Sonnet for building
  tester: "claude-haiku-4-5-20251001"   # Haiku for testing (3x cheaper)
  reviewer: "claude-haiku-4-5-20251001" # Haiku for review
  default: "claude-haiku-4-5-20251001"  # Haiku everywhere else
```

Haiku delivers roughly 90% of Sonnet's capability at a third of the cost. Use it for roles that do not require deep reasoning (testing, reviewing, documentation). Reserve Opus for architecture decisions where reasoning depth matters.

**Monitor spending:**

```bash
grove report <run-id>        # per-agent cost breakdown after a run
grove status                 # see run states (check for budget-killed runs)
```

When a run hits 80% of its budget, Grove logs a warning. At 100%, the current session is terminated and the run is marked failed. You can resume it with a higher budget:

```bash
# Edit grove.yaml to increase the budget, then resume
grove resume <run-id>
```

**Reduce parallelism to reduce cost:**

```yaml
runtime:
  max_agents: 1              # one agent at a time, lowest cost
```

More parallel agents means faster completion but higher peak spend. For budget-sensitive work, run one agent at a time.

**Disable optional agents:**

```yaml
agents:
  security:
    enabled: false           # skip security audit
  documenter:
    enabled: false           # skip doc generation
  judge:
    enabled: false           # skip final quality arbiter
```

Every disabled agent is money saved. Keep the core pipeline (architect, builder, tester, reviewer) and disable the rest unless you need them.

---

## 9. Resume after a crash

Grove checkpoints at every major stage transition. If your machine crashes, the network drops, or you accidentally kill the process, the run can pick up where it left off.

**Find the interrupted run:**

```bash
grove status
```

Look for runs in `failed` or `paused` state. The status output shows the run ID and the last known stage.

**Resume it:**

```bash
grove resume <run-id>
```

Grove reads the last checkpoint from the local SQLite database and replays from the last completed stage. Already-finished work (architect output, completed builder sessions) is not re-run.

**If tasks got stuck in the queue:**

```bash
grove tasks --refresh
```

This reconciles stale `running` tasks (marking them as failed if the process is gone) and restarts the queue processor.

**If sessions are zombie-locked:**

```bash
grove sessions <run-id>      # check session states
grove abort <run-id>         # kill the run and release all locks
grove worktrees --clean      # clean up orphaned worktrees
```

**Nuclear option: full garbage collection:**

```bash
grove gc                     # sweep pool slots, prune orphaned branches, run git gc
grove cleanup --force        # force-release all pool slots and delete all worktree dirs
```

**Prevention: configure the watchdog:**

The watchdog automatically detects and kills stalled sessions. Make sure it is enabled:

```yaml
watchdog:
  enabled: true
  boot_timeout_secs: 120         # kill if agent never starts producing output
  stale_threshold_secs: 300      # mark stalled after 5 minutes of silence
  zombie_threshold_secs: 600     # kill after 10 minutes of stall
  max_agent_lifetime_secs: 3600  # hard cap: 1 hour per agent session
  max_run_lifetime_secs: 7200    # hard cap: 2 hours per run
  poll_interval_secs: 30
```

---

## 10. Connect issue trackers and work issues

Grove integrates with GitHub Issues, Jira, and Linear. Connect one (or all), sync issues locally, fix them, and track everything from the terminal.

### Connect a tracker

**GitHub Issues** (uses the `gh` CLI):

```bash
# Make sure gh is authenticated
gh auth login

# Connect Grove
grove connect github

# Or provide a token directly
grove connect github --token ghp_...
```

**Jira:**

```bash
grove connect jira \
  --site https://mycompany.atlassian.net \
  --email you@company.com \
  --token <jira-api-token>
```

**Linear:**

```bash
grove connect linear --token lin_api_...
```

**Check connection status:**

```bash
grove connect status
```

### Sync and browse issues

```bash
# Sync issues from all connected trackers into the local board
grove issue sync

# Full sync (re-fetch everything, not just changes)
grove issue sync --full

# List issues
grove issue list

# Search
grove issue search "authentication bug"

# View details
grove issue show PROJ-42

# Kanban board view
grove issue board

# Filter the board
grove issue board --status in_progress
grove issue board --provider jira
grove issue board --assignee "Jane"
grove issue board --priority high
```

### Fix an issue

```bash
# Fix a specific issue — Grove reads the title and description, then runs agents
grove fix PROJ-42

# Add extra context for the agent
grove fix PROJ-42 --prompt "the bug is in the caching layer, ignore the API handler"

# Fix all issues labeled "ready"
grove fix --ready

# Fix ready issues in parallel
grove fix --ready --parallel --max 5
```

When a run linked to an issue completes, Grove can automatically comment on the issue with a link to the PR. Enable this in `grove.yaml`:

```yaml
publish:
  comment_on_issue: true
  comment_on_pr: true
```

### Manage issues from the terminal

```bash
# Create an issue
grove issue create "Login fails with SSO" --body "Steps to reproduce..." --labels bug

# Update an issue
grove issue update PROJ-42 --status "In Progress" --assignee "Alice"

# Move an issue to a new status
grove issue move PROJ-42 "In Review"

# Comment on an issue
grove issue comment PROJ-42 "Fixed in PR #87"

# Close an issue
grove issue close PROJ-42

# Push a local issue to an external tracker
grove issue push PROJ-42 --to github
```

### Disconnect a tracker

```bash
grove connect disconnect github
grove connect disconnect jira
grove connect disconnect linear
```

---

## Quick reference

| I want to... | Command |
|---|---|
| Fix a bug | `grove run "fix the bug"` |
| Fix a tracked issue | `grove fix PROJ-42` |
| Fix all ready issues | `grove fix --ready --parallel --max 5` |
| Build a feature | `grove run "build the feature"` |
| Continue iterating | `grove run "next change" -c` |
| Merge a conversation | `grove conversation merge <id>` |
| Check CI | `grove ci --wait` |
| Fix CI failures | `grove ci --fix` |
| See run cost | `grove report <run-id>` |
| Resume a crashed run | `grove resume <run-id>` |
| Clean up worktrees | `grove worktrees --clean` |
| Full garbage collection | `grove gc` |
| See what is running | `grove status` |
| Monitor live | `grove status --watch` |

---

## Next steps

- [Getting Started](getting-started.md) -- install and first run
- [Concepts](concepts.md) -- understand the mental model
- [CLI Reference](cli-reference.md) -- every command and flag
- [Configuration](configuration.md) -- tune `grove.yaml`
- [Integrations](integrations.md) -- providers, backends, trackers
