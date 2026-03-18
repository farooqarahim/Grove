/// Credential storage for LLM provider API keys.
///
/// Keys are stored in `<XDG_DATA_HOME>/grove/auth.json` (Linux) or
/// `~/Library/Application Support/grove/auth.json` (macOS) with file
/// permissions `0o600` (owner read/write only).
///
/// The file format mirrors opencode's auth.json:
/// ```json
/// {
///   "anthropic": { "type": "api", "key": "sk-ant-..." },
///   "openai":    { "type": "api", "key": "sk-..." },
///   "deepseek":  { "type": "api", "key": "sk-..." }
/// }
/// ```
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Supported credential types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthInfo {
    /// User's own API key stored in auth.json.
    Api { key: String },
    /// Use Grove's pooled API key; costs are debited from the workspace credit balance.
    WorkspaceCredits,
}

/// Persistent credential store backed by `auth.json`.
pub struct AuthStore;

impl AuthStore {
    /// Absolute path to the auth.json file.
    pub fn path() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("grove")
            .join("auth.json")
    }

    /// Read the credential for `provider_id`.
    ///
    /// Checks the environment variable for the provider first (e.g.
    /// `ANTHROPIC_API_KEY`), then falls back to the stored credential.
    /// Returns `None` if no credential is configured.
    pub fn get(provider_id: &str) -> Option<AuthInfo> {
        // Env var takes precedence over stored credential.
        let env_key = env_var_for(provider_id);
        if let Ok(val) = std::env::var(&env_key) {
            if !val.is_empty() {
                return Some(AuthInfo::Api { key: val });
            }
        }
        Self::all().remove(provider_id)
    }

    /// Read all stored credentials.
    pub fn all() -> HashMap<String, AuthInfo> {
        let path = Self::path();
        let content = fs::read_to_string(&path).unwrap_or_default();
        if content.is_empty() {
            return HashMap::new();
        }
        serde_json::from_str::<HashMap<String, AuthInfo>>(&content).unwrap_or_default()
    }

    /// Persist `info` for `provider_id`, creating the file if needed.
    ///
    /// Sets file permissions to `0o600` on Unix systems.
    pub fn set(provider_id: &str, info: AuthInfo) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut all = Self::all();
        all.insert(provider_id.to_string(), info);
        write_secure(&path, &all)
    }

    /// Remove the stored credential for `provider_id`.
    pub fn remove(provider_id: &str) -> std::io::Result<()> {
        let path = Self::path();
        let mut all = Self::all();
        all.remove(provider_id);
        write_secure(&path, &all)
    }
}

/// Write `data` to `path` as pretty JSON with `0o600` permissions.
fn write_secure(path: &PathBuf, data: &HashMap<String, AuthInfo>) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(data)
        .map_err(std::io::Error::other)?;
    fs::write(path, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// Return the conventional environment variable name for a provider's user key.
///
/// | provider_id   | env var              |
/// |---------------|----------------------|
/// | anthropic     | ANTHROPIC_API_KEY    |
/// | openai        | OPENAI_API_KEY       |
/// | deepseek      | DEEPSEEK_API_KEY     |
/// | inception     | INCEPTION_API_KEY    |
pub fn env_var_for(provider_id: &str) -> String {
    format!("{}_API_KEY", provider_id.to_uppercase().replace('-', "_"))
}

/// Return the Grove-owned pooled API key env var for a provider.
///
/// These are set in Grove's own deployment environment — not by the user.
///
/// | provider_id   | env var                    |
/// |---------------|----------------------------|
/// | anthropic     | GROVE_ANTHROPIC_API_KEY    |
/// | openai        | GROVE_OPENAI_API_KEY       |
/// | deepseek      | GROVE_DEEPSEEK_API_KEY     |
/// | inception     | GROVE_INCEPTION_API_KEY    |
pub fn grove_key_for(provider_id: &str) -> String {
    format!(
        "GROVE_{}_API_KEY",
        provider_id.to_uppercase().replace('-', "_")
    )
}

/// Resolve the Grove pooled API key for a provider from the environment.
///
/// Returns `None` when the pooled key is not configured (i.e., this is not a
/// Grove-hosted deployment or the provider is not available via workspace credits).
pub fn get_grove_key(provider_id: &str) -> Option<String> {
    std::env::var(grove_key_for(provider_id))
        .ok()
        .filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_var_names() {
        assert_eq!(env_var_for("anthropic"), "ANTHROPIC_API_KEY");
        assert_eq!(env_var_for("openai"), "OPENAI_API_KEY");
        assert_eq!(env_var_for("deepseek"), "DEEPSEEK_API_KEY");
    }

    #[test]
    fn auth_info_roundtrip() {
        let info = AuthInfo::Api {
            key: "sk-test-123".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: AuthInfo = serde_json::from_str(&json).unwrap();
        match back {
            AuthInfo::Api { key } => assert_eq!(key, "sk-test-123"),
            AuthInfo::WorkspaceCredits => panic!("unexpected variant"),
        }
    }

    #[test]
    fn grove_key_env_var_names() {
        assert_eq!(grove_key_for("anthropic"), "GROVE_ANTHROPIC_API_KEY");
        assert_eq!(grove_key_for("openai"), "GROVE_OPENAI_API_KEY");
        assert_eq!(grove_key_for("deepseek"), "GROVE_DEEPSEEK_API_KEY");
        assert_eq!(grove_key_for("inception"), "GROVE_INCEPTION_API_KEY");
    }

    #[test]
    fn get_grove_key_returns_none_when_unset() {
        // SAFETY: single-threaded test binary.
        unsafe { std::env::remove_var("GROVE_ANTHROPIC_API_KEY") };
        assert!(get_grove_key("anthropic").is_none());
    }
}
