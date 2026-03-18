use chrono::Utc;
use rusqlite::Connection;

use crate::errors::{GroveError, GroveResult};

#[derive(Debug, PartialEq)]
pub enum BudgetStatus {
    /// Spending is within the warning threshold.
    Ok { remaining_usd: f64 },
    /// Spending is above the warning threshold but below the hard stop.
    Warning {
        remaining_usd: f64,
        percent_used: f64,
    },
    /// Hard stop threshold reached or exceeded.
    Exceeded { used_usd: f64, limit_usd: f64 },
}

/// Check the current budget status for a run.
pub fn check_budget(conn: &Connection, run_id: &str) -> GroveResult<BudgetStatus> {
    let (budget_usd, cost_used_usd): (f64, f64) = conn.query_row(
        "SELECT budget_usd, cost_used_usd FROM runs WHERE id=?1",
        [run_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    classify_budget(budget_usd, cost_used_usd, run_id)
}

/// Add `amount_usd` to the run's `cost_used_usd`.
///
/// The total is capped at `budget_usd`. Callers should call `check_budget`
/// immediately after and trigger a hard stop if `Exceeded` is returned.
pub fn record_spend(conn: &Connection, run_id: &str, amount_usd: f64) -> GroveResult<()> {
    let now = Utc::now().to_rfc3339();
    let n = conn.execute(
        "UPDATE runs
         SET cost_used_usd = MIN(budget_usd, cost_used_usd + ?1),
             updated_at    = ?2
         WHERE id = ?3",
        rusqlite::params![amount_usd, now, run_id],
    )?;
    if n == 0 {
        return Err(GroveError::NotFound(format!("run {run_id}")));
    }
    Ok(())
}

/// Record `amount_usd` against `run_id` and return the updated `BudgetStatus`
/// in a **single round-trip** via `UPDATE ... RETURNING`.
///
/// This replaces the two-step `record_spend` + `check_budget` pattern used on
/// the hot path after every agent response.
pub fn record_spend_and_check(
    conn: &Connection,
    run_id: &str,
    amount_usd: f64,
) -> GroveResult<BudgetStatus> {
    if amount_usd > 0.0 {
        let now = Utc::now().to_rfc3339();
        let result = conn.query_row(
            "UPDATE runs
             SET cost_used_usd = MIN(budget_usd, cost_used_usd + ?1),
                 updated_at    = ?2
             WHERE id = ?3
             RETURNING budget_usd, cost_used_usd",
            rusqlite::params![amount_usd, now, run_id],
            |r| Ok((r.get::<_, f64>(0)?, r.get::<_, f64>(1)?)),
        );
        match result {
            Ok((budget_usd, cost_used_usd)) => classify_budget(budget_usd, cost_used_usd, run_id),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Err(GroveError::NotFound(format!("run {run_id}")))
            }
            Err(e) => Err(GroveError::Database(e)),
        }
    } else {
        check_budget(conn, run_id)
    }
}

fn classify_budget(budget_usd: f64, cost_used_usd: f64, run_id: &str) -> GroveResult<BudgetStatus> {
    if budget_usd <= 0.0 {
        return Err(GroveError::Config(format!(
            "run {run_id} has non-positive budget_usd ({budget_usd})"
        )));
    }
    let percent_used = (cost_used_usd / budget_usd) * 100.0;
    let remaining = (budget_usd - cost_used_usd).max(0.0);
    if percent_used >= 100.0 {
        Ok(BudgetStatus::Exceeded {
            used_usd: cost_used_usd,
            limit_usd: budget_usd,
        })
    } else if percent_used >= 80.0 {
        Ok(BudgetStatus::Warning {
            remaining_usd: remaining,
            percent_used,
        })
    } else {
        Ok(BudgetStatus::Ok {
            remaining_usd: remaining,
        })
    }
}
