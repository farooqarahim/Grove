//! Daemon-side plumbing for the persistent Claude Code session registry.
//!
//! Owns construction of the in-memory registry and the periodic idle-sweep
//! background task. The registry itself lives in `grove-core` so non-daemon
//! consumers (e.g. integration tests) can build their own.

use grove_core::providers::session_host::registry::{InMemorySessionHostRegistry, RegistryConfig};
use grove_core::providers::session_host::SessionHostRegistry;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;
use tracing::{info, warn};

/// Construct a fresh in-memory session-host registry with the given limits.
/// Returned as `Arc<dyn SessionHostRegistry>` so callers can hand it to
/// `DispatchCtx`, `queue_drain::run`, and `build_provider` without depending
/// on the concrete impl type.
pub fn build_registry(idle_secs: u64, max_hosts: usize) -> Arc<dyn SessionHostRegistry> {
    Arc::new(InMemorySessionHostRegistry::new(RegistryConfig {
        max_hosts,
        idle_timeout: Duration::from_secs(idle_secs),
    }))
}

/// Periodic idle-sweep loop. Wakes every `max(30, idle_secs/4)` seconds and
/// asks the concrete registry to evict hosts that have been idle longer than
/// `idle_timeout`. Exits when `shutdown` is notified.
///
/// The sweep is best-effort: if the registry behind the trait object is not
/// the in-memory impl (e.g. a test stub), this loop logs a warning and exits
/// — the registry can still serve get_or_spawn but no eviction happens.
pub async fn run_idle_sweep(
    registry: Arc<dyn SessionHostRegistry>,
    idle_secs: u64,
    shutdown: Arc<Notify>,
) {
    let tick = Duration::from_secs(std::cmp::max(30, idle_secs / 4));
    info!(interval_secs = tick.as_secs(), "session idle sweep started");
    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                info!("session idle sweep shutting down");
                return;
            }
            _ = tokio::time::sleep(tick) => {}
        }
        if let Some(concrete) = registry
            .as_any()
            .downcast_ref::<InMemorySessionHostRegistry>()
        {
            let n = concrete.sweep_idle().await;
            if n > 0 {
                info!(evicted = n, "idle sweep evicted stale hosts");
            }
        } else {
            warn!("registry does not support sweep_idle; idle sweep stopping");
            return;
        }
    }
}
