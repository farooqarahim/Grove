use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{SignalKind, signal};
use tracing::{error, info, warn};

use crate::config::DaemonConfig;
use crate::rpc::envelope::{RpcError, RpcRequest, RpcResponse};
use crate::rpc::{DispatchCtx, dispatch};

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
    let ctx = Arc::new(DispatchCtx::new(cfg));

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
