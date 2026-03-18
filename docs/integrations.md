# Integrations

Grove integrates with external LLM providers and issue trackers. This document covers how to configure and use each one.

---

## LLM providers

Grove uses the **Claude Code CLI** (`claude`) as its default agent provider. It also supports Gemini, Codex, Aider, and Cursor. You can mix providers within a project or switch the workspace default.

### Anthropic (Claude) — default

**Supported models:** claude-opus-4-6, claude-sonnet-4-6, claude-haiku-4-5-20251001, and others.

```bash
# Store your API key
grove auth set anthropic sk-ant-api03-...

# Select model at the workspace level
grove llm select anthropic claude-sonnet-4-6 --own-key

# Use Opus for planning agents, Sonnet for builders
```

In `grove.yaml`, assign different models to different agent types:

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

### OpenAI

```bash
grove auth set openai sk-...
grove llm select openai gpt-4o --own-key
grove llm models openai          # list available models
```

### DeepSeek

```bash
grove auth set deepseek sk-...
grove llm select deepseek deepseek-coder --own-key
```

### Inception

```bash
grove auth set inception <token>
grove llm select inception mercury --own-key
```

### Checking provider status

```bash
grove llm list              # shows all providers with auth status and model count
grove auth list             # shows which providers have stored keys
```

### Workspace credits

If you have Grove workspace credits, you can use Grove's pooled API key instead of your own. This lets you start immediately without managing API keys:

```bash
grove llm credits balance                                       # check your credit balance
grove llm select anthropic claude-sonnet-4-6 --workspace-credits  # use credits
```

---

## Issue trackers

Grove can connect to GitHub Issues, Jira, and Linear. Once connected, you can:

- View and manage issues from the terminal with `grove issue`
- Automatically fix issues with `grove fix`
- Have Grove comment on issues and PRs when work completes

### GitHub Issues

GitHub integration uses the `gh` CLI for authentication. Install and authenticate `gh` first:

```bash
# Install gh (https://cli.github.com/)
brew install gh              # macOS
# or: https://github.com/cli/cli#installation

# Authenticate
gh auth login
```

Then connect Grove:

```bash
grove connect github
```

Optionally provide a token directly (useful for automation):

```bash
grove connect github --token ghp_...
```

Set the project key (owner/repo) so Grove knows which repository to use:

```bash
grove project set --provider github --project-key "myorg/myrepo"
```

**What you can do with GitHub integration:**

```bash
grove issue list                    # list open issues
grove issue show 42                 # show issue details
grove issue create "Bug title" --body "Description" --labels "bug,priority:high"
grove issue sync                    # sync issues from GitHub into the local board
grove issue board                   # text-mode kanban view
grove issue search "authentication" # search issues by text
grove fix 42                        # fetch issue #42 and run agents to fix it
grove fix --ready                   # fix all issues labeled "ready"
```

When a run linked to a GitHub issue completes, Grove can automatically:
- Post a comment on the issue with the PR link
- Comment on the PR with a run summary

Configure this in `grove.yaml`:

```yaml
publish:
  comment_on_issue: true
  comment_on_pr: true
```

### Jira

```bash
grove connect jira \
  --site https://mycompany.atlassian.net \
  --email me@company.com \
  --token <jira-api-token>
```

Generate a Jira API token at: https://id.atlassian.com/manage-profile/security/api-tokens

Set the project key:

```bash
grove project set --provider jira --project-key "PROJ"
```

**What you can do:**

```bash
grove issue list                    # list Jira issues
grove issue show PROJ-123           # show issue details
grove issue sync                    # sync from Jira
grove issue board --status open     # view by status
grove issue move PROJ-123 "In Progress"  # update issue status
grove fix PROJ-123                  # fetch and fix an issue
grove fix --ready                   # fix all issues in "Ready" state
```

### Linear

```bash
grove connect linear --token lin_api_...
```

Generate a Linear API token at: https://linear.app/settings/api

Set the team key:

```bash
grove project set --provider linear --project-key "ENG"
```

**What you can do:**

```bash
grove issue list                    # list Linear issues
grove issue show ENG-42             # show issue details
grove issue sync                    # sync from Linear
grove issue board                   # kanban view
grove fix ENG-42                    # fix a specific issue
grove fix --ready --parallel        # fix all "ready" issues in parallel
```

### Checking connection status

```bash
grove connect status
```

Shows which providers are connected and their authentication status.

### Disconnecting a provider

```bash
grove connect disconnect github
grove connect disconnect jira
grove connect disconnect linear
```

---

## Issue board

Regardless of which external tracker you use, Grove maintains a local issue board that syncs from all connected providers. View it as a text-mode kanban:

```bash
grove issue board
grove issue board --status in_progress
grove issue board --provider jira
grove issue board --assignee "Jane Doe"
grove issue board --priority high
```

The board uses canonical statuses that map across all providers:

| Canonical status | GitHub | Jira | Linear |
|---|---|---|---|
| `open` | open | To Do | Todo |
| `in_progress` | — | In Progress | In Progress |
| `in_review` | — | In Review | In Review |
| `blocked` | — | Blocked | Blocked |
| `done` | closed | Done | Done |
| `cancelled` | — | Cancelled | Cancelled |

---

## MCP server

Grove ships a **Model Context Protocol (MCP) server** that exposes Grove's orchestration capabilities as tools. This lets AI agents self-coordinate using Grove via MCP tool calls rather than the CLI.

### Starting the MCP server

```bash
grove-mcp-server --db .grove/grove.db
```

Or via the dev script:

```bash
./scripts/dev.sh --build   # builds grove-mcp-server alongside the GUI
```

### Available MCP tools

The MCP server exposes two groups of tools:

**Graph tools** (for DAG-based orchestration):

| Tool | Description |
|---|---|
| `grove_create_graph` | Create a new execution graph |
| `grove_add_phase` | Add a phase to a graph |
| `grove_add_step` | Add a step to a phase |
| `grove_update_phase_status` | Update phase status |
| `grove_update_step_status` | Update step status |
| `grove_set_step_outcome` | Record step outcome and grade |
| `grove_set_phase_outcome` | Record phase outcome and grade |
| `grove_list_graph_phases` | List all phases of a graph |
| `grove_list_graph_steps` | List all steps in a phase |
| `grove_get_graph_progress` | Comprehensive progress overview |
| `grove_get_step_pipeline_state` | Get step's pipeline state and judge feedback |
| `grove_check_runtime_status` | Check for pause/abort signals |
| `grove_get_step_dependencies_status` | Check if step dependencies are satisfied |

**Run tools** (for classic pipeline orchestration):

| Tool | Description |
|---|---|
| `grove_get_pipeline_stage` | Get the next pending pipeline stage |
| `grove_complete_pipeline_stage` | Mark a stage complete |
| `grove_check_pipeline_gate` | Poll for gate approval |
| `grove_run_get_context` | Get full run context (objective, history, artifacts) |
| `grove_run_get_current_phase` | Get current agent and pending gate |
| `grove_run_get_phase_artifacts` | List artifacts produced by an agent |
| `grove_run_record_artifact` | Record an artifact for a run |
| `grove_run_request_gate` | Create a pending phase gate |
| `grove_run_wait_for_gate` | Wait for gate decision (polls until approved) |
| `grove_run_get_next_step` | Get the next assigned agent |
| `grove_run_complete_phase` | Mark the current agent phase complete |
| `grove_run_abort_check` | Check if the run has been aborted |
| `grove_run_budget_status` | Get current budget usage |

### Configuring Claude Code to use the MCP server

Add this to your Claude Code settings to inject Grove's MCP server into agent sessions:

```json
{
  "mcpServers": {
    "grove": {
      "command": "grove-mcp-server",
      "args": ["--db", ".grove/grove.db"]
    }
  }
}
```

When `orchestration.enable_run_mcp` is `true` (the default), Grove automatically injects MCP tools into agent sessions so agents can query their own run state, record artifacts, and coordinate via the graph/pipeline APIs.

---

## CI integration

Grove can watch your CI status and automatically fix failures.

```bash
# Check CI status for the current branch
grove ci

# Wait for CI to complete, then report
grove ci --wait

# If CI is failing, spawn an agent run to fix it
grove ci --fix --budget-usd 10

# Combine: wait for CI, then auto-fix if it fails
grove ci --wait --fix
```

This integrates with GitHub Actions via the `gh` CLI. The `grove ci` command reads the workflow status for the current branch and, if `--fix` is specified, spawns a run with the `ci-fix` pipeline targeting the specific failures.
