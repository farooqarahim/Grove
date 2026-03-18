use chrono::Utc;
use cron::Schedule;
use std::str::FromStr;

use crate::errors::{GroveError, GroveResult};

/// Parse a cron expression string into a [`Schedule`].
///
/// The `cron` crate uses a 7-field format:
/// `sec min hour day_of_month month day_of_week year`
pub fn parse_cron_schedule(schedule: &str) -> GroveResult<Schedule> {
    Schedule::from_str(schedule)
        .map_err(|e| GroveError::Runtime(format!("invalid cron expression '{}': {}", schedule, e)))
}

/// Returns the next upcoming occurrence for a cron schedule (from now).
pub fn next_cron_occurrence(schedule: &Schedule) -> Option<chrono::DateTime<Utc>> {
    schedule.upcoming(Utc).next()
}

/// Returns `true` if the cron schedule has a due occurrence since `last_triggered`.
///
/// - If `last_triggered` is `None`, the schedule is considered never-triggered and
///   this returns `true` (so the first run fires immediately).
/// - If `last_triggered` is `Some`, checks whether any scheduled occurrence falls
///   between `last_triggered` and now.
pub fn is_cron_due(schedule: &Schedule, last_triggered: Option<&str>) -> bool {
    let last = last_triggered
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let now = Utc::now();

    match last {
        Some(last_dt) => {
            // Check if there's any occurrence between last_triggered and now.
            schedule
                .after(&last_dt)
                .next()
                .is_some_and(|next| next <= now)
        }
        None => {
            // Never triggered — always due so the first run fires immediately.
            true
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_cron() {
        // Every Monday at 9:00:00 AM (7-field: sec min hour dom month dow year)
        let result = parse_cron_schedule("0 0 9 * * MON *");
        assert!(result.is_ok(), "expected Ok, got: {:?}", result);
    }

    #[test]
    fn parse_invalid_cron() {
        let result = parse_cron_schedule("not a cron expression");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("invalid cron expression"),
            "expected 'invalid cron expression' in: {err_msg}"
        );
    }

    #[test]
    fn is_cron_due_never_triggered() {
        let schedule = parse_cron_schedule("0 0 9 * * MON *").unwrap();
        assert!(is_cron_due(&schedule, None));
    }

    #[test]
    fn next_cron_occurrence_returns_some() {
        // Every second — always has a next occurrence.
        let schedule = parse_cron_schedule("* * * * * * *").unwrap();
        let next = next_cron_occurrence(&schedule);
        assert!(next.is_some(), "expected Some for every-second schedule");
    }

    #[test]
    fn is_cron_due_recently_triggered() {
        // Every-second schedule: if triggered right now, next tick hasn't passed yet.
        let schedule = parse_cron_schedule("0 0 9 * * MON *").unwrap();
        let now = Utc::now().to_rfc3339();
        // A weekly schedule triggered just now should not be due yet.
        assert!(!is_cron_due(&schedule, Some(&now)));
    }

    #[test]
    fn is_cron_due_with_old_timestamp() {
        // Every-second schedule: if last triggered a long time ago, should be due.
        let schedule = parse_cron_schedule("* * * * * * *").unwrap();
        let old = "2020-01-01T00:00:00Z";
        assert!(is_cron_due(&schedule, Some(old)));
    }

    #[test]
    fn is_cron_due_with_invalid_timestamp_treated_as_never() {
        let schedule = parse_cron_schedule("* * * * * * *").unwrap();
        // Invalid RFC3339 → parsed as None → treated as never triggered → due.
        assert!(is_cron_due(&schedule, Some("not-a-timestamp")));
    }
}
