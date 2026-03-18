use rusqlite::Connection;

use crate::budget::policy;
use crate::errors::GroveResult;

use super::ProviderResponse;

/// Extract the reported cost from a provider response (0.0 if not present).
pub fn cost_from_response(response: &ProviderResponse) -> f64 {
    response.cost_usd.unwrap_or(0.0)
}

/// Record the cost of a provider response against `run_id` and return the
/// updated budget status in a single DB round-trip via `record_spend_and_check`.
pub fn record(
    conn: &Connection,
    run_id: &str,
    response: &ProviderResponse,
) -> GroveResult<policy::BudgetStatus> {
    let amount = cost_from_response(response);
    policy::record_spend_and_check(conn, run_id, amount)
}
