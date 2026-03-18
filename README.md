# Grove

Local single-user orchestration engine for coordinating coding agents in isolated git worktrees.

Grove manages parallel AI coding agent sessions, each working in its own git worktree, with automatic merge orchestration, conflict resolution, and budget controls.

## Features

- **Worktree isolation** — each agent session runs in a dedicated git worktree, preventing interference
- **Merge orchestration** — automatic merge queue with conflict detection and resolution strategies
- **Budget controls** — token and cost limits per run with policy enforcement
- **Pipeline support** — define multi-phase workflows with quality gates
- **MCP server** — expose Grove operations via the Model Context Protocol
- **Desktop GUI** — Tauri-based desktop application for visual orchestration
- **Event system** — SQLite-backed audit log of all orchestration events
- **Crash recovery** — resume interrupted runs from the last checkpoint

## Architecture

Grove is a Rust workspace with the following crates:

| Crate              | Description                                                                |
| ------------------ | -------------------------------------------------------------------------- |
| `grove-core`       | Core orchestration engine, state machine, worktree management, merge queue |
| `grove-cli`        | Command-line interface (`grove` binary)                                    |
| `grove-gui`        | Tauri desktop application with web frontend                                |
| `grove-mcp-server` | MCP server for tool-based integration                                      |
| `grove-filter`     | Token and content filtering                                                |

## Prerequisites

- [Rust](https://rustup.rs/) 1.85+ (via `rust-toolchain.toml`)
- Git 2.30+
- [Node.js](https://nodejs.org/) 18+ (for the GUI frontend)

## Getting Started

```bash
# Clone the repository
git clone https://github.com/farooqarahim/grove.git
cd grove

# Bootstrap (verifies toolchain)
./scripts/bootstrap.sh

# Build
cargo build

# Run tests
cargo test

# Install the CLI locally
cargo install --path crates/grove-cli
```

## Usage

```bash
# Initialize a Grove workspace in a git repo
grove init

# Start an orchestrated run
grove run

# Check run status
grove status

# View run report
grove report
```

## Configuration

Grove uses YAML configuration files. See `templates/config/` for examples.

## Development

The quickest way to launch the full dev stack (Tauri + React with hot reload):

```bash
# Kill any running instances, then start fresh
./scripts/dev.sh --kill && ./scripts/dev.sh
```

This runs preflight checks (Rust, Tauri CLI, Node.js), installs npm dependencies if needed, builds companion binaries (MCP server), and launches the GUI in dev mode with hot reload.

### Other dev commands

```bash
./scripts/dev.sh              # Launch GUI in dev mode (hot reload)
./scripts/dev.sh --build      # Production build (native .app bundle)
./scripts/dev.sh --check      # Run full CI checks (clippy, test, tsc)
./scripts/dev.sh --admin      # Launch grove-db-lookup (DB explorer)
./scripts/dev.sh --kill       # Kill any running grove-gui instances

# Standard Rust tooling
cargo fmt                     # Format code
cargo clippy                  # Lint

# Project scripts
./scripts/check.sh            # Run all checks
./scripts/smoke.sh            # Run smoke tests
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
