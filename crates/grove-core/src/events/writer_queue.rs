use rusqlite::{Connection, TransactionBehavior, params};

use crate::errors::GroveResult;

/// A pending event waiting to be flushed to the DB.
#[derive(Debug, Clone)]
pub struct PendingEvent {
    pub run_id: String,
    pub session_id: Option<String>,
    pub event_type: String,
    pub payload_json: String,
    pub created_at: String,
}

/// Buffers event writes and flushes them all in a single `BEGIN IMMEDIATE`
/// transaction to avoid per-event write contention on SQLite.
#[derive(Default)]
pub struct WriterQueue {
    buffer: Vec<PendingEvent>,
}

impl WriterQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stage an event for the next flush.
    pub fn push(&mut self, event: PendingEvent) {
        self.buffer.push(event);
    }

    /// Write all buffered events to `conn` in a single transaction.
    /// Returns the number of rows inserted. Clears the buffer on success.
    pub fn flush(&mut self, conn: &mut Connection) -> GroveResult<usize> {
        if self.buffer.is_empty() {
            return Ok(0);
        }

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for ev in &self.buffer {
            tx.execute(
                "INSERT INTO events (run_id, session_id, type, payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    ev.run_id,
                    ev.session_id,
                    ev.event_type,
                    ev.payload_json,
                    ev.created_at,
                ],
            )?;
        }
        let n = self.buffer.len();
        tx.commit()?;
        self.buffer.clear();
        Ok(n)
    }

    /// Number of events staged but not yet flushed.
    pub fn pending_count(&self) -> usize {
        self.buffer.len()
    }
}
