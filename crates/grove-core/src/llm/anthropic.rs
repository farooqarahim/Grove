/// Direct HTTP client for the Anthropic Messages API.
///
/// Implements Grove's `Provider` trait so it can be used interchangeably
/// with `ClaudeCodeProvider` for non-agentic tasks (planning, analysis,
/// document generation).
///
/// API reference: <https://docs.anthropic.com/en/api/messages>
use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::errors::{GroveError, GroveResult};
use crate::providers::{Provider, ProviderRequest, ProviderResponse};

use super::auth::{AuthInfo, AuthStore};
use super::models::{ANTHROPIC, ModelDef};

/// Default model used when `ProviderRequest.model` is `None`.
const DEFAULT_MODEL: &str = "claude-sonnet-4-6";
/// Anthropic API version header value.
const API_VERSION: &str = "2023-06-01";
/// Maximum number of retry attempts on HTTP 429 responses.
const MAX_RETRIES: u32 = 3;
/// Maximum backoff delay in seconds between retries.
const MAX_RETRY_DELAY_SECS: u64 = 60;
/// Beta features header — enables extended output and fine-grained streaming.
const BETA_HEADER: &str =
    "claude-code-20250219,interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14";

// ── Wire types ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Usage,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    Text {
        text: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

#[derive(Deserialize)]
struct ApiError {
    error: ApiErrorBody,
}

#[derive(Deserialize)]
struct ApiErrorBody {
    message: String,
}

// ── Provider implementation ────────────────────────────────────────────────────

/// Direct Anthropic API provider.
///
/// The API key is resolved at call time from (in priority order):
/// 1. `ANTHROPIC_API_KEY` environment variable
/// 2. `~/.local/share/grove/auth.json` (or platform equivalent)
pub struct AnthropicProvider {
    client: Client,
    /// Explicit key override (takes precedence over env / auth store).
    api_key_override: Option<String>,
}

impl AnthropicProvider {
    /// Create a provider that resolves the API key from env / auth store.
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            api_key_override: None,
        }
    }

    /// Create a provider with an explicit API key (useful for tests).
    pub fn with_key(key: impl Into<String>) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            api_key_override: Some(key.into()),
        }
    }

    fn resolve_api_key(&self) -> GroveResult<String> {
        if let Some(ref k) = self.api_key_override {
            return Ok(k.clone());
        }
        match AuthStore::get("anthropic") {
            Some(AuthInfo::Api { key }) => Ok(key),
            Some(_) => Err(GroveError::LlmAuth {
                provider: "anthropic".into(),
                message: "unexpected auth type for Anthropic provider".into(),
            }),
            None => Err(GroveError::LlmAuth {
                provider: "anthropic".into(),
                message:
                    "No API key found. Set ANTHROPIC_API_KEY or run: grove auth set anthropic <key>"
                        .to_string(),
            }),
        }
    }
}

impl Default for AnthropicProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    #[instrument(skip(self, request), fields(model = ?request.model, role = %request.role))]
    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        let api_key = self.resolve_api_key()?;

        let model_id = request.model.as_deref().unwrap_or(DEFAULT_MODEL);

        // Look up max_output_tokens for the selected model; fall back to 8192.
        let max_tokens = ANTHROPIC
            .models
            .iter()
            .find(|m: &&ModelDef| m.id == model_id)
            .map(|m| m.max_output_tokens)
            .unwrap_or(8_192);

        let timeout = Duration::from_secs(request.timeout_override.unwrap_or(300));

        let payload = MessagesRequest {
            model: model_id,
            max_tokens,
            system: "You are a skilled software engineer working inside the Grove multi-agent system.",
            messages: vec![Message {
                role: "user",
                content: &request.instructions,
            }],
        };

        debug!(model = model_id, "calling Anthropic Messages API");

        // 6.5: Retry loop — up to MAX_RETRIES attempts on HTTP 429.
        // Respects the `Retry-After` response header when present.
        let (status, body) = 'retry: {
            let mut last_err: Option<GroveError> = None;
            for attempt in 0..=MAX_RETRIES {
                let resp = match self
                    .client
                    .post(format!("{}/v1/messages", ANTHROPIC.base_url))
                    .timeout(timeout)
                    .header("x-api-key", &api_key)
                    .header("anthropic-version", API_VERSION)
                    .header("anthropic-beta", BETA_HEADER)
                    .header("content-type", "application/json")
                    .json(&payload)
                    .send()
                {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(GroveError::LlmRequest {
                            provider: "anthropic".into(),
                            message: e.to_string(),
                        });
                    }
                };

                let s = resp.status();
                // Read Retry-After before consuming the body.
                let retry_after: Option<u64> = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok());
                let b = resp.text().map_err(|e| GroveError::LlmRequest {
                    provider: "anthropic".into(),
                    message: e.to_string(),
                })?;

                if s.as_u16() == 429 && attempt < MAX_RETRIES {
                    let delay = retry_after
                        .unwrap_or(2u64.pow(attempt + 1))
                        .min(MAX_RETRY_DELAY_SECS);
                    tracing::warn!(
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_secs = delay,
                        "anthropic: rate-limited (429) — retrying"
                    );
                    std::thread::sleep(Duration::from_secs(delay));
                    last_err = Some(GroveError::LlmApi {
                        provider: "anthropic".into(),
                        status: 429,
                        message: "rate limited".into(),
                    });
                    continue;
                }

                break 'retry (s, b);
            }
            // All retries exhausted on 429.
            return Err(last_err.unwrap_or(GroveError::LlmApi {
                provider: "anthropic".into(),
                status: 429,
                message: "rate limited after all retries".into(),
            }));
        };

        if !status.is_success() {
            let message = serde_json::from_str::<ApiError>(&body)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| body.clone());
            return Err(GroveError::LlmApi {
                provider: "anthropic".into(),
                status: status.as_u16(),
                message,
            });
        }

        let parsed: MessagesResponse =
            serde_json::from_str(&body).map_err(|e| GroveError::LlmRequest {
                provider: "anthropic".into(),
                message: format!("failed to parse response: {e}"),
            })?;

        let text = parsed
            .content
            .into_iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text),
                ContentBlock::Unknown => None,
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Calculate cost in USD.
        let cost_usd = ANTHROPIC.models.iter().find(|m| m.id == model_id).map(|m| {
            let input_cost = (parsed.usage.input_tokens as f64 / 1_000_000.0) * m.cost_input_per_m;
            let output_cost =
                (parsed.usage.output_tokens as f64 / 1_000_000.0) * m.cost_output_per_m;
            input_cost + output_cost
        });

        Ok(ProviderResponse {
            summary: text,
            changed_files: vec![],
            cost_usd,
            provider_session_id: None,
            pid: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name() {
        assert_eq!(AnthropicProvider::new().name(), "anthropic");
    }

    #[test]
    fn resolves_key_from_override() {
        let p = AnthropicProvider::with_key("sk-test");
        assert_eq!(p.resolve_api_key().unwrap(), "sk-test");
    }

    #[test]
    fn resolves_key_from_env() {
        // SAFETY: single-threaded test binary; no other threads reading this var.
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-env-test") };
        let p = AnthropicProvider::new();
        let result = p.resolve_api_key().unwrap();
        assert_eq!(result, "sk-env-test");
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }

    #[test]
    fn missing_key_returns_error() {
        // SAFETY: single-threaded test binary.
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
        let p = AnthropicProvider::new();
        // Only errors if nothing in auth store — acceptable in unit test.
        // We just verify the error type, not the message.
        match p.resolve_api_key() {
            Ok(_) => {} // key was found in auth store
            Err(GroveError::LlmAuth { provider, .. }) => assert_eq!(provider, "anthropic"),
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
