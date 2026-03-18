use crate::config::GroveConfig;

use super::queue::MergeEntry;

#[derive(Debug, PartialEq)]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
}

/// Decide whether a merge entry is eligible to proceed.
///
/// Checks (in order):
/// 1. Budget remaining must be > 0.
/// 2. Entry must still be in `queued` or `running` state.
pub fn can_merge(
    entry: &MergeEntry,
    _cfg: &GroveConfig,
    budget_remaining_usd: f64,
) -> PolicyDecision {
    if budget_remaining_usd <= 0.0 {
        return PolicyDecision::Deny {
            reason: format!(
                "budget exhausted (remaining: ${:.4}); cannot proceed with merge",
                budget_remaining_usd
            ),
        };
    }

    if entry.status != "queued" && entry.status != "running" {
        return PolicyDecision::Deny {
            reason: format!(
                "merge entry {} has status '{}'; only queued/running entries can merge",
                entry.id, entry.status
            ),
        };
    }

    PolicyDecision::Allow
}
