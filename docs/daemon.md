# Grove Daemon

`grove-daemon` is a long-lived background process that hosts Grove's orchestrator and answers every CLI command over a Unix domain socket. When the daemon is running, `grove <subcommand>` invocations connect to it instead of reopening the SQLite database and re-initializing the orchestrator on each call. This is part of the Wave A foundation described in [`docs/superpowers/cli-runtime-improvements-overview.md`](../../docs/superpowers/cli-runtime-improvements-overview.md).

The daemon is **opt-in**. Nothing auto-starts it. If no daemon is running, the CLI transparently falls back to the in-process `DirectTransport` and behaves exactly as it did before — same commands, same output, same exit codes. There is no functional difference; the daemon is purely a latency/throughput optimization for sessions that issue many CLI calls in a row.

## Lifecycle

```bash
# Start in the foreground (logs to stdout/stderr; Ctrl-C to stop)
grove daemon start

# Start detached (logs to file; returns once the socket is bound)
grove daemon start --detach

# Health check via JSON-RPC; prints pid + uptime_ms
grove daemon status

# Tail the daemon log file
grove daemon logs           # last 50 lines
grove daemon logs -n 200    # last 200 lines

# Send SIGTERM and wait up to 5s for graceful shutdown
grove daemon stop
```

`start --detach` blocks until the daemon has bound its socket (up to 5 seconds). If the child exits before becoming ready, `start` returns a non-zero exit and points at the log file so you can see the underlying error.

## File locations

All daemon state lives under the per-project workspace directory:

```
~/.grove/workspaces/<project_uuid>/
├── grove.sock          # JSON-RPC 2.0 over a Unix domain socket
├── grove-daemon.pid    # PID of the running daemon (RAII-cleaned on exit)
└── grove-daemon.log    # combined stdout+stderr when started with --detach
```

`<project_uuid>` is derived deterministically from the absolute path of the project root, so every CLI invocation in the same project resolves to the same socket. The directory is created lazily on first use.

## How the CLI picks a transport

`GroveTransport::detect_for_path()` chooses between the daemon and the in-process implementation:

1. If the socket file exists **and** a `UnixStream::connect` succeeds → use `SocketTransport` (talk to the daemon).
2. Otherwise → use `DirectTransport` (open the SQLite DB and run the orchestrator in-process).

Because the connect probe runs on every invocation, a stale socket file left by an unclean shutdown will not trick the CLI into talking to a dead endpoint — it falls back to direct mode automatically.

## Troubleshooting

**Stale PID file (`daemon already running (pid N)` but no such process).** The previous daemon crashed without removing its pidfile. Confirm with `ps -p N`; if the process is gone, delete `~/.grove/workspaces/<uuid>/grove-daemon.pid` and retry. `grove daemon start` skips this guard automatically when the recorded PID is no longer alive, so this should be rare.

**Permission denied on socket.** The socket is created with mode `0700` (owner-only). If you see this error, another user owns the socket file. Check `ls -l ~/.grove/workspaces/<uuid>/grove.sock` and either `chown` the workspace directory or remove the stale socket and restart.

**Daemon refuses to start; status shows `offline`.** Run `grove daemon logs -n 100` for the last log lines. The most common causes are: (1) the SQLite DB is locked by another grove process; (2) the workspace directory is on a filesystem that does not support Unix sockets (e.g., some network mounts).

**Force-kill if `stop` times out.** `grove daemon stop` waits 5s for graceful shutdown after sending SIGTERM. If the daemon is wedged, send SIGKILL manually: `kill -9 $(cat ~/.grove/workspaces/<uuid>/grove-daemon.pid)` and remove the pidfile.

## When to use it

Run `grove daemon start --detach` at the start of a session if you are about to issue many `grove` commands (interactive shell work, scripted pipelines, IDE integrations). For one-off invocations, the in-process fallback is fine and avoids the lifecycle overhead.
