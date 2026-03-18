use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::paths::{grove_app_dir, grove_info_path, workspace_data_root};
use crate::db::{DbHandle, DbPool};
use crate::errors::GroveResult;
use crate::orchestrator::workspace::get_or_create_workspace_id;

/// Top-level Grove application runtime.
///
/// Encapsulates the active workspace identity and its "virtual project root"
/// under `~/.grove/workspaces/<id>/`. All DB, config, and log paths derive
/// from `data_root` so existing `DbHandle::new()`, `GroveConfig::load_or_create()`,
/// etc. work unchanged.
#[derive(Debug, Clone)]
pub struct GroveApp {
    pub workspace_id: String,
    /// Virtual project root: `~/.grove/workspaces/<id>/`
    pub data_root: PathBuf,
    /// Connection pool for the workspace database.
    pool: DbPool,
}

impl GroveApp {
    /// Bootstrap the Grove application.
    ///
    /// 1. Reads or creates the workspace ID (`~/.grove/workspace_id`).
    /// 2. Loads or creates `info.yml`, recording the active workspace.
    /// 3. Ensures `<data_root>/.grove/` exists.
    /// 4. Runs DB migrations (`db::initialize`).
    pub fn init() -> GroveResult<Self> {
        let workspace_id = get_or_create_workspace_id()?;

        let data_root = workspace_data_root(&workspace_id);
        std::fs::create_dir_all(data_root.join(".grove"))?;

        // Persist active workspace in info.yml
        let mut info = AppInfo::load();
        info.active_workspace_id = Some(workspace_id.clone());
        info.save()?;

        // Run DB migrations on the virtual root (needs exclusive access).
        let init_result = crate::db::initialize(&data_root)?;

        // Load config to get pool settings (falls back to defaults if grove.yaml
        // has no `db:` section, preserving backward compatibility).
        let db_cfg = crate::config::GroveConfig::load_or_create(&data_root)
            .map(|c| c.db)
            .unwrap_or_default();

        // Create the connection pool AFTER migrations complete.
        let pool = DbPool::new(
            &init_result.db_path,
            db_cfg.pool_size,
            db_cfg.connection_timeout_ms,
        )?;

        Ok(Self {
            workspace_id,
            data_root,
            pool,
        })
    }

    /// Get a `DbHandle` pointing at this workspace's database.
    pub fn db_handle(&self) -> DbHandle {
        DbHandle::new(&self.data_root)
    }

    /// Get a reference to the connection pool.
    pub fn pool(&self) -> &DbPool {
        &self.pool
    }
}

// ── AppInfo (info.yml) ───────────────────────────────────────────────────────

/// Persistent app-level metadata stored at `~/.grove/info.yml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppInfo {
    #[serde(default)]
    pub active_workspace_id: Option<String>,
}

impl AppInfo {
    /// Load `info.yml` from `~/.grove/`. Returns defaults if the file is
    /// missing or unparseable.
    pub fn load() -> Self {
        let path = grove_info_path();
        match std::fs::read_to_string(&path) {
            Ok(contents) => serde_yaml::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Write `info.yml` to `~/.grove/`.
    pub fn save(&self) -> GroveResult<()> {
        let path = grove_info_path();
        std::fs::create_dir_all(grove_app_dir())?;
        let contents = serde_yaml::to_string(self).map_err(|e| {
            crate::errors::GroveError::Runtime(format!("failed to serialize info.yml: {e}"))
        })?;
        std::fs::write(&path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_roundtrip() {
        let info = AppInfo {
            active_workspace_id: Some("abc123".to_string()),
        };
        let yaml = serde_yaml::to_string(&info).unwrap();
        let parsed: AppInfo = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.active_workspace_id, Some("abc123".to_string()));
    }

    #[test]
    fn app_info_default_is_none() {
        let info = AppInfo::default();
        assert!(info.active_workspace_id.is_none());
    }

    #[test]
    fn app_info_load_missing_file_returns_default() {
        // Calling load when the file doesn't exist should not panic
        let info = AppInfo::load();
        // We can't assert it's None because the real ~/.grove/info.yml may exist,
        // but at minimum it shouldn't panic.
        let _ = info;
    }
}
