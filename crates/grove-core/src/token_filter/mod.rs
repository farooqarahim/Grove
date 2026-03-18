//! Token reduction pipeline for coding agent output.
//!
//! Grove intercepts known CLI commands via PATH shim injection and compresses
//! their output before agents consume tokens reading it. The pipeline has two
//! stages:
//!
//! 1. **Static filters** — per-command output compression (git, cargo, pytest, …)
//! 2. **Session layer** — deduplication + adaptive compression using session context
//!
//! The mechanism is transparent: Grove prepends a shim directory to PATH before
//! spawning any agent. Shims are symlinks to the `grove-filter` binary, which
//! detects its invoked name via `argv[0]`, runs the real command, and pipes
//! output through the appropriate filter chain.

pub mod metrics;
pub mod project_type;
pub mod session;
pub mod shim;
pub mod static_filters;
pub mod token_count;
