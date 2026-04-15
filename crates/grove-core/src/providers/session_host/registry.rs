//! In-memory `SessionHostRegistry` with idle-expiry and LRU capacity caps.

use super::{SessionHostRegistry, SessionKey, host::ClaudeSessionHost};
use crate::errors::GroveResult;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RegistryConfig {
    pub max_hosts: usize,
    pub idle_timeout: Duration,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            max_hosts: 8,
            idle_timeout: Duration::from_secs(900),
        }
    }
}

struct Entry {
    host: Arc<ClaudeSessionHost>,
    last_touch: Instant,
}

pub struct InMemorySessionHostRegistry {
    cfg: RegistryConfig,
    map: Mutex<HashMap<SessionKey, Entry>>,
}

impl InMemorySessionHostRegistry {
    pub fn new(cfg: RegistryConfig) -> Self {
        Self {
            cfg,
            map: Mutex::new(HashMap::new()),
        }
    }

    /// Remove any entries whose `last_touch` is older than `idle_timeout`.
    /// Returns the number evicted.
    pub async fn sweep_idle(&self) -> usize {
        let cutoff = Instant::now() - self.cfg.idle_timeout;
        let to_evict: Vec<SessionKey> = {
            let map = self.map.lock().await;
            map.iter()
                .filter(|(_, e)| e.last_touch < cutoff)
                .map(|(k, _)| k.clone())
                .collect()
        };
        for k in &to_evict {
            self.evict(k).await;
        }
        to_evict.len()
    }

    async fn enforce_capacity(&self) {
        let mut map = self.map.lock().await;
        if map.len() <= self.cfg.max_hosts {
            return;
        }
        let mut entries: Vec<(SessionKey, Instant)> =
            map.iter().map(|(k, e)| (k.clone(), e.last_touch)).collect();
        entries.sort_by_key(|(_, t)| *t);
        let excess = map.len() - self.cfg.max_hosts;
        let victims: Vec<SessionKey> = entries.into_iter().take(excess).map(|(k, _)| k).collect();
        for k in &victims {
            if let Some(entry) = map.remove(k) {
                let host = entry.host;
                tokio::spawn(async move { host.shutdown().await });
            }
        }
    }
}

#[async_trait::async_trait]
impl SessionHostRegistry for InMemorySessionHostRegistry {
    async fn get_or_spawn(
        &self,
        key: SessionKey,
        resume_session_id: Option<String>,
        spawn_fn: Box<
            dyn for<'a> FnOnce(
                    Option<&'a str>,
                ) -> futures::future::BoxFuture<
                    'a,
                    GroveResult<Arc<ClaudeSessionHost>>,
                > + Send,
        >,
    ) -> GroveResult<Arc<ClaudeSessionHost>> {
        {
            let mut map = self.map.lock().await;
            if let Some(e) = map.get_mut(&key) {
                e.last_touch = Instant::now();
                return Ok(e.host.clone());
            }
        }
        let host = spawn_fn(resume_session_id.as_deref()).await?;
        {
            let mut map = self.map.lock().await;
            if let Some(e) = map.get_mut(&key) {
                e.last_touch = Instant::now();
                let dup = host.clone();
                tokio::spawn(async move { dup.shutdown().await });
                return Ok(e.host.clone());
            }
            map.insert(
                key,
                Entry {
                    host: host.clone(),
                    last_touch: Instant::now(),
                },
            );
        }
        self.enforce_capacity().await;
        Ok(host)
    }

    async fn evict(&self, key: &SessionKey) {
        let entry = {
            let mut map = self.map.lock().await;
            map.remove(key)
        };
        if let Some(e) = entry {
            e.host.shutdown().await;
        }
    }

    async fn len(&self) -> usize {
        self.map.lock().await.len()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::session_host::host::ClaudeSessionHost;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn fake_claude_script() -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .prefix("fake-claude-")
            .suffix(".sh")
            .tempfile()
            .unwrap();
        writeln!(
            f,
            r#"#!/bin/sh
while IFS= read -r line; do
  echo '{{"type":"system","session_id":"S","model":"fake"}}'
  echo '{{"type":"result","subtype":"success","session_id":"S","cost_usd":0.0,"is_error":false}}'
done
"#
        )
        .unwrap();
        f.flush().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut p = std::fs::metadata(f.path()).unwrap().permissions();
            p.set_mode(0o755);
            std::fs::set_permissions(f.path(), p).unwrap();
        }
        f
    }

    #[allow(clippy::type_complexity)]
    fn spawn_fake(
        script: std::path::PathBuf,
        cwd: std::path::PathBuf,
    ) -> Box<
        dyn for<'a> FnOnce(
                Option<&'a str>,
            )
                -> futures::future::BoxFuture<'a, GroveResult<Arc<ClaudeSessionHost>>>
            + Send,
    > {
        Box::new(move |sid| {
            Box::pin(async move { ClaudeSessionHost::spawn(&script, &cwd, sid).await })
        })
    }

    #[tokio::test]
    async fn get_or_spawn_reuses_existing() {
        let script = fake_claude_script();
        let tmp = tempfile::tempdir().unwrap();
        let reg = InMemorySessionHostRegistry::new(RegistryConfig::default());
        let key = SessionKey::new("c1", tmp.path());
        let h1 = reg
            .get_or_spawn(
                key.clone(),
                None,
                spawn_fake(script.path().to_path_buf(), tmp.path().to_path_buf()),
            )
            .await
            .unwrap();
        let h2 = reg
            .get_or_spawn(
                key.clone(),
                None,
                spawn_fake(script.path().to_path_buf(), tmp.path().to_path_buf()),
            )
            .await
            .unwrap();
        assert!(
            Arc::ptr_eq(&h1, &h2),
            "second call must return the cached host"
        );
        assert_eq!(reg.len().await, 1);
    }

    #[tokio::test]
    async fn evict_removes_and_shuts_down() {
        let script = fake_claude_script();
        let tmp = tempfile::tempdir().unwrap();
        let reg = InMemorySessionHostRegistry::new(RegistryConfig::default());
        let key = SessionKey::new("c1", tmp.path());
        reg.get_or_spawn(
            key.clone(),
            None,
            spawn_fake(script.path().to_path_buf(), tmp.path().to_path_buf()),
        )
        .await
        .unwrap();
        assert_eq!(reg.len().await, 1);
        reg.evict(&key).await;
        assert_eq!(reg.len().await, 0);
    }

    #[tokio::test]
    async fn sweep_idle_evicts_stale() {
        let script = fake_claude_script();
        let tmp = tempfile::tempdir().unwrap();
        let reg = InMemorySessionHostRegistry::new(RegistryConfig {
            max_hosts: 8,
            idle_timeout: Duration::from_millis(20),
        });
        let key = SessionKey::new("c1", tmp.path());
        reg.get_or_spawn(
            key.clone(),
            None,
            spawn_fake(script.path().to_path_buf(), tmp.path().to_path_buf()),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
        let evicted = reg.sweep_idle().await;
        assert_eq!(evicted, 1);
        assert_eq!(reg.len().await, 0);
    }

    #[tokio::test]
    async fn capacity_lru_evicts_oldest() {
        let script = fake_claude_script();
        let tmp1 = tempfile::tempdir().unwrap();
        let tmp2 = tempfile::tempdir().unwrap();
        let tmp3 = tempfile::tempdir().unwrap();
        let reg = InMemorySessionHostRegistry::new(RegistryConfig {
            max_hosts: 2,
            idle_timeout: Duration::from_secs(600),
        });
        let k1 = SessionKey::new("c1", tmp1.path());
        let k2 = SessionKey::new("c2", tmp2.path());
        let k3 = SessionKey::new("c3", tmp3.path());
        reg.get_or_spawn(
            k1.clone(),
            None,
            spawn_fake(script.path().to_path_buf(), tmp1.path().to_path_buf()),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        reg.get_or_spawn(
            k2.clone(),
            None,
            spawn_fake(script.path().to_path_buf(), tmp2.path().to_path_buf()),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
        reg.get_or_spawn(
            k3.clone(),
            None,
            spawn_fake(script.path().to_path_buf(), tmp3.path().to_path_buf()),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(reg.len().await, 2, "LRU must cap at max_hosts");
    }
}
