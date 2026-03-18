# Getting Started with Grove

This guide walks you through installing Grove, running your first objective, and understanding what happens at each step.

---

## 1. Prerequisites

Before installing Grove, make sure you have:

- **Rust 1.85+** — install via [rustup.rs](https://rustup.rs/)
- **Git 2.30+** — required for git worktree support (`git worktree` was added in 2.5 but 2.30 is recommended for reliability)
- **Claude Code CLI** — the `claude` binary; install from [claude.ai/code](https://claude.ai/code)
- **Node.js 18+** — only needed if you want to use the desktop GUI

Verify your setup:

```bash
rustc --version       # rustc 1.85.0 or newer
git --version         # git version 2.30.0 or newer
claude --version      # any recent version
```

---

## 2. Install Grove

### From source (recommended)

```bash
git clone https://github.com/farooqarahim/grove.git
cd grove

# Run the bootstrap script (checks toolchain, verifies dependencies)
./scripts/bootstrap.sh

# Install the grove binary to ~/.cargo/bin/
cargo install --path crates/grove-cli
```

Make sure `~/.cargo/bin` is in your `PATH`:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc  # or ~/.zshrc
source ~/.bashrc
```

### Verify the install

```bash
grove --help
```

---

## 3. Run the doctor

`grove doctor` is a preflight check that verifies your environment is ready:

```bash
grove doctor
```

It checks for:
- Required binaries (`git`, `claude`)
- Git version compatibility
- Database accessibility
- Workspace configuration

If anything is wrong, it tells you exactly what to fix. Use `grove doctor --fix` to apply automatic fixes, or `grove doctor --fix-all` to apply every available fix at once.

---

## 4. Initialize a project

Navigate to any existing git repository and run:

```bash
cd ~/my-project
grove init
```

This creates a `.grove/` directory at the root of your project containing:
- `grove.yaml` — project configuration file
- `worktrees/` — where agent worktrees will be created (git-ignored by default)
- `grove.db` — the local SQLite database

> **Note:** Grove requires the directory to be a git repository. If it is not, run `git init` first.

### Inspect the generated config

```bash
cat .grove/grove.yaml
```

The defaults are sensible for most projects. See [Configuration](configuration.md) for the full reference.

---

## 5. Configure authentication

Grove needs credentials to call the AI provider that powers the agent sessions.

### Option A: Use your own API key

```bash
# Anthropic (Claude) — the default provider
grove auth set anthropic sk-ant-api03-...

# OpenAI
grove auth set openai sk-...

# DeepSeek
grove auth set deepseek sk-...

# Inception
grove auth set inception <token>
```

Keys are stored using your OS keychain (Keychain on macOS, libsecret on Linux) with `0o600` permissions. They are never written to disk in plaintext.

### Option B: Use Grove workspace credits

If your workspace has credits loaded, you can use Grove's pooled API key instead of your own:

```bash
grove llm select anthropic claude-sonnet-4-6 --workspace-credits
```

### Verify auth

```bash
grove auth list
```

---

## 6. Run your first objective

```bash
grove run "add a health check endpoint to the API"
```

Grove will:

1. **Plan** — an architect agent reads your codebase and writes a product requirements doc (`GROVE_PRD_<id>.md`) in the worktree
2. **Design** — a system design agent turns the PRD into a technical spec (`GROVE_DESIGN_<id>.md`)
3. **Build** — one or more builder agents implement the changes in isolated worktrees
4. **Review** — a reviewer agent audits the code and produces a review doc (`GROVE_REVIEW_<id>.md`)
5. **Judge** — a judge agent gives the final APPROVED / NEEDS_WORK verdict (`GROVE_VERDICT_<id>.md`)
6. **Merge** — completed branches are merged into the conversation branch
7. **Publish** — Grove opens a pull request (or pushes directly, depending on `publish.target`)

---

## 7. Monitor progress

### Check run status

```bash
grove status
```

Shows the most recent 20 runs with their state, agent, and cost so far.

### Watch live output

While a run is active, you can tail its event log:

```bash
grove logs <run-id>
```

Or watch the structured plan:

```bash
grove plan <run-id>
```

### Check costs

```bash
grove costs
```

Shows cost breakdown by agent type and a summary of recent runs.

---

## 8. View the report

When a run completes, generate a full report:

```bash
grove report <run-id>
```

The report includes:
- Objective and final verdict
- Per-agent cost breakdown
- Files changed
- Merge status
- PR URL (if published)

---

## 9. Resume an interrupted run

If a run is interrupted (crash, kill signal, network error), resume it from the last checkpoint:

```bash
grove resume <run-id>
```

Grove replays from the last saved stage transition, skipping already-completed work.

---

## 10. Clean up

Agent worktrees accumulate on disk. Clean them up when you're done:

```bash
# Remove all finished (completed/failed) worktrees
grove worktrees --clean

# Full garbage collection (sweep pool slots, prune orphaned branches, git gc)
grove gc
```

---

## Next steps

- [Concepts](concepts.md) — understand runs, pipelines, graphs, and more
- [CLI Reference](cli-reference.md) — full command and flag reference
- [Configuration](configuration.md) — tune Grove for your project
- [Integrations](integrations.md) — connect GitHub Issues, Jira, Linear, and other LLM providers
