use std::sync::Arc;
use std::time::Duration;

use crate::automation::TriggerConfig;
use crate::automation::engine::WorkflowEngine;
use crate::automation::event_bus::{AutomationEvent, EventBus};
use crate::automation::triggers::{is_cron_due, parse_cron_schedule};
use crate::db::DbHandle;
use crate::db::repositories::automations_repo;

/// Polls enabled cron and issue-triggered automations and triggers them when due.
///
/// Both cron and issue triggers use cron expressions for scheduling.
/// The scheduler is designed to be spawned as a background tokio task via
/// `tokio::spawn(scheduler.run())`.
pub struct CronScheduler {
    engine: Arc<WorkflowEngine>,
    event_bus: Arc<EventBus>,
    db_handle: DbHandle,
}

impl CronScheduler {
    pub fn new(engine: Arc<WorkflowEngine>, event_bus: Arc<EventBus>, db_handle: DbHandle) -> Self {
        Self {
            engine,
            event_bus,
            db_handle,
        }
    }

    /// Run the scheduler loop. Checks every 60 seconds for due automations.
    /// This should be spawned as a background tokio task.
    pub async fn run(self: Arc<Self>) {
        tracing::info!("cron scheduler started");
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = self.poll_cron() {
                tracing::error!(error = %e, "cron scheduler poll failed");
            }
            if let Err(e) = self.poll_issues() {
                tracing::error!(error = %e, "issue scanner poll failed");
            }
        }
    }

    /// Single poll iteration -- check all enabled cron automations and trigger due ones.
    fn poll_cron(&self) -> crate::errors::GroveResult<()> {
        let conn = self.db_handle.connect()?;
        let automations = automations_repo::list_enabled_cron_automations(&conn)?;

        for automation in automations {
            let schedule_str = match &automation.trigger {
                TriggerConfig::Cron { schedule } => schedule.clone(),
                _ => continue,
            };

            let schedule = match parse_cron_schedule(&schedule_str) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        automation_id = %automation.id,
                        schedule = %schedule_str,
                        error = %e,
                        "skipping automation with invalid cron schedule"
                    );
                    continue;
                }
            };

            if !is_cron_due(&schedule, automation.last_triggered_at.as_deref()) {
                continue;
            }

            tracing::info!(
                automation_id = %automation.id,
                name = %automation.name,
                "cron automation is due, triggering"
            );

            automations_repo::update_last_triggered(&conn, &automation.id)?;

            self.event_bus.publish(AutomationEvent::ScheduleTriggered {
                automation_id: automation.id.clone(),
                scheduled_at: chrono::Utc::now().to_rfc3339(),
            });

            match self.engine.start_run(
                &automation.id,
                serde_json::json!({
                    "trigger": "cron",
                    "schedule": schedule_str,
                }),
            ) {
                Ok(run_id) => {
                    tracing::info!(
                        automation_id = %automation.id,
                        run_id = %run_id,
                        "cron automation run started"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        automation_id = %automation.id,
                        error = %e,
                        "failed to start cron automation run"
                    );
                }
            }
        }
        Ok(())
    }

    /// Poll issue-triggered automations using their cron schedule.
    ///
    /// For each enabled issue-trigger automation whose cron schedule is due,
    /// checks the project's issues in the DB that match the configured
    /// statuses + labels. If any matching issues are found (that haven't
    /// already been processed), triggers a run for each with the issue data
    /// embedded in trigger_info for template variable interpolation.
    fn poll_issues(&self) -> crate::errors::GroveResult<()> {
        let conn = self.db_handle.connect()?;
        let automations = automations_repo::list_enabled_issue_automations(&conn)?;

        if automations.is_empty() {
            return Ok(());
        }

        for automation in automations {
            let (schedule_str, statuses, labels) = match &automation.trigger {
                TriggerConfig::Issue {
                    schedule,
                    statuses,
                    labels,
                } => (schedule.clone(), statuses.clone(), labels.clone()),
                _ => continue,
            };

            // Use the same cron evaluation as regular cron triggers.
            let schedule = match parse_cron_schedule(&schedule_str) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        automation_id = %automation.id,
                        schedule = %schedule_str,
                        error = %e,
                        "skipping issue automation with invalid cron schedule"
                    );
                    continue;
                }
            };

            if !is_cron_due(&schedule, automation.last_triggered_at.as_deref()) {
                continue;
            }

            // Query issues for this project that match the target statuses.
            let issues: Vec<serde_json::Value> = {
                let mut stmt = conn.prepare(
                    "SELECT id, title, body, status, labels, external_url
                     FROM issues
                     WHERE project_id = ?1 AND status IN (SELECT value FROM json_each(?2))",
                )?;
                let statuses_json = serde_json::to_string(&statuses).unwrap_or_default();
                let rows = stmt.query_map(
                    rusqlite::params![automation.project_id, statuses_json],
                    |row| {
                        let id: String = row.get(0)?;
                        let title: String = row.get(1)?;
                        let body: Option<String> = row.get(2)?;
                        let status: String = row.get(3)?;
                        let labels_str: Option<String> = row.get(4)?;
                        let url: Option<String> = row.get(5)?;
                        Ok(serde_json::json!({
                            "id": id,
                            "title": title,
                            "body": body.unwrap_or_default(),
                            "status": status,
                            "labels": labels_str.unwrap_or_default(),
                            "url": url.unwrap_or_default(),
                        }))
                    },
                )?;
                rows.filter_map(|r| r.ok()).collect()
            };

            // Filter by label requirements if specified.
            let matching: Vec<&serde_json::Value> = if labels.is_empty() {
                issues.iter().collect()
            } else {
                issues
                    .iter()
                    .filter(|issue| {
                        let issue_labels =
                            issue.get("labels").and_then(|l| l.as_str()).unwrap_or("");
                        labels.iter().all(|required| {
                            issue_labels
                                .split(',')
                                .any(|l| l.trim().eq_ignore_ascii_case(required))
                        })
                    })
                    .collect()
            };

            if matching.is_empty() {
                // Still update last_triggered so we don't re-check immediately.
                automations_repo::update_last_triggered(&conn, &automation.id)?;
                continue;
            }

            // Skip issues that already have a running automation run for this automation.
            for issue in matching {
                let issue_id = issue.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if issue_id.is_empty() {
                    continue;
                }

                // Check for existing active run for this issue.
                let already_running: bool = {
                    let mut check_stmt = conn.prepare(
                        "SELECT COUNT(*) FROM automation_runs
                         WHERE automation_id = ?1
                           AND state IN ('pending', 'running')
                           AND trigger_info LIKE ?2",
                    )?;
                    let pattern = format!("%\"id\":\"{}\"%%", issue_id);
                    let count: i64 = check_stmt
                        .query_row(rusqlite::params![automation.id, pattern], |row| row.get(0))
                        .unwrap_or(0);
                    count > 0
                };

                if already_running {
                    continue;
                }

                let issue_title = issue
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                tracing::info!(
                    automation_id = %automation.id,
                    issue_id = issue_id,
                    issue_title = issue_title,
                    "issue trigger matched, starting automation run"
                );

                automations_repo::update_last_triggered(&conn, &automation.id)?;

                match self.engine.start_run(
                    &automation.id,
                    serde_json::json!({
                        "trigger": "issue",
                        "issue": issue,
                    }),
                ) {
                    Ok(run_id) => {
                        tracing::info!(
                            automation_id = %automation.id,
                            run_id = %run_id,
                            issue_id = issue_id,
                            "issue-triggered automation run started"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            automation_id = %automation.id,
                            issue_id = issue_id,
                            error = %e,
                            "failed to start issue-triggered automation run"
                        );
                    }
                }
            }
        }

        Ok(())
    }
}
