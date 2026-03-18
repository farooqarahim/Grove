use chrono::Utc;
use rusqlite::Connection;

use crate::db::repositories::ownership_repo;
use crate::errors::{GroveError, GroveResult};

/// High-level ownership registry backed by the DB.
///
/// Unlike the raw functions in `ownership/mod.rs`, `OwnershipRegistry`
/// uses the repo layer and distinguishes between "same session already
/// holds this lock" (idempotent OK) and "a different session holds it"
/// (hard conflict error).
pub struct OwnershipRegistry<'a> {
    conn: &'a mut Connection,
}

impl<'a> OwnershipRegistry<'a> {
    pub fn new(conn: &'a mut Connection) -> Self {
        Self { conn }
    }

    /// Attempt to acquire the lock for `path` within `run_id` by `session_id`.
    ///
    /// - If this session already holds the lock: returns `Ok(())` (idempotent).
    /// - If a *different* session holds the lock: returns `Err` with a conflict message.
    /// - If no lock exists: acquires and returns `Ok(())`.
    pub fn try_acquire(&mut self, run_id: &str, path: &str, session_id: &str) -> GroveResult<()> {
        let now = Utc::now().to_rfc3339();
        let acquired = ownership_repo::acquire(self.conn, run_id, path, session_id, &now)?;

        if acquired {
            return Ok(());
        }

        // Lock was not inserted — check who holds it.
        let holder = ownership_repo::current_holder(self.conn, run_id, path)?;
        match holder {
            Some(ref h) if h == session_id => Ok(()), // same session — idempotent
            Some(other) => Err(GroveError::Runtime(format!(
                "ownership conflict: path '{path}' is locked by session '{other}' \
                 (requested by '{session_id}')"
            ))),
            None => {
                // Race: lock was released between INSERT OR IGNORE and the query — retry once.
                let acquired2 = ownership_repo::acquire(self.conn, run_id, path, session_id, &now)?;
                if acquired2 {
                    Ok(())
                } else {
                    Err(GroveError::Runtime(format!(
                        "ownership conflict: could not acquire lock for path '{path}'"
                    )))
                }
            }
        }
    }

    /// Release the lock for `path` held by `session_id`.
    pub fn release(&self, run_id: &str, path: &str, session_id: &str) -> GroveResult<()> {
        ownership_repo::release(self.conn, run_id, path, session_id)?;
        Ok(())
    }

    /// Release all locks held by `session_id` — called on session end.
    pub fn release_all(&self, session_id: &str) -> GroveResult<usize> {
        ownership_repo::release_all_for_session(self.conn, session_id)
    }
}
