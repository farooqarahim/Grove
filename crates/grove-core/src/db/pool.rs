use std::path::Path;
use std::time::Duration;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

use crate::errors::{GroveError, GroveResult};

use super::pragma;

/// A connection pool backed by r2d2 + rusqlite.
///
/// Connections are opened once, PRAGMAs applied via `on_acquire`, and then
/// reused across IPC calls. `Clone` is cheap (inner pool is `Arc`-backed).
#[derive(Debug, Clone)]
pub struct DbPool(Pool<SqliteConnectionManager>);

/// A pooled connection checked out from [`DbPool`].
///
/// Implements `Deref<Target = rusqlite::Connection>` so all existing code
/// that accepts `&Connection` / `&mut Connection` works transparently.
pub type PooledConn = r2d2::PooledConnection<SqliteConnectionManager>;

/// r2d2 hook that applies PRAGMAs and sets the prepared-statement cache
/// capacity on every newly opened physical connection.
#[derive(Debug)]
struct PragmaInitializer;

impl r2d2::CustomizeConnection<rusqlite::Connection, rusqlite::Error> for PragmaInitializer {
    fn on_acquire(&self, conn: &mut rusqlite::Connection) -> Result<(), rusqlite::Error> {
        pragma::apply(conn).map_err(|e| match e {
            GroveError::Database(sqlite_err) => sqlite_err,
            other => rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                Some(other.to_string()),
            ),
        })?;
        conn.set_prepared_statement_cache_capacity(32);
        Ok(())
    }
}

impl DbPool {
    /// Create a new connection pool for the database at `db_path`.
    ///
    /// - `max_size`: maximum number of connections kept open.
    /// - `timeout_ms`: how long to wait (in milliseconds) for a connection
    ///   before returning a pool-exhaustion error.
    /// - PRAGMAs are applied once per physical connection, not per checkout.
    pub fn new(db_path: &Path, max_size: u32, timeout_ms: u64) -> GroveResult<Self> {
        let manager = SqliteConnectionManager::file(db_path);

        // min_idle must not exceed max_size; cap at max_size.
        let min_idle = 2_u32.min(max_size);

        let pool = Pool::builder()
            .max_size(max_size)
            .min_idle(Some(min_idle))
            .connection_timeout(Duration::from_millis(timeout_ms))
            .idle_timeout(Some(Duration::from_secs(300)))
            .connection_customizer(Box::new(PragmaInitializer))
            .build(manager)
            .map_err(|e| GroveError::Runtime(format!("failed to create connection pool: {e}")))?;

        Ok(Self(pool))
    }

    /// Check out a connection from the pool.
    ///
    /// Returns immediately if an idle connection is available, otherwise
    /// blocks up to `connection_timeout`. Emits a `tracing::warn!` when the
    /// pool is exhausted so operators can tune `db.pool_size` or
    /// `db.connection_timeout_ms` in `grove.yaml`.
    pub fn get(&self) -> GroveResult<PooledConn> {
        self.0.get().map_err(|e| {
            tracing::warn!(
                pool_size = self.0.max_size(),
                "connection pool exhausted: {e}"
            );
            GroveError::Runtime(format!(
                "database connection pool exhausted (pool_size={}, timeout={}ms): {e}",
                self.0.max_size(),
                self.0.connection_timeout().as_millis()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_creates_and_checks_out() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create an empty DB file so SQLite can open it.
        std::fs::File::create(&db_path).unwrap();

        let pool = DbPool::new(&db_path, 4, 10_000).unwrap();
        let conn = pool.get().unwrap();

        // Verify PRAGMAs were applied.
        let journal_mode: String = conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        let foreign_keys: i64 = conn
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn pool_concurrent_checkouts() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        std::fs::File::create(&db_path).unwrap();

        let pool = DbPool::new(&db_path, 4, 10_000).unwrap();

        std::thread::scope(|s| {
            for _ in 0..8 {
                let pool = pool.clone();
                s.spawn(move || {
                    let conn = pool.get().unwrap();
                    conn.execute_batch("SELECT 1").unwrap();
                });
            }
        });
    }
}
