use serde::{Deserialize, Serialize};

use crate::errors::GroveResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetSnapshot {
    pub allocated_usd: f64,
    pub used_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipSnapshot {
    pub path: String,
    pub owner: String,
}

/// Full state snapshot persisted at each checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointPayload {
    pub run_id: String,
    pub stage: String,
    pub active_sessions: Vec<String>,
    pub pending_tasks: Vec<String>,
    pub ownership: Vec<OwnershipSnapshot>,
    pub budget: BudgetSnapshot,
}

impl CheckpointPayload {
    pub fn to_json(&self) -> GroveResult<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn from_json(s: &str) -> GroveResult<Self> {
        Ok(serde_json::from_str(s)?)
    }
}
