/// Direct HTTP client for the OpenAI Chat Completions API.
///
/// Also used for **DeepSeek**, which exposes an OpenAI-compatible API at a
/// different base URL.  Pass a `ProviderDef` at construction time to select
/// the target provider.
///
/// API reference: <https://platform.openai.com/docs/api-reference/chat>
/// DeepSeek ref:  <https://api-docs.deepseek.com>
use std::time::Duration;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::errors::{GroveError, GroveResult};
use crate::providers::{Provider, ProviderRequest, ProviderResponse};

use super::auth::{AuthInfo, AuthStore};
use super::models::{DEEPSEEK, OPENAI, ProviderDef};

// ── Default models ─────────────────────────────────────────────────────────────

const OPENAI_DEFAULT_MODEL: &str = "gpt-4o";
/// Maximum number of retry attempts on HTTP 429 responses.
const MAX_RETRIES: u32 = 3;
/// Maximum backoff delay in seconds between retries.
const MAX_RETRY_DELAY_SECS: u64 = 60;
const DEEPSEEK_DEFAULT_MODEL: &str = "deepseek-chat";
const INCEPTION_DEFAULT_MODEL: &str = "mercury-2";

// ── Wire types ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    #[serde(default)]
    usage: ChatUsage,
}

#[derive(Deserialize)]
struct Choice {
    message: AssistantMessage,
}

#[derive(Deserialize)]
struct AssistantMessage {
    content: Option<String>,
}

#[derive(Deserialize, Default)]
struct ChatUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
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

/// OpenAI-compatible provider.
///
/// Supports OpenAI (`openai`) and DeepSeek (`deepseek`).
/// The API key is resolved at call time from (in priority order):
/// 1. `<PROVIDER>_API_KEY` environment variable
/// 2. `~/.local/share/grove/auth.json` (or platform equivalent)
pub struct OpenAiCompatibleProvider {
    def: &'static ProviderDef,
    client: Client,
    api_key_override: Option<String>,
}

impl OpenAiCompatibleProvider {
    fn new_for(def: &'static ProviderDef) -> Self {
        Self {
            def,
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            api_key_override: None,
        }
    }

    /// Construct a provider for OpenAI.
    pub fn openai() -> Self {
        Self::new_for(&OPENAI)
    }

    /// Construct a provider for DeepSeek.
    pub fn deepseek() -> Self {
        Self::new_for(&DEEPSEEK)
    }

    /// Construct a provider for Inception Labs (Mercury).
    pub fn inception() -> Self {
        Self::new_for(&super::models::INCEPTION)
    }

    /// Override the API key (useful for tests).
    pub fn with_key(mut self, key: impl Into<String>) -> Self {
        self.api_key_override = Some(key.into());
        self
    }

    fn resolve_api_key(&self) -> GroveResult<String> {
        if let Some(ref k) = self.api_key_override {
            return Ok(k.clone());
        }
        match AuthStore::get(self.def.id) {
            Some(AuthInfo::Api { key }) => Ok(key),
            Some(_) => Err(GroveError::LlmAuth {
                provider: self.def.id.into(),
                message: "unexpected auth type for OpenAI-compatible provider".into(),
            }),
            None => Err(GroveError::LlmAuth {
                provider: self.def.id.into(),
                message: format!(
                    "No API key found. Set {} or run: grove auth set {} <key>",
                    self.def.env_key, self.def.id,
                ),
            }),
        }
    }

    fn default_model(&self) -> &'static str {
        match self.def.id {
            "deepseek" => DEEPSEEK_DEFAULT_MODEL,
            "inception" => INCEPTION_DEFAULT_MODEL,
            _ => OPENAI_DEFAULT_MODEL,
        }
    }
}

impl Provider for OpenAiCompatibleProvider {
    fn name(&self) -> &'static str {
        self.def.name
    }

    #[instrument(skip(self, request), fields(provider = self.def.id, model = ?request.model, role = %request.role))]
    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        let api_key = self.resolve_api_key()?;

        let model_id = request
            .model
            .as_deref()
            .unwrap_or_else(|| self.default_model());

        // Look up max_output_tokens for the selected model.
        let max_tokens = self
            .def
            .models
            .iter()
            .find(|m| m.id == model_id)
            .map(|m| m.max_output_tokens);

        let timeout = Duration::from_secs(request.timeout_override.unwrap_or(300));

        let payload = ChatRequest {
            model: model_id,
            max_tokens,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: "You are a skilled software engineer working inside the Grove multi-agent system.",
                },
                ChatMessage {
                    role: "user",
                    content: &request.instructions,
                },
            ],
        };

        debug!(
            provider = self.def.id,
            model = model_id,
            "calling Chat Completions API"
        );

        // 6.5: Retry loop — up to MAX_RETRIES attempts on HTTP 429.
        // Respects the `Retry-After` response header when present.
        let (status, body) = 'retry: {
            let mut last_err: Option<GroveError> = None;
            for attempt in 0..=MAX_RETRIES {
                let resp = match self
                    .client
                    .post(format!("{}/v1/chat/completions", self.def.base_url))
                    .timeout(timeout)
                    .bearer_auth(&api_key)
                    .header("content-type", "application/json")
                    .json(&payload)
                    .send()
                {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(GroveError::LlmRequest {
                            provider: self.def.id.into(),
                            message: e.to_string(),
                        });
                    }
                };

                let s = resp.status();
                let retry_after: Option<u64> = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok());
                let b = resp.text().map_err(|e| GroveError::LlmRequest {
                    provider: self.def.id.into(),
                    message: e.to_string(),
                })?;

                if s.as_u16() == 429 && attempt < MAX_RETRIES {
                    let delay = retry_after
                        .unwrap_or(2u64.pow(attempt + 1))
                        .min(MAX_RETRY_DELAY_SECS);
                    tracing::warn!(
                        provider = self.def.id,
                        attempt = attempt + 1,
                        max_retries = MAX_RETRIES,
                        delay_secs = delay,
                        "rate-limited (429) — retrying"
                    );
                    std::thread::sleep(Duration::from_secs(delay));
                    last_err = Some(GroveError::LlmApi {
                        provider: self.def.id.into(),
                        status: 429,
                        message: "rate limited".into(),
                    });
                    continue;
                }

                break 'retry (s, b);
            }
            return Err(last_err.unwrap_or(GroveError::LlmApi {
                provider: self.def.id.into(),
                status: 429,
                message: "rate limited after all retries".into(),
            }));
        };

        if !status.is_success() {
            let message = serde_json::from_str::<ApiError>(&body)
                .map(|e| e.error.message)
                .unwrap_or_else(|_| body.clone());
            return Err(GroveError::LlmApi {
                provider: self.def.id.into(),
                status: status.as_u16(),
                message,
            });
        }

        let parsed: ChatResponse =
            serde_json::from_str(&body).map_err(|e| GroveError::LlmRequest {
                provider: self.def.id.into(),
                message: format!("failed to parse response: {e}"),
            })?;

        let text = parsed
            .choices
            .into_iter()
            .filter_map(|c| c.message.content)
            .collect::<Vec<_>>()
            .join("\n");

        // Calculate cost in USD.
        let cost_usd = self.def.models.iter().find(|m| m.id == model_id).map(|m| {
            let input_cost = (parsed.usage.prompt_tokens as f64 / 1_000_000.0) * m.cost_input_per_m;
            let output_cost =
                (parsed.usage.completion_tokens as f64 / 1_000_000.0) * m.cost_output_per_m;
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
    fn openai_provider_name() {
        assert_eq!(OpenAiCompatibleProvider::openai().name(), "OpenAI");
    }

    #[test]
    fn deepseek_provider_name() {
        assert_eq!(OpenAiCompatibleProvider::deepseek().name(), "DeepSeek");
    }

    #[test]
    fn inception_provider_name() {
        assert_eq!(
            OpenAiCompatibleProvider::inception().name(),
            "Inception Labs"
        );
    }

    #[test]
    fn default_models() {
        let op = OpenAiCompatibleProvider::openai();
        assert_eq!(op.default_model(), "gpt-4o");

        let dp = OpenAiCompatibleProvider::deepseek();
        assert_eq!(dp.default_model(), "deepseek-chat");

        let ip = OpenAiCompatibleProvider::inception();
        assert_eq!(ip.default_model(), "mercury-2");
    }

    #[test]
    fn resolves_key_from_override() {
        let p = OpenAiCompatibleProvider::openai().with_key("sk-test");
        assert_eq!(p.resolve_api_key().unwrap(), "sk-test");
    }

    #[test]
    fn missing_key_returns_llm_auth_error() {
        // SAFETY: single-threaded test binary.
        unsafe { std::env::remove_var("OPENAI_API_KEY") };
        let p = OpenAiCompatibleProvider::openai();
        match p.resolve_api_key() {
            Ok(_) => {}
            Err(GroveError::LlmAuth { provider, .. }) => assert_eq!(provider, "openai"),
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
