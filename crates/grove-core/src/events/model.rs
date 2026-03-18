use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single event row as returned from the DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRow {
    pub id: i64,
    pub run_id: String,
    pub session_id: Option<String>,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}
