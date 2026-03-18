use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::automation::TriggerConfig;
use crate::automation::engine::WorkflowEngine;
use crate::automation::event_bus::{AutomationEvent, EventBus};
use crate::db::DbHandle;
use crate::db::repositories::automations_repo;

type HmacSha256 = Hmac<Sha256>;

struct AppState {
    engine: Arc<WorkflowEngine>,
    event_bus: Arc<EventBus>,
    db_handle: DbHandle,
    secret: String,
}

/// Lightweight axum HTTP server that receives incoming webhook requests and
/// triggers automation runs.
///
/// Each webhook URL is scoped to a specific automation by its id:
///   `POST /webhook/{automation_id}`
///
/// An optional HMAC-SHA256 signature (`x-grove-signature: sha256=<hex>`) is
/// verified when a non-empty `secret` is configured.
pub struct WebhookServer {
    engine: Arc<WorkflowEngine>,
    event_bus: Arc<EventBus>,
    db_handle: DbHandle,
    secret: String,
}

impl WebhookServer {
    pub fn new(
        engine: Arc<WorkflowEngine>,
        event_bus: Arc<EventBus>,
        db_handle: DbHandle,
        secret: String,
    ) -> Self {
        Self {
            engine,
            event_bus,
            db_handle,
            secret,
        }
    }

    /// Start listening for incoming webhooks on `0.0.0.0:{port}`.
    ///
    /// This method runs until the server encounters a fatal error or the
    /// process shuts down. It should be spawned as a background tokio task.
    pub async fn run(self, port: u16) {
        let state = Arc::new(AppState {
            engine: self.engine,
            event_bus: self.event_bus,
            db_handle: self.db_handle,
            secret: self.secret,
        });

        let app = Router::new()
            .route("/webhook/{automation_id}", post(handle_webhook))
            .with_state(state);

        let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(port, error = %e, "failed to bind webhook server");
                return;
            }
        };

        tracing::info!(port, "webhook server started");
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!(error = %e, "webhook server error");
        }
    }
}

async fn handle_webhook(
    State(state): State<Arc<AppState>>,
    Path(automation_id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    // 1. Verify HMAC signature if a secret is configured.
    if !state.secret.is_empty() {
        let signature = headers
            .get("x-grove-signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !verify_signature(&state.secret, &body, signature) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "invalid signature"})),
            );
        }
    }

    // 2. Load automation from DB.
    let conn = match state.db_handle.connect() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "webhook: failed to connect to DB");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "internal error"})),
            );
        }
    };

    let automation = match automations_repo::get_automation(&conn, &automation_id) {
        Ok(a) => a,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "automation not found"})),
            );
        }
    };

    // 3. Verify automation is enabled and has a webhook trigger type.
    if !automation.enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "automation is disabled"})),
        );
    }

    if !matches!(automation.trigger, TriggerConfig::Webhook { .. }) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "automation is not a webhook trigger"})),
        );
    }

    // 4. Parse body as JSON; fall back to wrapping the raw body as a JSON string.
    let payload: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(_) => Value::String(body),
    };

    // 5. Publish an event so other subscribers (notifier, UI) can react.
    state.event_bus.publish(AutomationEvent::WebhookReceived {
        automation_id: automation_id.clone(),
        payload: payload.clone(),
    });

    // 6. Start the automation run.
    match state.engine.start_run(
        &automation_id,
        json!({
            "trigger": "webhook",
            "payload": payload,
        }),
    ) {
        Ok(run_id) => (
            StatusCode::ACCEPTED,
            Json(json!({"automation_run_id": run_id})),
        ),
        Err(e) => {
            tracing::error!(error = %e, "webhook: failed to start automation run");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to start run"})),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// HMAC-SHA256 signature verification
// ---------------------------------------------------------------------------

/// Verify that `signature` (expected format: `sha256=<hex>`) matches the
/// HMAC-SHA256 of `body` using `secret`.
fn verify_signature(secret: &str, body: &str, signature: &str) -> bool {
    let hex_str = match signature.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };

    let expected = match decode_hex(hex_str) {
        Some(b) => b,
        None => return false,
    };

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body.as_bytes());
    mac.verify_slice(&expected).is_ok()
}

/// Decode a hex-encoded string into raw bytes.
fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}
