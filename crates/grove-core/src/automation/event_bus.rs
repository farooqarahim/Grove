use serde::{Deserialize, Serialize};

/// Events flowing through the automation system.
///
/// Three categories:
/// - **Trigger events** — emitted when a schedule fires, a webhook arrives, or a user clicks "run now".
/// - **Workflow lifecycle** — emitted by the automation engine as it creates runs and advances steps.
/// - **Bridge events** — re-published from the existing task/run system so the automation engine
///   can react to task completions without polling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AutomationEvent {
    // ── Triggers ─────────────────────────────────────────────────────────
    ScheduleTriggered {
        automation_id: String,
        scheduled_at: String,
    },
    WebhookReceived {
        automation_id: String,
        payload: serde_json::Value,
    },
    ManualTriggered {
        automation_id: String,
        triggered_by: String,
    },

    // ── Workflow lifecycle ───────────────────────────────────────────────
    RunStarted {
        automation_run_id: String,
        automation_id: String,
    },
    StepQueued {
        automation_run_id: String,
        step_key: String,
        task_id: String,
    },
    StepCompleted {
        automation_run_id: String,
        step_key: String,
        task_id: String,
        run_id: Option<String>,
    },
    StepFailed {
        automation_run_id: String,
        step_key: String,
        error: String,
    },
    StepSkipped {
        automation_run_id: String,
        step_key: String,
        reason: String,
    },
    RunCompleted {
        automation_run_id: String,
    },
    RunFailed {
        automation_run_id: String,
        error: String,
    },

    // ── Bridge from existing task system ─────────────────────────────────
    TaskFinished {
        task_id: String,
        state: String,
        run_id: Option<String>,
    },
}

/// Central pub/sub hub for [`AutomationEvent`]s.
///
/// Backed by a `tokio::sync::broadcast` channel so multiple subscribers (workflow engine,
/// notifier, UI, etc.) each receive every event independently.
pub struct EventBus {
    sender: tokio::sync::broadcast::Sender<AutomationEvent>,
}

impl EventBus {
    /// Create an event bus with a custom channel capacity.
    ///
    /// The capacity determines how many unread events can be buffered per
    /// slow subscriber before it starts losing (lagging) messages.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(capacity);
        Self { sender }
    }

    /// Create an event bus with the default capacity of 256 events.
    pub fn with_default_capacity() -> Self {
        Self::new(256)
    }

    /// Publish an event to all current subscribers.
    ///
    /// If no subscribers are listening the event is silently dropped (logged at debug level).
    pub fn publish(&self, event: AutomationEvent) {
        if let Err(e) = self.sender.send(event) {
            tracing::debug!(error = %e, "no active subscribers for automation event");
        }
    }

    /// Create a new subscription.
    ///
    /// The returned receiver will see every event published **after** this call.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AutomationEvent> {
        self.sender.subscribe()
    }
}
