use std::collections::HashMap;
use std::fs;
use std::sync::{LazyLock, Mutex};

use crate::config::paths::tracker_credentials_path;
use crate::errors::{GroveError, GroveResult};

const SERVICE_PREFIX: &str = "grove";

/// All fields for one provider are stored under this single keychain username
/// as a JSON object — so macOS only ever prompts once per provider, not once
/// per field (email, token, site-url, …).
const BUNDLE_KEY: &str = "credentials";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CredentialStorage {
    Keychain,
    File,
}

impl Default for CredentialStorage {
    fn default() -> Self {
        Self::Keychain
    }
}

impl CredentialStorage {
    pub fn parse(value: &str) -> GroveResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "keychain" | "os" | "system" => Ok(Self::Keychain),
            "file" | "grove" | "grove-file" => Ok(Self::File),
            other => Err(GroveError::Runtime(format!(
                "unknown credential storage '{other}'"
            ))),
        }
    }
}

#[derive(Default)]
struct CredentialState {
    keychain_cache: HashMap<String, HashMap<String, String>>,
}

/// Shared in-process state for serialized keychain/file access.
static CRED_STATE: LazyLock<Mutex<CredentialState>> =
    LazyLock::new(|| Mutex::new(CredentialState::default()));

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct FileCredentialStore {
    #[serde(default)]
    providers: HashMap<String, FileProviderRecord>,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct FileProviderRecord {
    #[serde(default)]
    storage: CredentialStorage,
    #[serde(default)]
    fields: HashMap<String, String>,
}

/// OS-keychain-backed credential store.
///
/// All credentials for one provider (e.g. "jira") are stored together in a
/// single keychain entry as a JSON object, so macOS prompts **once per
/// provider** per session rather than once per field.
///
/// Platform backends (via the `keyring` crate):
/// - macOS  → Keychain Services
/// - Linux  → Secret Service (libsecret / GNOME Keyring)
/// - Windows → Credential Manager
pub struct CredentialStore;

impl CredentialStore {
    // ── Internal helpers ──────────────────────────────────────────────────────

    fn service_name(provider: &str) -> String {
        format!("{SERVICE_PREFIX}-{provider}")
    }

    fn load_file_store() -> GroveResult<FileCredentialStore> {
        let path = tracker_credentials_path();
        match fs::read_to_string(&path) {
            Ok(json) => Ok(serde_json::from_str(&json)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(FileCredentialStore::default())
            }
            Err(e) => Err(e.into()),
        }
    }

    fn save_file_store(store: &FileCredentialStore) -> GroveResult<()> {
        let path = tracker_credentials_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(store)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Read the JSON bundle for `service` from the OS keychain.
    /// Returns an empty map when no entry exists yet.
    /// Caller must hold `CRED_STATE` lock (or call this only from within a
    /// locked section) to keep prompt serialization guarantees.
    fn load_bundle(service: &str) -> GroveResult<HashMap<String, String>> {
        let entry = keyring::Entry::new(service, BUNDLE_KEY)
            .map_err(|e| GroveError::Runtime(format!("keyring init error: {e}")))?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str::<HashMap<String, String>>(&json)
                .map_err(|e| GroveError::Runtime(format!("keyring parse error: {e}"))),
            Err(keyring::Error::NoEntry) => Ok(HashMap::new()),
            Err(e) => Err(GroveError::Runtime(format!("keyring retrieve error: {e}"))),
        }
    }

    /// Serialise `bundle` and write it back to the OS keychain.
    fn save_bundle(service: &str, bundle: &HashMap<String, String>) -> GroveResult<()> {
        let json = serde_json::to_string(bundle)
            .map_err(|e| GroveError::Runtime(format!("keyring serialize error: {e}")))?;
        let entry = keyring::Entry::new(service, BUNDLE_KEY)
            .map_err(|e| GroveError::Runtime(format!("keyring init error: {e}")))?;
        entry
            .set_password(&json)
            .map_err(|e| GroveError::Runtime(format!("keyring store error: {e}")))?;
        Ok(())
    }

    /// Ensure the bundle for `service` is in `cache`.
    /// Loads from keychain (one OS prompt) on the first call; no-op thereafter.
    fn ensure_loaded(cache: &mut CredentialState, service: &str) -> GroveResult<()> {
        if !cache.keychain_cache.contains_key(service) {
            let bundle = Self::load_bundle(service)?;
            cache.keychain_cache.insert(service.to_string(), bundle);
        }
        Ok(())
    }

    fn delete_keychain_service(cache: &mut CredentialState, service: &str) -> GroveResult<()> {
        let entry = keyring::Entry::new(service, BUNDLE_KEY)
            .map_err(|e| GroveError::Runtime(format!("keyring init error: {e}")))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(GroveError::Runtime(format!("keyring delete error: {e}"))),
        }
        cache.keychain_cache.remove(service);
        Ok(())
    }

    fn delete_keychain_provider(cache: &mut CredentialState, provider: &str) -> GroveResult<()> {
        let service = Self::service_name(provider);
        Self::delete_keychain_service(cache, &service)
    }

    fn keychain_bundle_mut<'a>(
        cache: &'a mut CredentialState,
        provider: &str,
    ) -> GroveResult<&'a mut HashMap<String, String>> {
        let service = Self::service_name(provider);
        Self::ensure_loaded(cache, &service)?;
        Ok(cache.keychain_cache.get_mut(&service).unwrap())
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Persist `secret` for `(provider, key)`.
    ///
    /// Loads the provider's existing bundle first (one keychain read), updates
    /// the field, and writes the bundle back — keeping all fields intact.
    pub fn store(provider: &str, key: &str, secret: &str) -> GroveResult<()> {
        Self::store_with_storage(provider, key, secret, CredentialStorage::Keychain)
    }

    pub fn store_with_storage(
        provider: &str,
        key: &str,
        secret: &str,
        storage: CredentialStorage,
    ) -> GroveResult<()> {
        let mut updates = HashMap::new();
        updates.insert(key.to_string(), secret.to_string());
        Self::store_bundle_with_storage(provider, updates, storage)
    }

    pub fn store_bundle_with_storage(
        provider: &str,
        updates: HashMap<String, String>,
        storage: CredentialStorage,
    ) -> GroveResult<()> {
        let mut state = CRED_STATE.lock().unwrap();
        let mut file_store = Self::load_file_store()?;

        match storage {
            CredentialStorage::Keychain => {
                let service = Self::service_name(provider);
                let bundle = Self::keychain_bundle_mut(&mut state, provider)?;
                for (key, value) in updates {
                    bundle.insert(key, value);
                }
                Self::save_bundle(&service, bundle)?;
                file_store.providers.insert(
                    provider.to_string(),
                    FileProviderRecord {
                        storage: CredentialStorage::Keychain,
                        fields: HashMap::new(),
                    },
                );
                Self::save_file_store(&file_store)?;
            }
            CredentialStorage::File => {
                let record = file_store
                    .providers
                    .entry(provider.to_string())
                    .or_insert_with(|| FileProviderRecord {
                        storage: CredentialStorage::File,
                        fields: HashMap::new(),
                    });
                record.storage = CredentialStorage::File;
                for (key, value) in updates {
                    record.fields.insert(key, value);
                }
                Self::save_file_store(&file_store)?;
                Self::delete_keychain_provider(&mut state, provider)?;
            }
        }

        Ok(())
    }

    /// Retrieve `secret` for `(provider, key)`.
    ///
    /// The first call for any key of a provider loads the entire bundle from the
    /// OS keychain (triggering at most one macOS prompt for that provider).
    /// All subsequent calls for any key of the same provider are served from the
    /// in-process cache with no keychain access.
    pub fn retrieve(provider: &str, key: &str) -> GroveResult<Option<String>> {
        let service = Self::service_name(provider);
        let mut state = CRED_STATE.lock().unwrap();
        let file_store = Self::load_file_store()?;
        let preferred = file_store
            .providers
            .get(provider)
            .map(|record| record.storage);

        match preferred {
            Some(CredentialStorage::File) => Ok(file_store
                .providers
                .get(provider)
                .and_then(|record| record.fields.get(key))
                .cloned()),
            Some(CredentialStorage::Keychain) => {
                Self::ensure_loaded(&mut state, &service)?;
                Ok(state
                    .keychain_cache
                    .get(&service)
                    .and_then(|bundle| bundle.get(key))
                    .cloned())
            }
            None => {
                Self::ensure_loaded(&mut state, &service)?;
                let from_keychain = state
                    .keychain_cache
                    .get(&service)
                    .and_then(|bundle| bundle.get(key))
                    .cloned();
                if from_keychain.is_some() {
                    return Ok(from_keychain);
                }
                Ok(file_store
                    .providers
                    .get(provider)
                    .and_then(|record| record.fields.get(key))
                    .cloned())
            }
        }
    }

    /// Delete `(provider, key)` from the OS keychain and the in-process cache.
    ///
    /// If no fields remain for the provider, the keychain entry is removed
    /// entirely; otherwise the bundle is updated in place.
    pub fn delete(provider: &str, key: &str) -> GroveResult<()> {
        let service = Self::service_name(provider);
        let mut state = CRED_STATE.lock().unwrap();
        let mut file_store = Self::load_file_store()?;

        Self::ensure_loaded(&mut state, &service)?;
        if let Some(bundle) = state.keychain_cache.get_mut(&service) {
            bundle.remove(key);
            if bundle.is_empty() {
                Self::delete_keychain_service(&mut state, &service)?;
            } else {
                let bundle = state.keychain_cache.get(&service).unwrap().clone();
                Self::save_bundle(&service, &bundle)?;
            }
        }

        if let Some(record) = file_store.providers.get_mut(provider) {
            record.fields.remove(key);
            let keychain_has_values = state
                .keychain_cache
                .get(&service)
                .map(|bundle| !bundle.is_empty())
                .unwrap_or(false);
            let file_has_values = !record.fields.is_empty();
            if !keychain_has_values && !file_has_values {
                file_store.providers.remove(provider);
            }
        }
        Self::save_file_store(&file_store)?;
        Ok(())
    }

    pub fn delete_provider(provider: &str) -> GroveResult<()> {
        let mut state = CRED_STATE.lock().unwrap();
        let mut file_store = Self::load_file_store()?;
        Self::delete_keychain_provider(&mut state, provider)?;
        file_store.providers.remove(provider);
        Self::save_file_store(&file_store)?;
        Ok(())
    }

    /// Return `true` if a secret exists for `(provider, key)`.
    /// Uses the in-process cache — no extra keychain prompt.
    pub fn has(provider: &str, key: &str) -> bool {
        Self::retrieve(provider, key)
            .map(|v| v.is_some())
            .unwrap_or(false)
    }
}

/// Connection status for a tracker provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionStatus {
    pub provider: String,
    pub connected: bool,
    pub user_display: Option<String>,
    pub error: Option<String>,
}

impl ConnectionStatus {
    pub fn ok(provider: &str, user_display: &str) -> Self {
        Self {
            provider: provider.to_string(),
            connected: true,
            user_display: Some(user_display.to_string()),
            error: None,
        }
    }

    pub fn disconnected(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
            connected: false,
            user_display: None,
            error: None,
        }
    }

    pub fn err(provider: &str, message: &str) -> Self {
        Self {
            provider: provider.to_string(),
            connected: false,
            user_display: None,
            error: Some(message.to_string()),
        }
    }
}
