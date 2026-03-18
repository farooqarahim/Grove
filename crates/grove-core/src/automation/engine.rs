use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::automation::conditions::evaluate_condition;
use crate::automation::event_bus::{AutomationEvent, EventBus};
use crate::automation::{AutomationRun, AutomationRunState, AutomationRunStep, StepState};
use crate::db::DbHandle;
use crate::db::repositories::automations_repo;
use crate::errors::{GroveError, GroveResult};

/// DAG executor for automation workflows.
///
/// The engine creates automation runs, advances steps by evaluating dependency
/// graphs and conditions, and bridges task completions back into the DAG to
/// drive it to completion.
pub struct WorkflowEngine {
    event_bus: Arc<EventBus>,
    db_handle: DbHandle,
}

impl WorkflowEngine {
    pub fn new(event_bus: Arc<EventBus>, db_handle: DbHandle) -> Self {
        Self {
            event_bus,
            db_handle,
        }
    }

    /// Create and start an automation run.
    ///
    /// Loads the automation definition, verifies it is enabled, creates the run
    /// and all initial run-step records, then kicks off the DAG by advancing
    /// any steps whose dependencies are already satisfied.
    pub fn start_run(
        &self,
        automation_id: &str,
        trigger_info: serde_json::Value,
    ) -> GroveResult<String> {
        let conn = self.db_handle.connect()?;

        // Load and validate the automation definition.
        let automation = automations_repo::get_automation(&conn, automation_id)?;
        if !automation.enabled {
            return Err(GroveError::Runtime(format!(
                "automation {} is disabled",
                automation_id
            )));
        }

        // Load the step definitions for this automation.
        let steps = automations_repo::list_steps(&conn, automation_id)?;

        // Generate a unique run id.
        let run_id = format!(
            "arun_{}_{}",
            Utc::now().format("%Y%m%d_%H%M%S"),
            &Uuid::new_v4().simple().to_string()[..8]
        );
        let now = Utc::now().to_rfc3339();

        // Create the run record.
        let run = AutomationRun {
            id: run_id.clone(),
            automation_id: automation_id.to_string(),
            state: AutomationRunState::Running,
            trigger_info: Some(trigger_info),
            conversation_id: None,
            started_at: Some(now.clone()),
            completed_at: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        automations_repo::insert_run(&conn, &run)?;

        // Create a run-step record for each step definition.
        for step in &steps {
            let run_step_id = format!("ars_{}_{}", &run_id[5..], step.step_key);
            let run_step = AutomationRunStep {
                id: run_step_id,
                automation_run_id: run_id.clone(),
                step_id: step.id.clone(),
                step_key: step.step_key.clone(),
                state: StepState::Pending,
                task_id: None,
                run_id: None,
                condition_result: None,
                error: None,
                started_at: None,
                completed_at: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            automations_repo::insert_run_step(&conn, &run_step)?;
        }

        self.event_bus.publish(AutomationEvent::RunStarted {
            automation_run_id: run_id.clone(),
            automation_id: automation_id.to_string(),
        });

        tracing::info!(
            automation_id = automation_id,
            run_id = %run_id,
            step_count = steps.len(),
            "automation run started"
        );

        // Kick off the DAG — queue any steps whose dependencies are met.
        self.advance_dag(&conn, &run_id)?;

        Ok(run_id)
    }

    /// Advance the DAG by queuing any steps that are ready to run.
    ///
    /// A step is "ready" when:
    /// 1. Its state is `Pending`.
    /// 2. Every step key listed in its `depends_on` has reached a terminal
    ///    state (`Completed` or `Skipped`).
    ///
    /// Ready steps with a condition expression are evaluated — if the condition
    /// is false the step is marked `Skipped`. Otherwise a task is queued into
    /// the Grove task system and the step moves to `Queued`.
    ///
    /// After processing ready steps, if every step in the run has reached a
    /// terminal state the run itself is marked `Completed` or `Failed`.
    fn advance_dag(&self, conn: &rusqlite::Connection, automation_run_id: &str) -> GroveResult<()> {
        let run_steps = automations_repo::list_run_steps(conn, automation_run_id)?;

        // Build a lookup: step_key -> current state string.
        let step_states: HashMap<String, String> = run_steps
            .iter()
            .map(|rs| (rs.step_key.clone(), rs.state.as_str().to_string()))
            .collect();

        // Load the run to get the automation_id, then load steps for dependency info.
        let run = automations_repo::get_run(conn, automation_run_id)?;
        let step_defs = automations_repo::list_steps(conn, &run.automation_id)?;
        let step_def_map: HashMap<String, _> = step_defs
            .into_iter()
            .map(|s| (s.step_key.clone(), s))
            .collect();

        // Load the automation definition for defaults.
        let automation = automations_repo::get_automation(conn, &run.automation_id)?;

        let now = Utc::now().to_rfc3339();

        // Find steps that are ready to advance.
        for mut run_step in run_steps.iter().cloned() {
            if run_step.state != StepState::Pending {
                continue;
            }

            let step_def = match step_def_map.get(&run_step.step_key) {
                Some(def) => def,
                None => {
                    tracing::error!(
                        step_key = %run_step.step_key,
                        "run step references unknown step definition; skipping"
                    );
                    continue;
                }
            };

            // Check that all dependencies have reached a terminal state that
            // allows downstream execution (Completed or Skipped).
            let deps_satisfied = step_def.depends_on.iter().all(|dep_key| {
                step_states
                    .get(dep_key)
                    .and_then(|s| StepState::parse(s))
                    .map(|st| st == StepState::Completed || st == StepState::Skipped)
                    .unwrap_or(false)
            });
            if !deps_satisfied {
                continue;
            }

            // Evaluate optional condition expression.
            if let Some(ref condition) = step_def.condition {
                match evaluate_condition(condition, &step_states) {
                    Ok(true) => {
                        run_step.condition_result = Some(true);
                    }
                    Ok(false) => {
                        run_step.state = StepState::Skipped;
                        run_step.condition_result = Some(false);
                        run_step.completed_at = Some(now.clone());
                        run_step.updated_at = now.clone();
                        automations_repo::update_run_step(conn, &run_step)?;

                        self.event_bus.publish(AutomationEvent::StepSkipped {
                            automation_run_id: automation_run_id.to_string(),
                            step_key: run_step.step_key.clone(),
                            reason: format!("condition evaluated to false: {condition}"),
                        });

                        tracing::info!(
                            run_id = automation_run_id,
                            step_key = %run_step.step_key,
                            "step skipped — condition false"
                        );
                        continue;
                    }
                    Err(e) => {
                        run_step.state = StepState::Failed;
                        run_step.error = Some(format!("condition evaluation error: {e}"));
                        run_step.completed_at = Some(now.clone());
                        run_step.updated_at = now.clone();
                        automations_repo::update_run_step(conn, &run_step)?;

                        self.event_bus.publish(AutomationEvent::StepFailed {
                            automation_run_id: automation_run_id.to_string(),
                            step_key: run_step.step_key.clone(),
                            error: format!("condition evaluation error: {e}"),
                        });

                        tracing::error!(
                            run_id = automation_run_id,
                            step_key = %run_step.step_key,
                            error = %e,
                            "step failed — condition evaluation error"
                        );
                        continue;
                    }
                }
            }

            // Resolve provider/model/budget/pipeline/permission_mode:
            // step overrides take precedence over automation defaults.
            let provider = step_def
                .provider
                .as_deref()
                .or(automation.defaults.provider.as_deref());
            let model = step_def
                .model
                .as_deref()
                .or(automation.defaults.model.as_deref());
            let budget = step_def.budget_usd.or(automation.defaults.budget_usd);
            let pipeline = step_def
                .pipeline
                .as_deref()
                .or(automation.defaults.pipeline.as_deref());
            let permission_mode = step_def
                .permission_mode
                .as_deref()
                .or(automation.defaults.permission_mode.as_deref());

            // Resolve conversation_id: if the automation uses a dedicated
            // conversation, reuse it for every step in every run.
            let conversation_id =
                if automation.session_mode == crate::automation::SessionMode::Dedicated {
                    automation.dedicated_conversation_id.as_deref()
                } else {
                    None
                };

            // Interpolate template variables in the objective if trigger_info
            // contains issue data (e.g. {{issue.title}}, {{issue.body}}).
            let objective = interpolate_trigger_vars(&step_def.objective, &run);

            // Queue the task into the Grove task system.
            let task = crate::orchestrator::insert_queued_task(
                conn,
                &objective,
                budget,
                0, // priority
                model,
                provider,
                conversation_id,
                None, // resume_provider_session_id
                pipeline,
                permission_mode,
                false, // automation steps currently use pipeline defaults
            )?;

            // Update the run step to Queued.
            run_step.state = StepState::Queued;
            run_step.task_id = Some(task.id.clone());
            run_step.started_at = Some(now.clone());
            run_step.updated_at = now.clone();
            automations_repo::update_run_step(conn, &run_step)?;

            self.event_bus.publish(AutomationEvent::StepQueued {
                automation_run_id: automation_run_id.to_string(),
                step_key: run_step.step_key.clone(),
                task_id: task.id.clone(),
            });

            tracing::info!(
                run_id = automation_run_id,
                step_key = %run_step.step_key,
                task_id = %task.id,
                "step queued"
            );
        }

        // Re-check: if all steps are terminal, finalize the run.
        self.maybe_finalize_run(conn, automation_run_id, &now)?;

        Ok(())
    }

    /// Check whether all steps are terminal and, if so, finalize the run.
    fn maybe_finalize_run(
        &self,
        conn: &rusqlite::Connection,
        automation_run_id: &str,
        now: &str,
    ) -> GroveResult<()> {
        let run_steps = automations_repo::list_run_steps(conn, automation_run_id)?;
        let all_terminal = run_steps.iter().all(|rs| rs.state.is_terminal());
        if !all_terminal {
            return Ok(());
        }

        let any_failed = run_steps.iter().any(|rs| rs.state == StepState::Failed);
        if any_failed {
            automations_repo::update_run_state(
                conn,
                automation_run_id,
                AutomationRunState::Failed.as_str(),
                Some(now),
            )?;
            self.event_bus.publish(AutomationEvent::RunFailed {
                automation_run_id: automation_run_id.to_string(),
                error: "one or more steps failed".to_string(),
            });
            tracing::info!(run_id = automation_run_id, "automation run failed");
        } else {
            automations_repo::update_run_state(
                conn,
                automation_run_id,
                AutomationRunState::Completed.as_str(),
                Some(now),
            )?;
            self.event_bus.publish(AutomationEvent::RunCompleted {
                automation_run_id: automation_run_id.to_string(),
            });
            tracing::info!(run_id = automation_run_id, "automation run completed");
        }

        Ok(())
    }

    /// Handle a task completion from the Grove task system.
    ///
    /// Looks up the run step associated with the finished task and updates its
    /// state accordingly. Then re-advances the DAG to queue any newly-ready
    /// downstream steps.
    ///
    /// If `task_id` does not correspond to an automation run step, this is a
    /// no-op (the task was not created by the automation system).
    pub fn handle_task_finished(
        &self,
        task_id: &str,
        state: &str,
        run_id: Option<&str>,
    ) -> GroveResult<()> {
        let conn = self.db_handle.connect()?;

        let run_step = match automations_repo::get_run_step_by_task(&conn, task_id)? {
            Some(rs) => rs,
            None => return Ok(()), // Not an automation task — nothing to do.
        };

        let new_state = match state {
            "completed" => StepState::Completed,
            "failed" | "cancelled" => StepState::Failed,
            other => {
                tracing::error!(
                    task_id = task_id,
                    task_state = other,
                    "unexpected task state mapped to Failed"
                );
                StepState::Failed
            }
        };

        let now = Utc::now().to_rfc3339();
        let mut updated = run_step.clone();
        updated.state = new_state;
        updated.completed_at = Some(now.clone());
        updated.updated_at = now.clone();
        if let Some(rid) = run_id {
            updated.run_id = Some(rid.to_string());
        }
        automations_repo::update_run_step(&conn, &updated)?;

        match new_state {
            StepState::Completed => {
                self.event_bus.publish(AutomationEvent::StepCompleted {
                    automation_run_id: updated.automation_run_id.clone(),
                    step_key: updated.step_key.clone(),
                    task_id: task_id.to_string(),
                    run_id: run_id.map(str::to_string),
                });
                tracing::info!(
                    run_id = %updated.automation_run_id,
                    step_key = %updated.step_key,
                    task_id = task_id,
                    "step completed"
                );
            }
            StepState::Failed => {
                self.event_bus.publish(AutomationEvent::StepFailed {
                    automation_run_id: updated.automation_run_id.clone(),
                    step_key: updated.step_key.clone(),
                    error: format!("task finished with state: {state}"),
                });
                tracing::info!(
                    run_id = %updated.automation_run_id,
                    step_key = %updated.step_key,
                    task_id = task_id,
                    task_state = state,
                    "step failed"
                );
            }
            _ => {}
        }

        // Advance the DAG — downstream steps may now be ready.
        self.advance_dag(&conn, &updated.automation_run_id)?;

        Ok(())
    }

    /// Background event loop that listens for `TaskFinished` events on the
    /// event bus and drives the DAG forward.
    ///
    /// This method runs indefinitely. It should be spawned as a background
    /// tokio task via `tokio::spawn(engine.run_event_loop())`.
    pub async fn run_event_loop(self: Arc<Self>) {
        let mut rx = self.event_bus.subscribe();
        tracing::info!("automation engine event loop started");

        loop {
            match rx.recv().await {
                Ok(AutomationEvent::TaskFinished {
                    task_id,
                    state,
                    run_id,
                }) => {
                    if let Err(e) = self.handle_task_finished(&task_id, &state, run_id.as_deref()) {
                        tracing::error!(
                            task_id = %task_id,
                            error = %e,
                            "error handling task finished event"
                        );
                    }
                }
                Ok(_) => {
                    // Other events are handled by other subsystems (notifier, UI, etc.).
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(
                        missed = n,
                        "automation engine event loop lagged — missed {n} events"
                    );
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("automation engine event loop shutting down — channel closed");
                    break;
                }
            }
        }
    }

    /// Cancel a running automation run.
    ///
    /// Sets the run state to `Cancelled` and marks all non-terminal steps as
    /// `Skipped`.
    pub fn cancel_run(&self, automation_run_id: &str) -> GroveResult<()> {
        let conn = self.db_handle.connect()?;
        let now = Utc::now().to_rfc3339();

        automations_repo::update_run_state(
            &conn,
            automation_run_id,
            AutomationRunState::Cancelled.as_str(),
            Some(&now),
        )?;

        let run_steps = automations_repo::list_run_steps(&conn, automation_run_id)?;
        for mut rs in run_steps {
            if !rs.state.is_terminal() {
                rs.state = StepState::Skipped;
                rs.completed_at = Some(now.clone());
                rs.updated_at = now.clone();
                automations_repo::update_run_step(&conn, &rs)?;
            }
        }

        self.event_bus.publish(AutomationEvent::RunFailed {
            automation_run_id: automation_run_id.to_string(),
            error: "cancelled".to_string(),
        });

        tracing::info!(run_id = automation_run_id, "automation run cancelled");

        Ok(())
    }
}

/// Interpolate `{{issue.title}}`, `{{issue.body}}`, `{{issue.id}}`, and
/// `{{issue.url}}` placeholders in a step objective using data from the
/// automation run's `trigger_info`.
///
/// If `trigger_info` is `None` or doesn't contain the referenced field, the
/// placeholder is left as-is (safe no-op for non-issue triggers).
fn interpolate_trigger_vars(template: &str, run: &AutomationRun) -> String {
    let info = match run.trigger_info.as_ref() {
        Some(v) => v,
        None => return template.to_string(),
    };

    let issue = match info.get("issue") {
        Some(v) => v,
        None => return template.to_string(),
    };

    let mut result = template.to_string();
    let replacements = [
        (
            "{{issue.title}}",
            issue.get("title").and_then(|v| v.as_str()),
        ),
        ("{{issue.body}}", issue.get("body").and_then(|v| v.as_str())),
        ("{{issue.id}}", issue.get("id").and_then(|v| v.as_str())),
        ("{{issue.url}}", issue.get("url").and_then(|v| v.as_str())),
        (
            "{{issue.status}}",
            issue.get("status").and_then(|v| v.as_str()),
        ),
        (
            "{{issue.labels}}",
            issue.get("labels").and_then(|v| v.as_str()),
        ),
    ];

    for (placeholder, value) in &replacements {
        if let Some(val) = value {
            result = result.replace(placeholder, val);
        }
    }

    result
}
