use crate::config::DaemonConfig;
use anyhow::Result;

pub async fn serve(_cfg: DaemonConfig) -> Result<()> {
    anyhow::bail!("server::serve not implemented — see Task 5")
}
