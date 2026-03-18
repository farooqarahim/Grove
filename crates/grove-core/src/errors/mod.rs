use thiserror::Error;

pub type GroveResult<T> = Result<T, GroveError>;

#[derive(Debug, Error)]
pub enum GroveError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("yaml error: {0}")]
    SerdeYaml(#[from] serde_yaml::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid state transition: {0}")]
    InvalidTransition(String),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("budget exceeded: used ${used_usd:.4} of ${limit_usd:.4}")]
    BudgetExceeded { used_usd: f64, limit_usd: f64 },

    #[error("merge conflict on {file_count} file(s): {files}")]
    MergeConflict { files: String, file_count: usize },

    /// LLM provider is not authenticated — API key missing.
    #[error("LLM auth error ({provider}): {message}")]
    LlmAuth { provider: String, message: String },

    /// HTTP request to an LLM provider failed (network / TLS).
    #[error("LLM request error ({provider}): {message}")]
    LlmRequest { provider: String, message: String },

    /// LLM provider returned a non-2xx HTTP status.
    #[error("LLM API error ({provider}) HTTP {status}: {message}")]
    LlmApi {
        provider: String,
        status: u16,
        message: String,
    },

    /// Workspace credit balance is too low to cover the requested LLM call.
    #[error("insufficient workspace credits: have ${available_usd:.4}, need ${required_usd:.4}")]
    InsufficientCredits {
        available_usd: f64,
        required_usd: f64,
    },

    /// Run was aborted by the user (GUI or CLI).
    #[error("run aborted by user")]
    Aborted,

    /// Concurrent conversation limit reached — run should be queued.
    #[error(
        "concurrent conversation limit reached for project {project_id} ({active}/{max} active)"
    )]
    PoolFull {
        project_id: String,
        active: usize,
        max: usize,
    },

    /// A git or filesystem worktree operation failed.
    #[error("worktree error ({operation}): {message}")]
    WorktreeError { operation: String, message: String },

    /// A file ownership lock is held by another session.
    #[error("ownership conflict on '{path}': held by session {holder}")]
    OwnershipConflict { path: String, holder: String },

    /// A provider subprocess or API call failed for a non-HTTP reason.
    #[error("provider error ({provider}): {message}")]
    ProviderError { provider: String, message: String },

    /// A hook script exited with a non-zero code or could not be launched.
    #[error("hook error ({hook}): {message}")]
    HookError { hook: String, message: String },

    /// A configuration or input validation check failed.
    #[error("validation error ({field}): {message}")]
    ValidationError { field: String, message: String },
}
