use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// Sub-modules
pub mod conditions;
pub mod engine;
pub mod event_bus;
pub mod markdown;
pub mod notifier;
pub mod scheduler;
pub mod triggers;
pub mod webhook;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationDef {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub trigger: TriggerConfig,
    pub defaults: AutomationDefaults,
    pub session_mode: SessionMode,
    pub dedicated_conversation_id: Option<String>,
    pub source_path: Option<String>,
    pub last_triggered_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub notifications: Option<NotificationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerConfig {
    Cron {
        schedule: String,
    },
    Event {
        event_type: String,
        filter: Option<serde_json::Value>,
    },
    Manual,
    Webhook {
        filter: Option<serde_json::Value>,
    },
    /// Trigger when issues matching the filter move to a target status.
    /// Uses a cron schedule to poll the issue tracker.
    Issue {
        /// Cron expression for how often to poll (e.g., "*/5 * * * *").
        schedule: String,
        /// Which statuses to watch for (e.g., ["ready", "in_progress"]).
        statuses: Vec<String>,
        /// Optional label filter — only issues with ALL of these labels trigger.
        #[serde(default)]
        labels: Vec<String>,
    },
}

impl TriggerConfig {
    pub fn type_str(&self) -> &'static str {
        match self {
            TriggerConfig::Cron { .. } => "cron",
            TriggerConfig::Event { .. } => "event",
            TriggerConfig::Manual => "manual",
            TriggerConfig::Webhook { .. } => "webhook",
            TriggerConfig::Issue { .. } => "issue",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AutomationDefaults {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub budget_usd: Option<f64>,
    pub pipeline: Option<String>,
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    New,
    Dedicated,
}

impl Default for SessionMode {
    fn default() -> Self {
        Self::New
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationStep {
    pub id: String,
    pub automation_id: String,
    pub step_key: String,
    pub ordinal: i32,
    pub objective: String,
    pub depends_on: Vec<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub budget_usd: Option<f64>,
    pub pipeline: Option<String>,
    pub permission_mode: Option<String>,
    pub condition: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRun {
    pub id: String,
    pub automation_id: String,
    pub state: AutomationRunState,
    pub trigger_info: Option<serde_json::Value>,
    pub conversation_id: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunState {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AutomationRunState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRunStep {
    pub id: String,
    pub automation_run_id: String,
    pub step_id: String,
    pub step_key: String,
    pub state: StepState,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub condition_result: Option<bool>,
    pub error: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl StepState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "queued" => Some(Self::Queued),
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "skipped" => Some(Self::Skipped),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Skipped)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    #[serde(default)]
    pub on_success: Vec<NotificationTarget>,
    #[serde(default)]
    pub on_failure: Vec<NotificationTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NotificationTarget {
    Slack {
        webhook_url: String,
        channel: Option<String>,
    },
    System,
    Webhook {
        url: String,
        headers: Option<HashMap<String, String>>,
    },
}
