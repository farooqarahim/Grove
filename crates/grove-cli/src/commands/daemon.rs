use crate::error::{CliError, CliResult};
use clap::Subcommand;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Subcommand, Debug)]
pub enum DaemonCmd {
    /// Start the daemon (foreground by default).
    Start {
        /// Detach into the background and return immediately.
        #[arg(long)]
        detach: bool,
    },
    /// Stop a running daemon via SIGTERM.
    Stop,
    /// Check daemon status via grove.health.
    Status,
    /// Tail the daemon log file.
    Logs {
        #[arg(short = 'n', default_value_t = 50)]
        lines: usize,
    },
}

pub fn run(cmd: DaemonCmd, project_root: &Path) -> CliResult<()> {
    match cmd {
        DaemonCmd::Start { detach } => start(project_root, detach),
        DaemonCmd::Stop => stop(project_root),
        DaemonCmd::Status => status(project_root),
        DaemonCmd::Logs { lines } => logs(project_root, lines),
    }
}

fn locate_daemon_binary() -> CliResult<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("grove-daemon")))
        .ok_or_else(|| CliError::Other("locate grove-daemon binary".into()))
}

fn start(project_root: &Path, detach: bool) -> CliResult<()> {
    let pid_path = grove_core::config::paths::daemon_pid_path(project_root);
    if pid_path.exists() {
        if let Ok(s) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = s.trim().parse::<u32>() {
                if pid_is_live(pid) {
                    return Err(CliError::Other(format!(
                        "daemon already running (pid {pid})"
                    )));
                }
            }
        }
        let _ = std::fs::remove_file(&pid_path);
    }

    let bin = locate_daemon_binary()?;
    let log_path = grove_core::config::paths::daemon_log_path(project_root);
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let mut cmd = Command::new(&bin);
    cmd.arg("--project-root").arg(project_root);

    if detach {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|e| CliError::Other(format!("open log: {e}")))?;
        cmd.stdout(Stdio::from(
            log.try_clone()
                .map_err(|e| CliError::Other(e.to_string()))?,
        ))
        .stderr(Stdio::from(log))
        .stdin(Stdio::null());

        // Spawn and return. The child inherits a detached stdio config; the
        // child remains a member of the CLI's session — acceptable for dev
        // workflow. Production installs should use launchd/systemd.
        let child = cmd
            .spawn()
            .map_err(|e| CliError::Other(format!("spawn daemon: {e}")))?;
        let pid = child.id();

        let sock_path = grove_core::config::paths::daemon_socket_path(project_root);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if sock_path.exists() && pid_is_live(pid) {
                println!("daemon started (pid {pid})");
                return Ok(());
            }
            if !pid_is_live(pid) {
                return Err(CliError::Other(format!(
                    "daemon exited before becoming ready (pid {pid}); see {}",
                    log_path.display()
                )));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(CliError::Other(format!(
            "daemon did not become ready within 5s (pid {pid}); see {}",
            log_path.display()
        )))
    } else {
        let status = cmd
            .status()
            .map_err(|e| CliError::Other(format!("run daemon: {e}")))?;
        if !status.success() {
            return Err(CliError::Other(format!("daemon exited with {status}")));
        }
        Ok(())
    }
}

fn stop(project_root: &Path) -> CliResult<()> {
    let pid_path = grove_core::config::paths::daemon_pid_path(project_root);
    let s = std::fs::read_to_string(&pid_path)
        .map_err(|_| CliError::Other("daemon not running (no pid file)".into()))?;
    let pid: u32 = s
        .trim()
        .parse()
        .map_err(|_| CliError::Other(format!("invalid pid file: {s:?}")))?;

    #[cfg(unix)]
    {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;
        kill(Pid::from_raw(pid as i32), Signal::SIGTERM)
            .map_err(|e| CliError::Other(format!("send SIGTERM: {e}")))?;
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !pid_is_live(pid) {
            println!("daemon stopped (pid {pid})");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(CliError::Other(format!(
        "daemon did not exit within 5s (pid {pid})"
    )))
}

fn status(project_root: &Path) -> CliResult<()> {
    let sock = grove_core::config::paths::daemon_socket_path(project_root);
    if !sock.exists() {
        println!("status: offline (no socket at {})", sock.display());
        return Ok(());
    }
    let transport = crate::transport::socket::SocketTransport::new(sock);
    let v = transport.call_raw("grove.health", serde_json::json!({}))?;
    println!("status: ok");
    println!(
        "pid: {}",
        v.get("pid").and_then(|x| x.as_u64()).unwrap_or(0)
    );
    println!(
        "uptime_ms: {}",
        v.get("uptime_ms").and_then(|x| x.as_u64()).unwrap_or(0)
    );
    Ok(())
}

fn logs(project_root: &Path, lines: usize) -> CliResult<()> {
    let path = grove_core::config::paths::daemon_log_path(project_root);
    let content = std::fs::read_to_string(&path)
        .map_err(|e| CliError::Other(format!("read {}: {e}", path.display())))?;
    let collected: Vec<&str> = content.lines().rev().take(lines).collect();
    for line in collected.into_iter().rev() {
        println!("{line}");
    }
    Ok(())
}

fn pid_is_live(pid: u32) -> bool {
    #[cfg(unix)]
    {
        use nix::sys::signal;
        use nix::unistd::Pid;
        signal::kill(Pid::from_raw(pid as i32), None).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}
