pub mod anthropic;
/// LLM Router — multi-provider LLM integration for Grove.
///
/// Supports direct API access to Anthropic (Claude), OpenAI (GPT/o-series),
/// DeepSeek (OpenAI-compatible), and Inception Labs / Mercury (OpenAI-compatible)
/// without requiring any provider CLI to be installed.
///
/// # Credential storage
///
/// API keys are stored in `<XDG_DATA_HOME>/grove/auth.json` (Linux) or
/// `~/Library/Application Support/grove/auth.json` (macOS) with `0o600`
/// permissions.  Environment variables take precedence over stored keys:
///
/// | Provider   | Env var              |
/// |------------|----------------------|
/// | anthropic  | `ANTHROPIC_API_KEY`  |
/// | openai     | `OPENAI_API_KEY`     |
/// | deepseek   | `DEEPSEEK_API_KEY`   |
/// | inception  | `INCEPTION_API_KEY`  |
///
/// # Usage
///
/// ```rust,ignore
/// use grove_core::llm::{LlmRouter, LlmProviderKind};
///
/// // Store a key.
/// LlmRouter::set_api_key(LlmProviderKind::Anthropic, "sk-ant-...").unwrap();
///
/// // Build a Provider and use it.
/// let provider = LlmRouter::build_provider(LlmProviderKind::OpenAi, None);
/// ```
pub mod auth;
pub mod models;
pub mod openai;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;

pub use auth::{AuthInfo, AuthStore, get_grove_key};
pub use models::{ALL_PROVIDERS, ModelDef, ProviderDef};

use crate::errors::{GroveError, GroveResult};
use crate::providers::{Provider, ProviderRequest, ProviderResponse};

use self::{
    anthropic::AnthropicProvider,
    models::{ANTHROPIC, DEEPSEEK, INCEPTION, OPENAI, find_provider, provider_for_model},
    openai::OpenAiCompatibleProvider,
};

/// Stable identifier for a supported LLM provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmProviderKind {
    Anthropic,
    OpenAi,
    DeepSeek,
    Inception,
}

impl LlmProviderKind {
    /// Parse from a string id (case-insensitive).
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "anthropic" | "claude" => Some(Self::Anthropic),
            "openai" | "open-ai" | "open_ai" => Some(Self::OpenAi),
            "deepseek" | "deep-seek" | "deep_seek" => Some(Self::DeepSeek),
            "inception" | "inception-labs" | "mercury" => Some(Self::Inception),
            _ => None,
        }
    }

    /// Canonical provider id (matches `ProviderDef.id`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Anthropic => ANTHROPIC.id,
            Self::OpenAi => OPENAI.id,
            Self::DeepSeek => DEEPSEEK.id,
            Self::Inception => INCEPTION.id,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Anthropic => ANTHROPIC.name,
            Self::OpenAi => OPENAI.name,
            Self::DeepSeek => DEEPSEEK.name,
            Self::Inception => INCEPTION.name,
        }
    }
}

impl std::fmt::Display for LlmProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

/// How API costs for LLM calls are paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmAuthMode {
    /// Use the user's own API key (from auth.json or env var).
    UserKey,
    /// Use Grove's pooled API key; debit workspace credit balance before each call.
    WorkspaceCredits,
}

impl LlmAuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserKey => "user_key",
            Self::WorkspaceCredits => "workspace_credits",
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "user_key" => Some(Self::UserKey),
            "workspace_credits" => Some(Self::WorkspaceCredits),
            _ => None,
        }
    }
}

/// A workspace-level LLM selection — the user's chosen provider + model + auth mode.
#[derive(Debug, Clone)]
pub struct LlmSelection {
    pub kind: LlmProviderKind,
    pub model: Option<String>,
    pub auth_mode: LlmAuthMode,
}

/// Summary of a configured provider shown in `grove llm list`.
#[derive(Debug, Clone)]
pub struct ProviderStatus {
    pub kind: LlmProviderKind,
    pub name: &'static str,
    pub authenticated: bool,
    pub model_count: usize,
    pub default_model: &'static str,
}

/// Top-level LLM router.
///
/// Routes a workspace selection to the correct `Provider` implementation.
/// All methods are associated functions — no instance is needed.
pub struct LlmRouter;

impl LlmRouter {
    // ── Credential management ─────────────────────────────────────────────────

    /// Store an API key for `provider`.
    pub fn set_api_key(provider: LlmProviderKind, key: impl Into<String>) -> std::io::Result<()> {
        AuthStore::set(provider.id(), AuthInfo::Api { key: key.into() })
    }

    /// Remove the stored API key for `provider`.
    pub fn remove_api_key(provider: LlmProviderKind) -> std::io::Result<()> {
        AuthStore::remove(provider.id())
    }

    /// Check whether a usable key exists for `provider` (env var or stored).
    pub fn is_authenticated(provider: LlmProviderKind) -> bool {
        AuthStore::get(provider.id()).is_some()
    }

    // ── Model registry ────────────────────────────────────────────────────────

    /// Return status for all providers, derived from the static `ALL_PROVIDERS` registry.
    pub fn providers() -> Vec<ProviderStatus> {
        ALL_PROVIDERS
            .iter()
            .filter_map(|def| {
                let kind = LlmProviderKind::from_str(def.id)?;
                Some(ProviderStatus {
                    kind,
                    name: def.name,
                    authenticated: Self::is_authenticated(kind),
                    model_count: def.models.len(),
                    default_model: def.models.first().map(|m| m.id).unwrap_or(""),
                })
            })
            .collect()
    }

    /// Return all models for `provider`.
    pub fn models(provider: LlmProviderKind) -> &'static [ModelDef] {
        find_provider(provider.id())
            .map(|p| p.models)
            .unwrap_or(&[])
    }

    // ── Provider factory ──────────────────────────────────────────────────────

    /// Build a `Provider` implementation for `kind`.
    ///
    /// The provider reads its API key lazily at call time, so this function
    /// never fails even if no key is configured yet (the first `execute` call
    /// will return a `GroveError::LlmAuth` if no key is found).
    ///
    /// `api_key_override` pins a specific key, bypassing the auth store and
    /// environment variable lookup — useful in tests.
    pub fn build_provider(
        kind: LlmProviderKind,
        api_key_override: Option<&str>,
    ) -> Arc<dyn Provider> {
        match kind {
            LlmProviderKind::Anthropic => {
                let p = if let Some(k) = api_key_override {
                    AnthropicProvider::with_key(k)
                } else {
                    AnthropicProvider::new()
                };
                Arc::new(p)
            }
            LlmProviderKind::OpenAi => {
                let p = if let Some(k) = api_key_override {
                    OpenAiCompatibleProvider::openai().with_key(k)
                } else {
                    OpenAiCompatibleProvider::openai()
                };
                Arc::new(p)
            }
            LlmProviderKind::DeepSeek => {
                let p = if let Some(k) = api_key_override {
                    OpenAiCompatibleProvider::deepseek().with_key(k)
                } else {
                    OpenAiCompatibleProvider::deepseek()
                };
                Arc::new(p)
            }
            LlmProviderKind::Inception => {
                let p = if let Some(k) = api_key_override {
                    OpenAiCompatibleProvider::inception().with_key(k)
                } else {
                    OpenAiCompatibleProvider::inception()
                };
                Arc::new(p)
            }
        }
    }

    /// Infer the provider from a model ID and build the matching provider.
    ///
    /// Returns `None` if the model ID is not recognised.
    pub fn build_provider_for_model(model_id: &str) -> Option<Arc<dyn Provider>> {
        let def = provider_for_model(model_id)?;
        let kind = LlmProviderKind::from_str(def.id)?;
        Some(Self::build_provider(kind, None))
    }

    // ── Workspace LLM selection ───────────────────────────────────────────────

    /// Persist the LLM selection for `workspace_id`.
    pub fn set_workspace_selection(
        conn: &Connection,
        workspace_id: &str,
        selection: &LlmSelection,
    ) -> GroveResult<()> {
        crate::db::repositories::workspaces_repo::update_llm_selection(
            conn,
            workspace_id,
            selection.kind.id(),
            selection.model.as_deref(),
            selection.auth_mode.as_str(),
        )
    }

    /// Read the current LLM selection for `workspace_id`.
    ///
    /// Returns `None` when no selection has been made yet.
    pub fn get_workspace_selection(
        conn: &Connection,
        workspace_id: &str,
    ) -> GroveResult<Option<LlmSelection>> {
        let row = crate::db::repositories::workspaces_repo::get(conn, workspace_id)?;
        let provider_id = match &row.llm_provider {
            Some(p) => p.clone(),
            None => return Ok(None),
        };
        let kind = LlmProviderKind::from_str(&provider_id).ok_or_else(|| {
            GroveError::Config(format!(
                "unrecognised llm_provider in workspace: {provider_id}"
            ))
        })?;
        let auth_mode = LlmAuthMode::from_str(&row.llm_auth_mode).unwrap_or(LlmAuthMode::UserKey);
        Ok(Some(LlmSelection {
            kind,
            model: row.llm_model,
            auth_mode,
        }))
    }

    // ── Credit-gated provider factory ─────────────────────────────────────────

    /// Build a provider that checks and deducts workspace credits before each call.
    ///
    /// Wraps the resolved provider in a `CreditGatedProvider` that:
    /// 1. Before `execute`: reserves $0.001 from the workspace balance (atomic guard).
    /// 2. After `execute`: deducts the remaining actual cost.
    ///
    /// The Grove pooled API key is read from `GROVE_<PROVIDER>_API_KEY`.
    /// Returns `Err(GroveError::LlmAuth)` when the pooled key is not configured.
    pub fn build_credit_gated_provider(
        kind: LlmProviderKind,
        workspace_id: &str,
        db_path: &Path,
    ) -> GroveResult<Arc<dyn Provider>> {
        let pooled_key = auth::get_grove_key(kind.id()).ok_or_else(|| GroveError::LlmAuth {
            provider: kind.id().to_string(),
            message: format!(
                "Workspace credits mode requires {} to be set in Grove's environment",
                auth::grove_key_for(kind.id())
            ),
        })?;

        let inner = Self::build_provider(kind, Some(&pooled_key));

        Ok(Arc::new(CreditGatedProvider {
            inner,
            workspace_id: workspace_id.to_string(),
            db_path: db_path.to_path_buf(),
        }))
    }
}

// ── CreditGatedProvider ───────────────────────────────────────────────────────

/// A `Provider` wrapper that checks and deducts workspace credits on every call.
///
/// Before `execute`:  reserves $0.001 from the balance (atomic check-and-deduct).
/// After `execute`:   deducts the remaining actual cost (actual − 0.001).
///
/// If the post-call deduction fails (e.g. a concurrent call already depleted the
/// balance), a warning is logged — the call is NOT reversed since tokens have
/// already been consumed.
struct CreditGatedProvider {
    inner: Arc<dyn Provider>,
    workspace_id: String,
    db_path: PathBuf,
}

// Send + Sync auto-derived: all fields (Arc<dyn Provider>, String, PathBuf) are Send + Sync,
// and Provider: Send + Sync means Arc<dyn Provider> is Send + Sync.

impl Provider for CreditGatedProvider {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn execute(&self, request: &ProviderRequest) -> GroveResult<ProviderResponse> {
        use crate::db::connection;
        use crate::db::repositories::workspaces_repo;

        // Pre-call: atomically reserve $0.001 — fails fast if balance is zero.
        let conn = connection::open(&self.db_path)?;
        workspaces_repo::check_and_deduct_credits(&conn, &self.workspace_id, 0.001)?;
        drop(conn); // release DB lock before the (potentially long) LLM call

        let response = self.inner.execute(request)?;

        // Post-call: deduct the remaining actual cost (actual_cost − $0.001 already taken).
        let conn = connection::open(&self.db_path)?;
        let actual_cost = response.cost_usd.unwrap_or(0.0);
        let remaining = (actual_cost - 0.001).max(0.0);
        if remaining > 0.0 {
            if let Err(e) =
                workspaces_repo::check_and_deduct_credits(&conn, &self.workspace_id, remaining)
            {
                tracing::warn!(
                    workspace_id = %self.workspace_id,
                    actual_cost,
                    error = %e,
                    "post-call credit deduction failed; balance may be negative"
                );
            }
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_str_variants() {
        assert_eq!(
            LlmProviderKind::from_str("anthropic"),
            Some(LlmProviderKind::Anthropic)
        );
        assert_eq!(
            LlmProviderKind::from_str("claude"),
            Some(LlmProviderKind::Anthropic)
        );
        assert_eq!(
            LlmProviderKind::from_str("openai"),
            Some(LlmProviderKind::OpenAi)
        );
        assert_eq!(
            LlmProviderKind::from_str("open-ai"),
            Some(LlmProviderKind::OpenAi)
        );
        assert_eq!(
            LlmProviderKind::from_str("deepseek"),
            Some(LlmProviderKind::DeepSeek)
        );
        assert_eq!(
            LlmProviderKind::from_str("deep-seek"),
            Some(LlmProviderKind::DeepSeek)
        );
        assert_eq!(
            LlmProviderKind::from_str("inception"),
            Some(LlmProviderKind::Inception)
        );
        assert_eq!(
            LlmProviderKind::from_str("inception-labs"),
            Some(LlmProviderKind::Inception)
        );
        assert_eq!(
            LlmProviderKind::from_str("mercury"),
            Some(LlmProviderKind::Inception)
        );
        assert_eq!(LlmProviderKind::from_str("unknown"), None);
    }

    #[test]
    fn providers_returns_four() {
        let list = LlmRouter::providers();
        assert_eq!(list.len(), 4);
        let ids: Vec<_> = list.iter().map(|p| p.kind.id()).collect();
        assert!(ids.contains(&"anthropic"));
        assert!(ids.contains(&"openai"));
        assert!(ids.contains(&"deepseek"));
        assert!(ids.contains(&"inception"));
    }

    #[test]
    fn models_non_empty() {
        assert!(!LlmRouter::models(LlmProviderKind::Anthropic).is_empty());
        assert!(!LlmRouter::models(LlmProviderKind::OpenAi).is_empty());
        assert!(!LlmRouter::models(LlmProviderKind::DeepSeek).is_empty());
        assert!(!LlmRouter::models(LlmProviderKind::Inception).is_empty());
    }

    #[test]
    fn build_provider_for_known_models() {
        assert!(LlmRouter::build_provider_for_model("claude-sonnet-4-6").is_some());
        assert!(LlmRouter::build_provider_for_model("gpt-4o").is_some());
        assert!(LlmRouter::build_provider_for_model("deepseek-chat").is_some());
        assert!(LlmRouter::build_provider_for_model("mercury-2").is_some());
        assert!(LlmRouter::build_provider_for_model("unknown-xyz").is_none());
    }

    #[test]
    fn build_provider_returns_correct_name() {
        let p = LlmRouter::build_provider(LlmProviderKind::Anthropic, None);
        assert_eq!(p.name(), "anthropic");

        let p = LlmRouter::build_provider(LlmProviderKind::OpenAi, None);
        assert_eq!(p.name(), "OpenAI");

        let p = LlmRouter::build_provider(LlmProviderKind::DeepSeek, None);
        assert_eq!(p.name(), "DeepSeek");

        let p = LlmRouter::build_provider(LlmProviderKind::Inception, None);
        assert_eq!(p.name(), "Inception Labs");
    }
}
