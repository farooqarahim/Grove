use std::sync::Arc;

use crate::automation::NotificationTarget;
use crate::automation::event_bus::{AutomationEvent, EventBus};
use crate::db::DbHandle;
use crate::db::repositories::automations_repo;
use crate::errors::{GroveError, GroveResult};

/// Subscribes to the automation [`EventBus`] and dispatches notifications
/// when automation runs complete or fail.
///
/// Supported notification targets:
/// - **Slack** — posts a message to a Slack incoming-webhook URL.
/// - **System** — shows a desktop notification via `notify-rust`.
/// - **Custom webhook** — POSTs a JSON payload to an arbitrary URL.
pub struct Notifier {
    event_bus: Arc<EventBus>,
    db_handle: DbHandle,
}

impl Notifier {
    pub fn new(event_bus: Arc<EventBus>, db_handle: DbHandle) -> Self {
        Self {
            event_bus,
            db_handle,
        }
    }

    /// Run the notification listener loop.
    ///
    /// This method runs indefinitely. It should be spawned as a background
    /// tokio task via `tokio::spawn(notifier.run())`.
    pub async fn run(self: Arc<Self>) {
        tracing::info!("notifier started");
        let mut rx = self.event_bus.subscribe();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(e) = self.handle_event(&event).await {
                        tracing::error!(error = %e, "notifier failed to handle event");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(missed = n, "notifier lagged behind event bus");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("event bus closed, notifier shutting down");
                    break;
                }
            }
        }
    }

    async fn handle_event(&self, event: &AutomationEvent) -> GroveResult<()> {
        match event {
            AutomationEvent::RunCompleted {
                automation_run_id, ..
            } => self.notify_run_outcome(automation_run_id, true, None).await,
            AutomationEvent::RunFailed {
                automation_run_id,
                error,
            } => {
                self.notify_run_outcome(automation_run_id, false, Some(error))
                    .await
            }
            _ => Ok(()), // other events are not notification-worthy
        }
    }

    async fn notify_run_outcome(
        &self,
        automation_run_id: &str,
        success: bool,
        error: Option<&String>,
    ) -> GroveResult<()> {
        let conn = self.db_handle.connect()?;
        let run = automations_repo::get_run(&conn, automation_run_id)?;
        let automation = automations_repo::get_automation(&conn, &run.automation_id)?;

        let notification_config = automation.notifications.unwrap_or_default();
        let targets = if success {
            &notification_config.on_success
        } else {
            &notification_config.on_failure
        };

        if targets.is_empty() {
            return Ok(());
        }

        let message = if success {
            format!(
                "Automation '{}' completed successfully. Run: {}",
                automation.name, automation_run_id
            )
        } else {
            format!(
                "Automation '{}' failed. Run: {}. Error: {}",
                automation.name,
                automation_run_id,
                error.map_or("unknown", String::as_str)
            )
        };

        let title = if success {
            format!("Grove: {} completed", automation.name)
        } else {
            format!("Grove: {} failed", automation.name)
        };

        for target in targets {
            if let Err(e) = self.dispatch(target, &title, &message).await {
                tracing::error!(
                    target = ?target,
                    error = %e,
                    "failed to send notification"
                );
            }
        }

        Ok(())
    }

    async fn dispatch(
        &self,
        target: &NotificationTarget,
        title: &str,
        message: &str,
    ) -> GroveResult<()> {
        match target {
            NotificationTarget::Slack {
                webhook_url,
                channel,
            } => send_slack(webhook_url, channel.as_deref(), message).await,
            NotificationTarget::System => send_system_notification(title, message),
            NotificationTarget::Webhook { url, headers } => {
                send_custom_webhook(url, headers.as_ref(), message).await
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Notification dispatchers
// ---------------------------------------------------------------------------

async fn send_slack(webhook_url: &str, channel: Option<&str>, message: &str) -> GroveResult<()> {
    let mut payload = serde_json::json!({ "text": message });
    if let Some(ch) = channel {
        payload["channel"] = serde_json::Value::String(ch.to_string());
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(webhook_url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| GroveError::Runtime(format!("slack webhook failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(GroveError::Runtime(format!(
            "slack webhook returned status: {}",
            resp.status()
        )));
    }
    Ok(())
}

fn send_system_notification(title: &str, body: &str) -> GroveResult<()> {
    notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show()
        .map_err(|e| GroveError::Runtime(format!("system notification failed: {e}")))?;
    Ok(())
}

async fn send_custom_webhook(
    url: &str,
    headers: Option<&std::collections::HashMap<String, String>>,
    message: &str,
) -> GroveResult<()> {
    let payload = serde_json::json!({
        "text": message,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let client = reqwest::Client::new();
    let mut req = client
        .post(url)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(10));

    if let Some(hdrs) = headers {
        for (k, v) in hdrs {
            req = req.header(k.as_str(), v.as_str());
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| GroveError::Runtime(format!("custom webhook failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(GroveError::Runtime(format!(
            "custom webhook returned status: {}",
            resp.status()
        )));
    }
    Ok(())
}
