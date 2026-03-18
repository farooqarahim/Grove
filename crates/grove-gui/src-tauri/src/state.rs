use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use std::time::Instant;

use grove_core::app::GroveApp;
use grove_core::automation::engine::WorkflowEngine;
use grove_core::automation::event_bus::EventBus;
use grove_core::automation::notifier::Notifier;
use grove_core::automation::scheduler::CronScheduler;
use grove_core::config::loader::load_or_create;
use grove_core::db::{DbHandle, DbPool};
use grove_core::orchestrator::abort_handle::AbortHandle;

/// Shared application state managed by Tauri.
pub struct AppState {
    /// Connection pool for the workspace database.
    pool: DbPool,
    /// Tauri app handle — used to emit events to the frontend.
    pub app_handle: tauri::AppHandle,
    app: GroveApp,
    /// Abort handles keyed by conversation_id.
    active_aborts: Mutex<HashMap<String, AbortHandle>>,
    /// Cache: run_id → (project_root, fetched_at). TTL = 30 s.
    pub project_root_cache: Mutex<HashMap<String, (std::path::PathBuf, Instant)>>,
    /// Cache: run_id → (run_cwd, fetched_at). TTL = 10 s.
    pub run_cwd_cache: Mutex<HashMap<String, (std::path::PathBuf, Instant)>>,
    /// Live stdin handles for running agents, keyed by run_id.
    pub agent_inputs:
        Arc<Mutex<HashMap<String, grove_core::providers::agent_input::AgentInputHandle>>>,
    /// Persistent run-control senders keyed by run_id.
    pub run_controls: Arc<
        Mutex<
            HashMap<
                String,
                grove_core::providers::claude_code_persistent::PersistentRunControlHandle,
            >,
        >,
    >,
    /// Automation event bus — shared by the workflow engine, scheduler, notifier, and UI.
    pub event_bus: Arc<EventBus>,
    /// Automation workflow engine — drives DAG execution for automation runs.
    pub workflow_engine: Arc<WorkflowEngine>,
    /// DB handle for automation subsystem (used to spawn background services).
    automation_db_handle: DbHandle,
}

impl AppState {
    pub fn new(app: GroveApp, app_handle: tauri::AppHandle) -> Self {
        let pool = app.pool().clone();
        let event_bus = Arc::new(EventBus::with_default_capacity());
        let db_handle = DbHandle::new(app.data_root.as_path());
        let workflow_engine = Arc::new(WorkflowEngine::new(
            Arc::clone(&event_bus),
            db_handle.clone(),
        ));
        Self {
            pool,
            app_handle,
            app,
            active_aborts: Mutex::new(HashMap::new()),
            project_root_cache: Mutex::new(HashMap::new()),
            run_cwd_cache: Mutex::new(HashMap::new()),
            agent_inputs: Arc::new(Mutex::new(HashMap::new())),
            run_controls: Arc::new(Mutex::new(HashMap::new())),
            event_bus,
            workflow_engine,
            automation_db_handle: db_handle,
        }
    }

    /// Spawn background automation services on the tokio runtime.
    ///
    /// Must be called after the AppState is fully constructed and managed by
    /// Tauri, from within the Tauri `setup()` callback where a tokio runtime
    /// is available.
    pub fn spawn_automation_services(&self) {
        let db_handle = self.automation_db_handle.clone();

        // 1. Engine event loop — listens for TaskFinished events and advances DAGs.
        let engine = Arc::clone(&self.workflow_engine);
        tauri::async_runtime::spawn(async move {
            engine.run_event_loop().await;
        });

        // 2. Cron scheduler — polls every 60s for due cron automations.
        let scheduler = Arc::new(CronScheduler::new(
            Arc::clone(&self.workflow_engine),
            Arc::clone(&self.event_bus),
            db_handle.clone(),
        ));
        tauri::async_runtime::spawn(async move {
            scheduler.run().await;
        });

        // 3. Notifier — dispatches notifications on run completion/failure.
        let notifier = Arc::new(Notifier::new(
            Arc::clone(&self.event_bus),
            db_handle.clone(),
        ));
        tauri::async_runtime::spawn(async move {
            notifier.run().await;
        });

        // 4. Webhook server — only if webhook.enabled in grove.yaml.
        let data_root = self.app.data_root.clone();
        let webhook_cfg = load_or_create(&data_root)
            .map(|c| c.webhook)
            .unwrap_or_default();
        if webhook_cfg.enabled {
            let server = grove_core::automation::webhook::WebhookServer::new(
                Arc::clone(&self.workflow_engine),
                Arc::clone(&self.event_bus),
                db_handle,
                webhook_cfg.secret,
            );
            let port = webhook_cfg.port;
            tauri::async_runtime::spawn(async move {
                server.run(port).await;
            });
        }

        tracing::info!("automation background services spawned");
    }

    pub fn workspace_root(&self) -> &Path {
        &self.app.data_root
    }

    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    #[allow(dead_code)]
    pub fn app(&self) -> &GroveApp {
        &self.app
    }

    pub fn set_abort(&self, conversation_id: String, handle: AbortHandle) {
        self.active_aborts.lock().insert(conversation_id, handle);
    }

    pub fn take_abort(&self, key: &str) -> Option<AbortHandle> {
        self.active_aborts.lock().remove(key)
    }

    #[allow(dead_code)]
    pub fn take_active_abort(&self) -> Option<AbortHandle> {
        let mut map = self.active_aborts.lock();
        let key = map.keys().next()?.to_string();
        map.remove(&key)
    }
}

impl Drop for AppState {
    fn drop(&mut self) {
        for handle in self.active_aborts.lock().values() {
            handle.abort();
        }
        for control in self.run_controls.lock().values() {
            let _ = control
                .tx
                .send(grove_core::providers::claude_code_persistent::RunControlMessage::Abort);
        }
    }
}
