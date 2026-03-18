use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde_json::json;

use crate::config::WatchdogConfig;
use crate::db::DbHandle;
use crate::errors::GroveResult;

/// Actions the watchdog recommends after polling sessions.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchdogAction {
    Healthy,
    Stalled {
        session_id: String,
        idle_secs: u64,
    },
    Zombie {
        session_id: String,
        idle_secs: u64,
    },
    LifetimeExceeded {
        session_id: String,
        elapsed_secs: u64,
    },
    RunLifetimeExceeded {
        run_id: String,
        elapsed_secs: u64,
    },
    BootTimeout {
        session_id: String,
    },
}

/// Maximum retries for watchdog DB connection.
const WATCHDOG_MAX_CONNECT_RETRIES: u32 = 3;

/// Backoff durations for each retry attempt (5s, 10s, 20s).
const WATCHDOG_RETRY_BACKOFFS: [u64; 3] = [5, 10, 20];

/// Try to open a DB connection with retry and backoff.
///
/// Ensures the parent directory exists before each attempt.
fn open_watchdog_connection(db_path: &Path) -> Option<rusqlite::Connection> {
    for attempt in 0..=WATCHDOG_MAX_CONNECT_RETRIES {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let handle = DbHandle::from_db_path(db_path.to_path_buf());
        match handle.connect() {
            Ok(conn) => return Some(conn),
            Err(e) => {
                if attempt < WATCHDOG_MAX_CONNECT_RETRIES {
                    let backoff = WATCHDOG_RETRY_BACKOFFS[attempt as usize];
                    tracing::warn!(
                        error = %e,
                        attempt = attempt + 1,
                        max_retries = WATCHDOG_MAX_CONNECT_RETRIES,
                        backoff_secs = backoff,
                        "watchdog: DB open failed — retrying"
                    );
                    thread::sleep(Duration::from_secs(backoff));
                } else {
                    tracing::error!(
                        error = %e,
                        db_path = %db_path.display(),
                        "watchdog: DB open failed after all retries — \
                         no stale/zombie detection for this run"
                    );
                }
            }
        }
    }
    None
}

/// Spawn a background watchdog thread that polls every `cfg.poll_interval_secs`.
///
/// Drop the returned sender (or send `()`) to shut the thread down.
pub fn spawn_watchdog(
    db_path: &Path,
    run_id: String,
    cfg: &WatchdogConfig,
) -> GroveResult<mpsc::Sender<()>> {
    let (tx, rx) = mpsc::channel::<()>();
    let poll_interval = Duration::from_secs(cfg.poll_interval_secs);
    let cfg = cfg.clone();
    let db_path = db_path.to_owned();

    thread::spawn(move || {
        let mut conn = match open_watchdog_connection(&db_path) {
            Some(c) => c,
            None => return,
        };

        loop {
            match rx.recv_timeout(poll_interval) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::debug!("watchdog: shutdown signal received");
                    break;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Poll
                }
            }

            let now = Utc::now().to_rfc3339();
            match poll_sessions(&conn, &run_id, &cfg, &now) {
                Ok(actions) => {
                    if let Err(e) = execute_actions(&conn, &run_id, &actions) {
                        tracing::warn!(error = %e, "watchdog: failed to execute actions");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "watchdog: poll failed — attempting reconnect");
                    match open_watchdog_connection(&db_path) {
                        Some(new_conn) => {
                            conn = new_conn;
                            tracing::info!("watchdog: reconnected to DB");
                        }
                        None => {
                            tracing::error!("watchdog: reconnect failed — stopping watchdog");
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(tx)
}

/// Pure polling function: queries running sessions and classifies them.
///
/// This is the testable core of the watchdog — no side effects.
pub fn poll_sessions(
    conn: &Connection,
    run_id: &str,
    cfg: &WatchdogConfig,
    now: &str,
) -> GroveResult<Vec<WatchdogAction>> {
    let now_dt: DateTime<Utc> = now.parse().unwrap_or_else(|_| Utc::now());

    let mut actions = Vec::new();

    // Check run lifetime
    let run_created: Option<String> = conn
        .query_row("SELECT created_at FROM runs WHERE id = ?1", [run_id], |r| {
            r.get(0)
        })
        .ok();

    if let Some(ref created_str) = run_created {
        if let Ok(created) = created_str.parse::<DateTime<Utc>>() {
            let elapsed = (now_dt - created).num_seconds().max(0) as u64;
            if elapsed > cfg.max_run_lifetime_secs {
                actions.push(WatchdogAction::RunLifetimeExceeded {
                    run_id: run_id.to_string(),
                    elapsed_secs: elapsed,
                });
            }
        }
    }

    // Query running sessions for this run
    let mut stmt = conn.prepare(
        "SELECT id, started_at, last_heartbeat, stalled_since
         FROM sessions
         WHERE run_id = ?1 AND state = 'running'",
    )?;

    struct SessionInfo {
        id: String,
        started_at: Option<String>,
        last_heartbeat: Option<String>,
    }

    let rows: Vec<SessionInfo> = stmt
        .query_map([run_id], |r| {
            Ok(SessionInfo {
                id: r.get(0)?,
                started_at: r.get(1)?,
                last_heartbeat: r.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    for info in rows {
        let session_id = info.id;
        let started_at = info.started_at;
        let last_heartbeat = info.last_heartbeat;
        // Determine idle time from heartbeat or started_at
        let reference_time = last_heartbeat.as_deref().or(started_at.as_deref());

        let idle_secs = match reference_time {
            Some(ts) => ts
                .parse::<DateTime<Utc>>()
                .map(|t| (now_dt - t).num_seconds().max(0) as u64)
                .unwrap_or(0),
            None => {
                // No heartbeat and no started_at — boot timeout check
                actions.push(WatchdogAction::BootTimeout {
                    session_id: session_id.clone(),
                });
                continue;
            }
        };

        // Check agent lifetime
        if let Some(ref started) = started_at {
            if let Ok(start_dt) = started.parse::<DateTime<Utc>>() {
                let elapsed = (now_dt - start_dt).num_seconds().max(0) as u64;
                if elapsed > cfg.max_agent_lifetime_secs {
                    actions.push(WatchdogAction::LifetimeExceeded {
                        session_id: session_id.clone(),
                        elapsed_secs: elapsed,
                    });
                    continue;
                }
            }
        }

        // Boot timeout: no heartbeat ever received within boot window
        if last_heartbeat.is_none() {
            if let Some(ref started) = started_at {
                if let Ok(start_dt) = started.parse::<DateTime<Utc>>() {
                    let since_start = (now_dt - start_dt).num_seconds().max(0) as u64;
                    if since_start > cfg.boot_timeout_secs {
                        actions.push(WatchdogAction::BootTimeout {
                            session_id: session_id.clone(),
                        });
                        continue;
                    }
                }
            }
        }

        // Zombie vs stalled vs healthy
        if idle_secs > cfg.zombie_threshold_secs {
            actions.push(WatchdogAction::Zombie {
                session_id,
                idle_secs,
            });
        } else if idle_secs > cfg.stale_threshold_secs {
            actions.push(WatchdogAction::Stalled {
                session_id,
                idle_secs,
            });
        }
        // else: healthy — no action needed
    }

    if actions.is_empty() {
        actions.push(WatchdogAction::Healthy);
    }

    Ok(actions)
}

/// Update the heartbeat timestamp for a session.
pub fn touch_heartbeat(conn: &Connection, session_id: &str) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE sessions SET last_heartbeat = ?1 WHERE id = ?2",
        params![now, session_id],
    )?;
    Ok(())
}

/// Execute watchdog actions: emit events, update stalled_since, etc.
fn execute_actions(conn: &Connection, run_id: &str, actions: &[WatchdogAction]) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();

    for action in actions {
        match action {
            WatchdogAction::Healthy => {}
            WatchdogAction::Stalled {
                session_id,
                idle_secs,
            } => {
                conn.execute(
                    "UPDATE sessions SET stalled_since = COALESCE(stalled_since, ?1) WHERE id = ?2",
                    params![now, session_id],
                )?;
                crate::events::emit(
                    conn,
                    run_id,
                    Some(session_id),
                    crate::events::event_types::WATCHDOG_STALLED,
                    json!({ "session_id": session_id, "idle_secs": idle_secs }),
                )?;
                tracing::warn!(session_id = %session_id, idle_secs, "watchdog: session stalled");
            }
            WatchdogAction::Zombie {
                session_id,
                idle_secs,
            } => {
                crate::events::emit(
                    conn,
                    run_id,
                    Some(session_id),
                    crate::events::event_types::WATCHDOG_ZOMBIE,
                    json!({ "session_id": session_id, "idle_secs": idle_secs }),
                )?;
                tracing::error!(session_id = %session_id, idle_secs, "watchdog: zombie session detected");
            }
            WatchdogAction::BootTimeout { session_id } => {
                crate::events::emit(
                    conn,
                    run_id,
                    Some(session_id),
                    crate::events::event_types::WATCHDOG_BOOT_TIMEOUT,
                    json!({ "session_id": session_id }),
                )?;
                tracing::warn!(session_id = %session_id, "watchdog: boot timeout — no heartbeat received");
            }
            WatchdogAction::LifetimeExceeded {
                session_id,
                elapsed_secs,
            } => {
                crate::events::emit(
                    conn,
                    run_id,
                    Some(session_id),
                    crate::events::event_types::WATCHDOG_LIFETIME_EXCEEDED,
                    json!({ "session_id": session_id, "elapsed_secs": elapsed_secs }),
                )?;
                tracing::warn!(session_id = %session_id, elapsed_secs, "watchdog: agent lifetime exceeded");
            }
            WatchdogAction::RunLifetimeExceeded {
                run_id: rid,
                elapsed_secs,
            } => {
                crate::events::emit(
                    conn,
                    run_id,
                    None,
                    crate::events::event_types::WATCHDOG_RUN_LIFETIME_EXCEEDED,
                    json!({ "run_id": rid, "elapsed_secs": elapsed_secs }),
                )?;
                tracing::warn!(run_id = %rid, elapsed_secs, "watchdog: run lifetime exceeded");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn setup_test_db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        db::initialize(dir.path()).unwrap();
        // Re-open for test use
        let handle = DbHandle::new(dir.path());
        let conn = handle.connect().unwrap();
        (dir, conn)
    }

    fn insert_test_run(conn: &Connection, run_id: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'test', 'executing', 5.0, 0.0, ?2, ?2)",
            params![run_id, now],
        )
        .unwrap();
    }

    fn insert_test_session(conn: &Connection, session_id: &str, run_id: &str, started_at: &str) {
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (id, run_id, agent_type, state, worktree_path, started_at, created_at, updated_at)
             VALUES (?1, ?2, 'builder', 'running', '/tmp/wt', ?3, ?4, ?4)",
            params![session_id, run_id, started_at, now],
        )
        .unwrap();
    }

    fn default_cfg() -> WatchdogConfig {
        WatchdogConfig::default()
    }

    #[test]
    fn test_poll_healthy() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_healthy";
        insert_test_run(&conn, run_id);
        let now = Utc::now();
        let started = (now - chrono::Duration::seconds(10)).to_rfc3339();
        insert_test_session(&conn, "sess_h1", run_id, &started);
        // Set a recent heartbeat
        let hb = (now - chrono::Duration::seconds(5)).to_rfc3339();
        conn.execute(
            "UPDATE sessions SET last_heartbeat = ?1 WHERE id = 'sess_h1'",
            [&hb],
        )
        .unwrap();

        let actions = poll_sessions(&conn, run_id, &default_cfg(), &now.to_rfc3339()).unwrap();
        assert_eq!(actions, vec![WatchdogAction::Healthy]);
    }

    #[test]
    fn test_poll_stalled() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_stalled";
        insert_test_run(&conn, run_id);
        let now = Utc::now();
        let started = (now - chrono::Duration::seconds(400)).to_rfc3339();
        insert_test_session(&conn, "sess_s1", run_id, &started);
        // Last heartbeat 310 seconds ago (> 300 stale threshold, < 600 zombie)
        let hb = (now - chrono::Duration::seconds(310)).to_rfc3339();
        conn.execute(
            "UPDATE sessions SET last_heartbeat = ?1 WHERE id = 'sess_s1'",
            [&hb],
        )
        .unwrap();

        let actions = poll_sessions(&conn, run_id, &default_cfg(), &now.to_rfc3339()).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, WatchdogAction::Stalled { .. }))
        );
    }

    #[test]
    fn test_poll_zombie() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_zombie";
        insert_test_run(&conn, run_id);
        let now = Utc::now();
        let started = (now - chrono::Duration::seconds(700)).to_rfc3339();
        insert_test_session(&conn, "sess_z1", run_id, &started);
        // Last heartbeat 650 seconds ago (> 600 zombie threshold)
        let hb = (now - chrono::Duration::seconds(650)).to_rfc3339();
        conn.execute(
            "UPDATE sessions SET last_heartbeat = ?1 WHERE id = 'sess_z1'",
            [&hb],
        )
        .unwrap();

        let actions = poll_sessions(&conn, run_id, &default_cfg(), &now.to_rfc3339()).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, WatchdogAction::Zombie { .. }))
        );
    }

    #[test]
    fn test_boot_timeout() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_boot";
        insert_test_run(&conn, run_id);
        let now = Utc::now();
        // Started 130 seconds ago with no heartbeat (> 120 boot timeout)
        let started = (now - chrono::Duration::seconds(130)).to_rfc3339();
        insert_test_session(&conn, "sess_b1", run_id, &started);

        let actions = poll_sessions(&conn, run_id, &default_cfg(), &now.to_rfc3339()).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, WatchdogAction::BootTimeout { .. }))
        );
    }

    #[test]
    fn test_run_lifetime() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_lifetime";
        let now = Utc::now();
        let old_created = (now - chrono::Duration::seconds(7300)).to_rfc3339();
        conn.execute(
            "INSERT INTO runs (id, objective, state, budget_usd, cost_used_usd, created_at, updated_at)
             VALUES (?1, 'test', 'executing', 5.0, 0.0, ?2, ?2)",
            params![run_id, old_created],
        )
        .unwrap();

        let actions = poll_sessions(&conn, run_id, &default_cfg(), &now.to_rfc3339()).unwrap();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, WatchdogAction::RunLifetimeExceeded { .. }))
        );
    }

    #[test]
    fn test_touch_heartbeat() {
        let (_dir, conn) = setup_test_db();
        let run_id = "run_touch";
        insert_test_run(&conn, run_id);
        let now = Utc::now().to_rfc3339();
        insert_test_session(&conn, "sess_t1", run_id, &now);

        touch_heartbeat(&conn, "sess_t1").unwrap();

        let hb: Option<String> = conn
            .query_row(
                "SELECT last_heartbeat FROM sessions WHERE id = 'sess_t1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(hb.is_some());
    }
}
