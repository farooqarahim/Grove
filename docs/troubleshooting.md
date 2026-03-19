# Troubleshooting

This guide covers common issues you'll hit with Grove and how to fix them. If you're looking for setup instructions, see [getting-started.md](./getting-started.md). For configuration reference, see [configuration.md](./configuration.md).

---

## grove doctor

`grove doctor` is your first stop when something feels off. It runs a series of health checks in dependency order and tells you exactly what's broken.

### What it checks

| Check | What it verifies | Fix hint |
|---|---|---|
| `git_available` | `git --version` succeeds | Install git |
| `config_valid` | `.grove/grove.yaml` exists and passes validation | Run `grove init` or `grove doctor --fix` |
| `db_accessible` | SQLite database exists and can be opened | Run `grove init` or delete `.grove/grove.db` and reinit |
| `schema_version_current` | DB schema version meets minimum requirements | Run `grove doctor --fix` to apply migrations |
| `provider_binary_present` | The configured provider CLI (`claude`, `codex`, etc.) is on PATH | Install the provider binary |
| `api_key_set` | `ANTHROPIC_API_KEY` is set (when using `claude_code` provider) | Set the env var, or rely on the Claude CLI's stored credentials |

### Using --fix

```bash
grove doctor --fix       # apply fixes interactively
grove doctor --fix-all   # apply every safe fix without prompting
```

What `--fix` can do automatically:
- Create a default `.grove/grove.yaml` if missing
- Create the `.grove/` subdirectories (logs, reports, checkpoints, worktrees)
- Initialize or re-initialize the SQLite database and apply schema migrations

What `--fix` cannot do (you need to handle these yourself):
- Install git
- Install a provider binary (claude, codex, etc.)
- Set environment variables like `ANTHROPIC_API_KEY`

These show up as `NotApplicable` in the fix output, but the check result includes a hint telling you what to do.

### Interpreting output

Text mode shows a summary line followed by per-check status:

```
ok  healthy
  git:    ok
  sqlite: ok
  config: ok
```

If anything fails, you'll see `FAIL` or `MISSING` next to it. Use `--json` for machine-readable output:

```bash
grove doctor --json
```

Returns `{"ok": true/false, "git": true/false, "sqlite": true/false, "config": true/false}`.

---

## Agent won't start

### Provider binary not found

Grove shells out to an external coding agent CLI. If the binary isn't on your PATH, the run fails immediately.

The default provider is `claude_code`, which expects a binary called `claude`. Check:

```bash
which claude
claude --version
```

If you're using a different provider (codex, gemini, aider, etc.), make sure that binary is installed and reachable. The command name is set in `.grove/grove.yaml` under `providers.claude_code.command` or `providers.coding_agents.<id>.command`.

Grove resolves the PATH using the user's shell profile, so if the binary works in your terminal but not in Grove, check that your PATH is exported properly in `.zshrc` / `.bashrc`.

### Authentication issues

When the `claude_code` provider is active, Grove checks for `ANTHROPIC_API_KEY` in the environment. If it's missing, Grove logs a warning but continues -- the Claude CLI has its own stored credentials that may work fine.

If you see `LLM auth error (claude_code): ...`, your API key is either invalid or expired. Fix it:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

Or store it in the OS keychain:

```bash
grove auth set anthropic sk-ant-...
```

For other providers, the error message includes the provider name -- e.g., `LLM auth error (openai): ...` means your OpenAI key is the problem.

### Permission modes

Grove supports three permission modes for how the coding agent handles tool-use requests:

- **`skip_all`** (default) -- passes `--dangerously-skip-permissions` to auto-approve everything. Fastest, least safe.
- **`human_gate`** -- pauses on each permission request and asks you via TTY. Only works in interactive sessions.
- **`autonomous_gate`** -- spawns a lightweight gatekeeper model to decide. Requires a working API key.

If an agent hangs at startup in `human_gate` mode but you're running headless (CI, background task), switch to `skip_all`:

```yaml
# .grove/grove.yaml
providers:
  claude_code:
    permission_mode: "skip_all"
```

Or pass it per-run:

```bash
grove run "fix the bug" --permission-mode skip-all
```

---

## Run stuck or hanging

### Watchdog timeouts

Grove runs a watchdog that monitors agent sessions for signs of trouble. The thresholds are configurable in `.grove/grove.yaml`:

```yaml
watchdog:
  enabled: true
  boot_timeout_secs: 120      # agent must produce output within 2 min of start
  stale_threshold_secs: 300   # no activity for 5 min = stale warning
  zombie_threshold_secs: 600  # no activity for 10 min = killed
  max_agent_lifetime_secs: 3600  # hard cap: 1 hour per agent
  max_run_lifetime_secs: 7200    # hard cap: 2 hours per run
  poll_interval_secs: 30      # how often to check
```

The provider also has its own idle timeout: if an agent produces no stdout for 10 minutes, Grove kills the child process.

If your agents legitimately need more time (large codebases, complex reasoning), bump these values. If they're genuinely stuck, see the next section.

### Stale sessions

A session is "stale" when the DB says it's `running` or `waiting` but nothing is actually happening. This can occur after a crash, power loss, or `kill -9`.

`grove gc` detects ghost sessions -- sessions whose worktree directory has disappeared from disk -- and marks them as `failed`. It also cleans up their parent runs.

```bash
grove gc                    # sweep stale sessions, prune branches, run git gc
grove gc --dry-run          # see what would happen without doing it
```

You can also use `grove tasks --refresh` to reconcile stale `running` tasks and restart the queue.

### How to abort

```bash
grove abort <run-id>
```

This sends a termination signal to all active sessions in the run, releases all ownership locks, saves a checkpoint (so you can resume later), and transitions the run to `paused` state. The abort is graceful -- it captures the provider session ID so a future `grove resume` can continue the conversation.

If `grove abort` doesn't work (the process is truly wedged), you can force-clean everything:

```bash
grove cleanup --force       # release all pool slots, delete all worktree directories
```

---

## Merge conflicts

### How Grove handles merges

After agents finish their work in isolated worktrees, Grove merges their branches back. The merge happens in a temporary detached worktree so your checked-out branch is never touched.

The merge flow:
1. Pre-flight conflict check: Grove identifies files changed on both sides before attempting the merge. This is informational only -- it logs a warning but doesn't block.
2. `git merge --no-ff --no-commit <branch>` in the temp worktree.
3. If configured, a `PreMerge` hook runs. If the hook fails, the merge is aborted.
4. If the merge succeeds, the commit is written and the target branch ref is updated.
5. If the merge has conflicts, the merge is aborted immediately and the conflicting files are reported.
6. The temporary worktree is always cleaned up, regardless of outcome.

There's a 60-second timeout on each git command during merge. If git hangs (e.g., waiting for credential input), it gets killed.

### Conflict strategies

Configure how Grove handles unresolved conflicts in `.grove/grove.yaml`:

```yaml
merge:
  strategy: "last_writer_wins"     # file-level: last agent's version wins (default)
  # strategy: "three_way"          # line-level merge via git merge-file
  # strategy: "ai_resolve"         # AI-assisted conflict resolution (needs API key)
  conflict_strategy: "markers"     # write conflict markers into files (default)
  # conflict_strategy: "auto"      # silently last-writer-wins (good for CI)
  # conflict_strategy: "pause"     # pause and ask you to resolve (TTY only)
  # conflict_strategy: "fail"      # treat any conflict as fatal
```

Binary files always use `last_writer` by default. Lockfiles default to `regenerate` (re-runs the package manager after merge).

### Manual resolution

When you see `merge conflict on N file(s): ...` in the output:

```bash
grove conflicts                    # list all conflicts
grove conflicts --show <run-id>    # show conflicts for a specific run
```

If the merge target is `github`, conflicts appear in the PR. If it's `direct`, the conflicting merge entry is marked `conflict` in the merge queue and you can re-queue it after resolving:

```bash
grove merge-status <conversation-id>
```

---

## Budget exceeded

### What happens

Every run has a dollar budget. After each agent response, Grove records the cost and checks the budget:

- At **80% used** (configurable via `warning_threshold_percent`): a warning is logged. The run continues.
- At **100% used** (configurable via `hard_stop_percent`): the run is hard-stopped. Active sessions are terminated. The error looks like: `budget exceeded: used $5.0000 of $5.0000`.

The budget is also enforced at the merge layer -- if the budget is exhausted, pending merges are denied with `budget exhausted (remaining: $0.0000); cannot proceed with merge`.

### How to adjust

The default budget is `$5.00` per run. Change it globally in `.grove/grove.yaml`:

```yaml
budgets:
  default_run_usd: 10.0
  warning_threshold_percent: 80
  hard_stop_percent: 100
```

### Per-run overrides

Pass a budget when starting a run:

```bash
grove run "refactor the entire auth module" --budget 25.0
```

### After a budget stop

The run transitions to `failed`. You have two options:

1. **Resume with more budget** -- if there's a checkpoint, `grove resume <run-id>` picks up where it left off. You'll need to increase the budget in config first.
2. **Start fresh** -- just run a new `grove run` with a higher budget.

Check the cost breakdown:

```bash
grove report <run-id>
```

---

## Crash recovery

### How checkpoints work

Grove saves checkpoints at stage transitions when `checkpoint.save_on_stage_transition` is enabled (it is by default). A checkpoint captures:

- `run_id` and current `stage`
- List of `active_sessions` (session IDs)
- List of `pending_tasks` (remaining objectives)
- `ownership` snapshot (which files are locked by which session)
- `budget` snapshot (allocated vs. used USD)

Checkpoints are stored as JSON rows in the `checkpoints` table in SQLite. Each checkpoint gets a unique ID like `cp_<uuid>`.

When a run is aborted gracefully (via `grove abort`), Grove also captures the provider's session/thread ID so that `resume` can continue the coding agent's conversation rather than starting cold.

### grove resume

```bash
grove resume <run-id>
```

This:
1. Loads the latest checkpoint for the run.
2. Reads the original pipeline settings (pipeline type, phase gates, provider thread ID) from the run record.
3. Transitions the run from `paused` or `failed` back to `executing`.
4. Re-runs the remaining agent plan using the same objective and conversation.

If no checkpoint exists, you'll see: `not found: no checkpoint found for run <run-id>`.

### When to start fresh

Resume from checkpoint when:
- The run was aborted intentionally and you want to continue.
- A transient failure (network, API rate limit) caused the run to fail.
- You increased the budget after a budget-exceeded stop.

Start a new run when:
- The original objective was wrong.
- The codebase has changed significantly since the run started.
- The checkpoint is from a different schema version (after a Grove upgrade).
- The run failed due to a fundamental problem (wrong provider, broken config).

---

## Worktree issues

### How worktrees work in Grove

Each agent session gets its own git worktree under `.grove/worktrees/`. This gives agents isolated working directories so they can edit files without stepping on each other.

Before creating a worktree, Grove checks that there's at least 1 GiB of free disk space (configurable via `worktree.min_disk_bytes`). If disk space is low, the run refuses to start.

### Stale worktrees

After a crash or forced kill, worktree directories can be left behind. They take up disk space and their git branches clutter `git branch` output.

Clean them up:

```bash
grove worktrees                     # list all worktrees and their status
grove worktrees --clean             # delete finished (completed/failed) worktrees
grove worktrees --delete <sess-id>  # delete a specific worktree by session ID
grove worktrees --delete-all -y     # delete all worktrees (skips active sessions)
```

### Orphaned branches

`grove gc` handles orphaned branches -- it finds `grove/*` branches whose worktree directory no longer exists and batch-deletes them with `git branch -D`.

```bash
grove gc
```

This also runs `git worktree prune` to clean up git's internal worktree tracking.

### Ghost sessions

If a session's worktree directory disappeared from disk (host reboot, manual deletion) but the DB still shows it as `running` or `waiting`, `grove gc` detects this and marks both the session and its parent run as `failed`. This prevents the system from waiting forever for a session that will never complete.

---

## Database issues

### WAL mode

Grove uses SQLite in WAL (Write-Ahead Logging) mode with these pragmas applied on every connection:

```
journal_mode = WAL
synchronous = NORMAL
foreign_keys = ON
busy_timeout = 30000   (30 seconds)
cache_size = -8000     (8 MB)
temp_store = MEMORY
auto_vacuum = INCREMENTAL
```

WAL mode enables concurrent readers and a single writer without blocking. The `busy_timeout` of 30 seconds means a write that can't acquire the lock will retry for 30 seconds before failing with `SQLITE_BUSY`.

The database uses a connection pool (default 8 connections, 10-second checkout timeout). If you see pool exhaustion errors, increase the pool size:

```yaml
db:
  pool_size: 16
  connection_timeout_ms: 15000
```

### WAL checkpointing

After each run, Grove runs a passive WAL checkpoint -- this writes WAL pages back to the main database file without blocking active readers. You can see the WAL stats in `grove gc` output.

If the WAL file grows very large, run a full checkpoint manually (only do this when no runs are active):

```bash
sqlite3 .grove/grove.db "PRAGMA wal_checkpoint(FULL);"
```

### Corruption recovery

If `grove doctor` reports `sqlite: FAIL`, the database may be corrupted. Grove's integrity check runs both `PRAGMA integrity_check` and `PRAGMA foreign_key_check`.

Options:

1. **Try `grove doctor --fix`** -- this re-initializes the database and applies migrations. It creates a new database if the old one can't be opened, but you'll lose run history.

2. **Delete and reinitialize**:
   ```bash
   rm ~/.grove/workspaces/<project-id>/.grove/grove.db
   grove init
   ```
   The database path is centralized under `~/.grove/workspaces/<project-uuid>/` for real projects. For the exact location, check `grove doctor --json` output.

3. **Recover from backup** -- if you have a backup of `grove.db`, replace the file and run `grove doctor --fix` to apply any missing migrations.

### grove gc

Full garbage collection sweep:

```bash
grove gc              # sweep ghost sessions, prune branches, run git gc
grove gc --dry-run    # preview what would happen
```

What `grove gc` does:
- Detects ghost sessions (worktree missing on disk, session still marked active) and marks them as `failed`
- Cleans up orphaned `grove/*` branches
- Runs `git worktree prune`
- Runs `git gc` on the main repository

---

## CI integration failures

### Checking CI status

```bash
grove ci                          # check CI for current branch
grove ci <branch>                 # check a specific branch
grove ci --wait --timeout 300     # wait up to 5 min for checks to finish
```

### Fixing CI failures

```bash
grove ci --fix                    # spawn an agent to fix failing CI checks
grove ci --fix --model claude-opus-4-6  # use a specific model for the fix
```

### Common CI issues

**Branch not found**: If `grove ci <branch>` says the branch doesn't exist, it may not have been pushed yet. Check with `git branch -r` or push it first:

```bash
grove git push
```

**Timeout**: The default `--wait` timeout is reasonable for most CI pipelines. If your CI is slow, pass a longer timeout:

```bash
grove ci --wait --timeout 600
```

**Fix not applied**: When `grove ci --fix` runs, it spawns a new agent run targeting the failing checks. If the fix doesn't take, check the run report:

```bash
grove status              # find the fix run ID
grove report <run-id>     # see what happened
grove logs <run-id>       # see detailed events
```

---

## Common error messages

These are the actual error types from Grove's source code. Here's what each one means and what to do about it.

### `configuration error: <details>`

Your `.grove/grove.yaml` has an invalid value. The details tell you which field is wrong. Common causes:
- `runtime.max_agents` set to 0 or above 32
- `budgets.default_run_usd` set to 0 or negative
- `budgets.warning_threshold_percent` >= `hard_stop_percent`
- `providers.default` is empty
- `project.name` or `project.default_branch` is empty
- `worktree.branch_prefix` contains invalid git ref characters

Fix: edit `.grove/grove.yaml` and correct the field mentioned in the error.

### `database error: <details>`

SQLite operation failed. Usually means the database is locked, corrupted, or the schema is out of date. If it's a `SQLITE_BUSY` error, another process is holding the write lock -- wait and retry. If it persists, see the [Database issues](#database-issues) section.

### `not found: <resource>`

The requested resource doesn't exist. Common cases:
- `not found: run <id>` -- the run ID doesn't match any record
- `not found: no checkpoint found for run <id>` -- can't resume because no checkpoint was saved

Double-check the ID. Use `grove status` to list recent runs.

### `invalid state transition: run <id>: <from> -> <to> is not allowed`

You tried an operation that doesn't make sense for the run's current state. For example, trying to resume a run that's already `completed`, or aborting a run that's already `failed`. Check the run's current state with `grove status` and act accordingly.

### `budget exceeded: used $X.XXXX of $Y.YYYY`

The run hit its spending cap. See [Budget exceeded](#budget-exceeded).

### `merge conflict on N file(s): <files>`

Agent branches have conflicting changes. See [Merge conflicts](#merge-conflicts).

### `LLM auth error (<provider>): <message>`

API key is missing, invalid, or expired for the named provider. Set it:

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
# or
grove auth set anthropic sk-ant-...
```

### `LLM request error (<provider>): <message>`

Network or TLS failure talking to the LLM API. Check your internet connection. If you're behind a proxy, make sure `HTTPS_PROXY` is set. This is usually transient -- Grove will retry according to the retry config (default: 3 retries, exponential backoff from 1s to 30s).

### `LLM API error (<provider>) HTTP <status>: <message>`

The LLM provider returned a non-2xx HTTP status. Common statuses:
- **401** -- bad API key
- **429** -- rate limited. Wait and retry, or reduce `runtime.max_agents` to lower concurrent requests.
- **500/502/503** -- provider outage. Wait and retry.

### `run aborted by user`

You (or the GUI) triggered `grove abort`. This is normal -- the run saves a checkpoint and you can `grove resume` it later.

### `concurrent conversation limit reached for project <id> (<active>/<max> active)`

Too many conversations are running at once. The default limit is 4 concurrent runs. Either wait for a run to finish, or increase the limit:

```yaml
runtime:
  max_concurrent_runs: 8
```

### `worktree error (<operation>): <message>`

A git worktree operation failed. Common causes:
- Disk full (Grove requires at least 1 GiB free by default)
- The target branch doesn't exist
- Git index lock left behind by a crashed process

For stale index locks:

```bash
rm .git/index.lock
# or in a worktree:
rm .grove/worktrees/<session-id>/.git/index.lock
```

### `ownership conflict on '<path>': held by session <holder>`

Two agents tried to modify the same file. Grove uses ownership locks to prevent this. If the holding session is dead (crashed), the lock is stale. `grove abort <run-id>` releases all locks for a run, or `grove gc` cleans up ghost sessions.

### `provider error (<provider>): <message>`

The coding agent subprocess failed for a non-HTTP reason (crash, timeout, unexpected output). Check:
- Is the provider binary the right version?
- Is the provider properly configured? Run `grove doctor` to verify.
- Check the provider's own logs if it has any.

### `hook error (<hook>): <message>`

A lifecycle hook script failed. Check the hook command in your config:

```yaml
hooks:
  post_run: ["npm install"]
  on:
    pre_merge:
      - command: "./scripts/pre-merge-check.sh"
        blocking: true
        timeout_secs: 30
```

If a blocking `PreMerge` hook fails, the merge is aborted. Fix the hook script or remove it from the config.

### `validation error (<field>): <message>`

An input value failed validation. The field name and message tell you exactly what's wrong. This is a catch-all for validation logic beyond config file parsing.

### `insufficient workspace credits: have $X.XXXX, need $Y.YYYY`

The workspace credit balance is too low for the requested LLM call. Top up credits or switch to using your own API key.

---

## Exit codes

| Code | Meaning | When you'll see it |
|---|---|---|
| `0` | Success | Command completed without errors |
| `1` | General error | Catch-all for `GroveError::Config`, `GroveError::Database`, `GroveError::Io`, `GroveError::Runtime`, and other unclassified errors |
| `2` | Invalid argument | Bad CLI flag, missing required argument, or configuration validation failure (`CliError::BadArg`) |
| `3` | Resource not found | Run ID, session ID, or other resource doesn't exist (`CliError::NotFound`) |
| `4` | Transport error | Socket or connection failure, typically when communicating with the Grove daemon or a remote service (`CliError::Transport`) |

In scripts, check the exit code to decide how to proceed:

```bash
grove doctor
case $? in
  0) echo "all good" ;;
  1) echo "something broke -- check the output" ;;
  2) echo "bad arguments -- check your command" ;;
  3) echo "resource not found" ;;
  4) echo "connection failed" ;;
esac
```

---

## Getting help

### Verbose mode

Add `--verbose` to any command for detailed logging:

```bash
grove --verbose run "add a /health endpoint"
grove --verbose doctor
```

This enables structured log output that includes timing, module paths, and internal state transitions.

### Viewing logs

```bash
grove logs <run-id>          # show recent events for a run
grove logs <run-id> --all    # show all events
```

Logs are also written as JSON to `.grove/logs/` and as markdown run memory to `.grove/log/<conversation-id>/`.

### Run reports

```bash
grove report <run-id>
```

Shows a structured cost report with per-agent breakdown. Reports are saved to `.grove/reports/`.

### JSON output

Every command supports `--json` for machine-readable output:

```bash
grove --json status
grove --json doctor
grove --json report <run-id>
```

### Useful diagnostic commands

```bash
grove doctor               # health check everything
grove status               # see recent runs and their states
grove sessions <run-id>    # see all agent sessions for a run
grove ownership            # see held file locks (stale locks = stuck run)
grove worktrees            # see worktree status and disk usage
grove gc --dry-run         # preview what cleanup would do
grove plan <run-id>        # see the structured plan (waves, steps, statuses)
grove subtasks <run-id>    # see sub-task breakdown
```

### Filing issues

When reporting a bug, include:
1. The exact command you ran
2. The full error message (use `--verbose` for extra detail)
3. Output of `grove doctor --json`
4. Output of `grove --json status` (if the issue involves a run)
5. Your `.grove/grove.yaml` (redact any API keys)
6. Your OS and Grove version
