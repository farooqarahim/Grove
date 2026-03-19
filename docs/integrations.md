# Integrations

Grove integrates with external LLM providers, coding agent CLIs, and issue trackers. This document covers how to configure and use each one.

---

## LLM providers

Grove supports multiple LLM providers for direct API access. The default agent backend is Claude Code (which handles its own API calls), but you can also configure Grove to call LLM APIs directly.

### Anthropic (Claude) — default

**Supported models:** claude-opus-4-6, claude-sonnet-4-6, claude-haiku-4-5-20251001, and others.

```bash
grove auth set anthropic sk-ant-api03-...
grove llm select anthropic claude-sonnet-4-6
```

In `grove.yaml`, assign different models to different agent roles:

```yaml
agent_models:
  architect: "claude-opus-4-6"
  builder: "claude-sonnet-4-6"
  tester: "claude-haiku-4-5-20251001"
  reviewer: "claude-sonnet-4-6"
  security: "claude-opus-4-6"
  default: "claude-sonnet-4-6"
```

### OpenAI

```bash
grove auth set openai sk-...
grove llm select openai
grove llm models openai          # list available models
```

### DeepSeek

```bash
grove auth set deepseek sk-...
grove llm select deepseek
```

### Inception

```bash
grove auth set inception <token>
grove llm select inception
```

### Checking provider status

```bash
grove llm list              # shows all providers with auth status and model count
grove auth list             # shows which providers have stored keys
```

---

## Coding agent backends

Grove orchestrates work by spawning external coding agent CLIs. Each agent session runs one of these backends in an isolated worktree.

### Supported backends

| Backend | CLI command | Notes |
|---|---|---|
| **Claude Code** | `claude` | Default. Best integration. |
| **Gemini** | `gemini` | Google's coding agent |
| **Codex** | `codex` | OpenAI's coding agent |
| **Aider** | `aider` | Open-source AI pair programmer |
| **Cursor** | `cursor-agent` | Cursor's agent mode |
| **Copilot** | `copilot` | GitHub Copilot CLI |
| **Qwen Code** | `qwen` | Alibaba's coding agent |
| **OpenCode** | `opencode` | Uses keystroke injection |
| **Kimi** | `kimi` | Moonshot's coding agent |
| **Amp** | `amp` | Sourcegraph's coding agent |
| **Goose** | `goose` | Block's coding agent |
| **Cline** | `cline` | VS Code extension CLI |
| **Continue** | `cn` | Continue.dev CLI |
| **Kiro** | `kiro-cli` | Amazon's coding agent |
| **Auggie** | `auggie` | AI coding assistant |
| **Kilocode** | `kilocode` | AI coding assistant |

### Switching the default backend

```yaml
# grove.yaml
providers:
  default: "gemini"    # route all agent sessions through Gemini
```

### Per-backend configuration

Each backend can be customized in `grove.yaml`:

```yaml
providers:
  coding_agents:
    aider:
      enabled: true
      command: "aider"
      timeout_seconds: 300
      auto_approve_flag: "--yes"
      initial_prompt_flag: "--message"
      model_flag: "--model"
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
grove project set --provider github
```

**What you can do with GitHub integration:**

```bash
grove issue list                    # list open issues
grove issue show 42                 # show issue details
grove issue create "Bug title" --body "Description" --labels bug --labels priority:high
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
grove project set --provider jira
```

**What you can do:**

```bash
grove issue list                    # list Jira issues
grove issue show PROJ-123           # show issue details
grove issue sync                    # sync from Jira
grove issue board --status open     # view by status
grove issue move PROJ-123 "In Progress"  # update issue status
grove fix PROJ-123                  # fetch and fix an issue
```

### Linear

```bash
grove connect linear --token lin_api_...
```

Generate a Linear API token at: https://linear.app/settings/api

**What you can do:**

```bash
grove issue list                    # list Linear issues
grove issue show ENG-42             # show issue details
grove issue sync                    # sync from Linear
grove issue board                   # kanban view
grove fix ENG-42                    # fix a specific issue
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

Grove can check your CI status and automatically fix failures.

```bash
# Check CI status for the current branch
grove ci

# Wait for CI to complete, then report
grove ci --wait

# If CI is failing, spawn an agent run to fix it
grove ci --fix

# Combine: wait for CI, then auto-fix if it fails
grove ci --wait --fix --model claude-sonnet-4-6
```

This integrates with GitHub Actions via the `gh` CLI. The `grove ci` command reads the workflow status for the current branch and, if `--fix` is specified, spawns a run targeting the specific failures.

---

## Linting

Grove can run configured linters and optionally spawn an agent to fix issues:

```bash
# Run linters and report results
grove lint

# Run linters, then spawn an agent to fix issues
grove lint --fix --model claude-sonnet-4-6
```

Configure linters in `grove.yaml` under the `tracker` section.
