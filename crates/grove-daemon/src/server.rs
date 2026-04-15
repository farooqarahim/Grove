use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::Notify;
use tracing::{error, info, warn};

use crate::config::DaemonConfig;
use crate::queue_drain::{self, DrainShutdown, DrainSignal};
use crate::rpc::envelope::{RpcError, RpcRequest, RpcResponse};
use crate::rpc::{DispatchCtx, dispatch};
use crate::session_host::{build_registry, run_idle_sweep};

pub async fn serve(cfg: DaemonConfig) -> Result<()> {
    let _pid_guard = crate::lifecycle::pidfile::PidGuard::acquire(&cfg.pid_path)?;
    if cfg.socket_path.exists() {
        std::fs::remove_file(&cfg.socket_path)
            .with_context(|| format!("remove stale socket {:?}", cfg.socket_path))?;
    }
    let listener = UnixListener::bind(&cfg.socket_path)
        .with_context(|| format!("bind {:?}", cfg.socket_path))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&cfg.socket_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&cfg.socket_path, perms)?;
    }

    info!(path = %cfg.socket_path.display(), "grove-daemon listening");

    let drain_signal = DrainSignal::new();
    let drain_shutdown = DrainShutdown::new();

    let session_registry = build_registry(cfg.session_idle_secs, cfg.max_sessions);
    let sweep_shutdown = Arc::new(Notify::new());
    let sweep_handle = tokio::spawn(run_idle_sweep(
        session_registry.clone(),
        cfg.session_idle_secs,
        sweep_shutdown.clone(),
    ));

    let drain_cfg = cfg.clone();
    let drain_handle = tokio::spawn(queue_drain::run(
        drain_cfg,
        drain_signal.clone(),
        drain_shutdown.clone(),
        session_registry.clone(),
    ));
    // Kick the drain once at startup so any tasks enqueued while the daemon
    // was offline are picked up immediately rather than waiting a full tick.
    drain_signal.notify();

    let ctx = Arc::new(DispatchCtx::new(cfg, drain_signal, session_registry));

    let mut sigterm = signal(SignalKind::terminate()).context("install SIGTERM handler")?;
    let mut sigint = signal(SignalKind::interrupt()).context("install SIGINT handler")?;

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, _addr)) => {
                        let ctx = ctx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, ctx).await {
                                warn!(error = %e, "connection handler error");
                            }
                        });
                    }
                    Err(e) => {
                        error!(error = %e, "accept failed");
                    }
                }
            }
            _ = sigterm.recv() => {
                info!("SIGTERM — shutting down");
                break;
            }
            _ = sigint.recv() => {
                info!("SIGINT — shutting down");
                break;
            }
        }
    }

    drain_shutdown.shutdown();
    if let Err(e) = drain_handle.await {
        warn!(error = %e, "drain loop join error");
    }

    sweep_shutdown.notify_waiters();
    if let Err(e) = sweep_handle.await {
        warn!(error = %e, "idle sweep join error");
    }

    let _ = std::fs::remove_file(&ctx.cfg.socket_path);
    Ok(())
}

async fn handle_connection(stream: UnixStream, ctx: Arc<DispatchCtx>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            return Ok(());
        }
        let resp = match serde_json::from_str::<RpcRequest>(line.trim()) {
            Ok(req) => dispatch(&ctx, req).await,
            Err(e) => RpcResponse::err(None, RpcError::parse_error(e.to_string())),
        };
        let mut bytes = serde_json::to_vec(&resp)?;
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
        writer.flush().await?;
    }
}
