# CLI Reference

Complete reference for the `grove` command-line interface.

## Global flags

These flags are accepted by every subcommand:

| Flag | Default | Description |
|---|---|---|
| `--project <path>` | `.` | Path to the project root (directory containing `.grove/`) |
| `--json` | off | Emit machine-readable JSON output |
| `--verbose` | off | Enable verbose logging |
| `--no-color` | off | Disable ANSI color output |

---

## `grove init`

Initialize Grove in the current git repository.

```bash
grove init
```

Creates `.grove/grove.yaml`, `.grove/grove.db`, and `.grove/worktrees/`. Adds `.grove/worktrees/` and `.grove/grove.db` to `.gitignore`.

---

## `grove doctor`

Run a preflight check and report environment issues.

```bash
grove doctor [--fix] [--fix-all]
```

Checks for required binaries, Git version, database health, and configuration validity.

| Flag | Description |
|---|---|
| `--fix` | Apply available automatic fixes interactively |
| `--fix-all` | Apply every available automatic fix without prompting |

---

## `grove run`

Start a new orchestrated agent run.

```bash
grove run "<objective>" [options]
```

| Flag | Description |
|---|---|
| `--max-agents <n>` | Maximum parallel agent sessions |
| `--model <model-id>` | LLM model for agents (e.g. `claude-sonnet-4-6`) |
| `--pipeline <name>` | Named pipeline to use |
| `--permission-mode <mode>` | Tool permission mode: `skip-all`, `human-gate`, `autonomous-gate` |
| `--conversation <id>` | Continue an existing conversation thread |
| `-c, --continue-last` | Continue the most recent conversation for this project |
| `--issue <id>` | Link this run to an external issue by ID |
| `--watch` | Live TUI view (requires the `tui` compile-time feature) |

**Examples:**

```bash
grove run "add a /health endpoint"
grove run "refactor auth module" --model claude-opus-4-6
grove run "now add rate limiting" --continue-last
grove run "redesign database schema" --permission-mode human-gate
```

---

## `grove queue`

Add an objective to the task queue. Starts immediately if nothing is running.

```bash
grove queue "<objective>" [options]
```

| Flag | Default | Description |
|---|---|---|
| `--priority <n>` | `0` | Higher values run first; ties broken by queue time |
| `--model <model-id>` | — | LLM model to use when this task executes |
| `--conversation <id>` | — | Continue an existing conversation |
| `-c, --continue-last` | — | Continue the most recent conversation |

---

## `grove tasks`

List the task queue (queued, running, completed).

```bash
grove tasks [--limit <n>] [--refresh]
```

| Flag | Default | Description |
|---|---|---|
| `--limit <n>` | `50` | Maximum tasks to show |
| `--refresh` | — | Reconcile stale `running` tasks (after crashes) and restart the queue |

---

## `grove task-cancel`

Cancel a queued task by ID.

```bash
grove task-cancel <task-id>
```

Only tasks in `queued` state can be cancelled.

---

## `grove status`

Show recent run status.

```bash
grove status [--limit <n>] [--watch]
```

| Flag | Default | Description |
|---|---|---|
| `--limit <n>` | `20` | Number of recent runs to show |
| `--watch` | — | Live TUI view (requires `tui` feature) |

---

## `grove resume`

Resume an interrupted run from its last checkpoint.

```bash
grove resume <run-id>
```

---

## `grove abort`

Abort an active run.

```bash
grove abort <run-id>
```

Sends a termination signal to all active sessions and marks the run as failed.

---

## `grove logs`

View events from a run.

```bash
grove logs <run-id> [--all]
```

| Flag | Description |
|---|---|
| `--all` | Show all events (default: most recent) |

---

## `grove report`

Display a structured cost report for a completed run.

```bash
grove report <run-id>
```

Includes: total spend, per-agent cost breakdown.

---

## `grove plan`

Show the structured plan (waves, steps, statuses) for a run.

```bash
grove plan [<run-id>]
```

If `run-id` is omitted, shows the most recent run with a plan.

---

## `grove subtasks`

Show the sub-task breakdown for a run.

```bash
grove subtasks [<run-id>]
```

---

## `grove sessions`

List all sessions (agent instances) for a run.

```bash
grove sessions <run-id>
```

---

## `grove worktrees`

Manage agent worktrees.

```bash
grove worktrees [options]
```

| Flag | Description |
|---|---|
| `--clean` | Delete all finished (completed/failed) worktrees |
| `--delete <session-id>` | Delete a specific worktree by session ID |
| `--delete-all` | Delete all agent worktrees (active sessions are skipped) |
| `-y` | Skip confirmation prompt for `--delete-all` |

---

## `grove ownership`

List currently held file ownership locks.

```bash
grove ownership [<run-id>]
```

If `run-id` is omitted, lists all locks across all runs.

---

## `grove merge-status`

Show merge-queue status for a conversation.

```bash
grove merge-status <conversation-id>
```

---

## `grove conflicts`

List and inspect merge conflicts.

```bash
grove conflicts [--show <run-id>]
```

| Flag | Description |
|---|---|
| `--show <run-id>` | Filter conflicts by a specific run ID |

---

## `grove cleanup`

Clean up finished resources.

```bash
grove cleanup [options]
```

| Flag | Description |
|---|---|
| `--project` | Clean up archived/deleted projects |
| `--conversation` | Clean up archived/deleted conversations |
| `--dry-run` | Show what would be deleted without deleting |
| `-y` | Skip confirmation prompt |
| `--force` | Force-release all pool slots and delete all worktree directories |

---

## `grove gc`

Full garbage collection: sweep expired pool holds, prune orphaned branches, run `git gc`.

```bash
grove gc [--dry-run]
```

---

## `grove workspace`

Manage the workspace identity for this machine.

```bash
grove workspace show
grove workspace set-name "<name>"
grove workspace archive <id>
grove workspace delete <id>
```

---

## `grove project`

Manage projects registered in the workspace.

```bash
grove project show
grove project list
grove project open-folder <path> [--name "<name>"]
grove project clone <repo-url> <path> [--name "<name>"]
grove project create-repo <repo> <path> [--provider <name>] [--visibility <vis>] [--gitignore <template>]
grove project fork-repo <source> <target> <repo> [--provider <name>]
grove project fork-folder <source> <target> [--preserve-git]
grove project ssh <host> <remote-path> [--user <user>] [--port <port>]
grove project ssh-shell [<id>]
grove project set-name "<name>"
grove project set [--provider <name>] [--parallel <n>] [--pipeline <name>] [--permission-mode <mode>] [--reset]
grove project archive [<id>]
grove project delete [<id>]
```

---

## `grove conversation`

Manage conversation threads.

```bash
grove conversation list [--limit <n>]
grove conversation show <id> [--limit <n>]
grove conversation archive <id>
grove conversation delete <id>
grove conversation rebase <id>
grove conversation merge <id>
```

`rebase` rebases the conversation branch onto the latest default branch. If there are conflicts, the branch is left unchanged and the conflicting files are reported.

`merge` merges the conversation branch into the project's default branch.

---

## `grove auth`

Manage API keys for LLM providers.

```bash
grove auth set <provider> <api-key>
grove auth remove <provider>
grove auth list
```

Supported providers: `anthropic`, `openai`, `deepseek`, `inception`.

Keys are stored in the OS keychain.

---

## `grove llm`

Browse and configure LLM providers and models.

```bash
grove llm list
grove llm models <provider>
grove llm select <provider> [<model-id>] [--own-key | --workspace-credits]
```

**Examples:**

```bash
grove llm list                                 # show all providers and auth status
grove llm models anthropic                     # list Anthropic models
grove llm select anthropic claude-sonnet-4-6   # set as default
```

> **Note:** `--own-key` and `--workspace-credits` are recognized but not yet fully supported.

---

## `grove signal`

Send and receive inter-agent signals within a run.

```bash
grove signal send <run-id> <from> <to> <type> [--payload <json>] [--priority <n>]
grove signal check <run-id> <agent-name>
grove signal list <run-id>
```

---

## `grove issue`

Manage issues from connected external trackers.

```bash
grove issue list [--cached]
grove issue show <id>
grove issue create "<title>" [--body <text>] [--labels <label>...] [--priority <level>]
grove issue close <id>
grove issue update <id> [--title <text>] [--status <status>] [--label <label>...] [--assignee <name>] [--priority <level>]
grove issue comment <id> "<body>"
grove issue assign <id> <assignee>
grove issue move <id> <status>
grove issue reopen <id>
grove issue search "<query>" [--limit <n>] [--provider <name>]
grove issue sync [--provider <name>] [--full]
grove issue board [--status <status>] [--provider <name>] [--assignee <name>] [--priority <level>]
grove issue board-config show
grove issue board-config set --file <path>
grove issue board-config reset
grove issue activity <id>
grove issue ready
grove issue push <id> --to <provider>
```

---

## `grove fix`

Fetch an issue from a connected tracker and run agents to fix it.

```bash
grove fix [<issue-id>] [options]
grove fix --ready [options]
```

| Flag | Description |
|---|---|
| `--prompt <text>` | Additional instructions beyond the issue description |
| `--ready` | Fix all issues marked as "ready" in connected trackers |
| `--max <n>` | Maximum number of ready issues to fix (with `--ready`) |
| `--parallel` | Queue ready issues as parallel tasks instead of sequential |

**Examples:**

```bash
grove fix PROJ-123
grove fix 42 --prompt "focus on the edge case with empty arrays"
grove fix --ready --max 5
```

> **Note:** `--ready` and `--parallel` are recognized but depend on upstream grove-core support.

---

## `grove connect`

Connect or disconnect external issue tracker providers.

```bash
grove connect github [--token <token>]
grove connect jira --site <url> --email <email> --token <token>
grove connect linear --token <token>
grove connect status
grove connect disconnect <provider>
```

---

## `grove lint`

Run configured linters and show results.

```bash
grove lint [--fix] [--model <model-id>]
```

| Flag | Description |
|---|---|
| `--fix` | Spawn an agent run to fix lint issues after reporting |
| `--model <model-id>` | Model for the fix run |

---

## `grove ci`

Check CI status for a branch and optionally fix failures.

```bash
grove ci [<branch>] [options]
```

| Flag | Description |
|---|---|
| `--wait` | Wait for all CI checks to finish |
| `--timeout <seconds>` | Timeout when using `--wait` |
| `--fix` | If CI is failing, spawn an agent run to fix the failures |
| `--model <model-id>` | Model for the fix run |

---

## `grove publish`

Retry run publication (push + PR creation).

```bash
grove publish retry <run-id>
```

Retries the publish phase for a completed run without re-running agents.

---

## `grove git`

Git operations scoped to Grove's context.

```bash
grove git status
grove git stage <paths...>
grove git unstage <paths...>
grove git revert [<paths...>] [--all]
grove git commit [-m "<message>"] [-a] [--push]
grove git push
grove git pull
grove git branch
grove git log [-n <count>]
grove git undo
grove git pr [--title <text>] [--body <text>] [--base <branch>] [--push]
grove git pr-status
grove git merge [--strategy squash|merge|rebase] [--admin]
```

### `grove git pr`

Creates a pull request via the `gh` CLI. Use `--push` to push the branch first.

### `grove git pr-status`

Shows PR details (number, title, state, URL, branches, author) via `gh pr view`.

### `grove git merge`

Merges the current PR via `gh pr merge`. Always deletes the remote branch after merge.

---

## `grove hook`

Called internally by Claude Code's hook mechanism. Not typically invoked directly.

```bash
grove hook <event> <agent-type> [options]
```

| Flag | Description |
|---|---|
| `--run-id <id>` | Run ID |
| `--session-id <id>` | Session ID |
| `--tool <name>` | Tool name (for `pre_tool_use` / `post_tool_use` events) |
| `--file-path <path>` | File path (for file-write guard checks) |

Supported events: `session_start`, `user_prompt_submit`, `pre_tool_use`, `post_tool_use`, `stop`, `pre_compact`, `post_run`.

---

## JSON output

Any command supports `--json` for machine-readable output:

```bash
grove --json status
grove --json issue list
grove --json report <run-id>
```

---

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | General error |
| `2` | Invalid argument or configuration error |
| `3` | Resource not found |
| `4` | Transport error (socket/connection failure) |
