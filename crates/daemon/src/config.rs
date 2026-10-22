use std::path::PathBuf;
use std::sync::Arc;
use anyhow::Context;
use tokio::sync::watch;
use tracing::{debug, info};
use daemon_core::config::Config;

pub struct ConfigStore {
    path: PathBuf,
    tx: watch::Sender<Arc<Config>>
}

impl ConfigStore {
    /// Loads the config from the given path. Also launches a background task to watch the file for changes.
    pub async fn load(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        info!("Loading config from {}", path.display());

        if !path.exists() {
            info!("Config file doesn't exist, creating a blank one");
            if let Some(parent) = path.parent() {
                debug!("Creating parent directory: {}", parent.display());
                tokio::fs::create_dir_all(parent).await?;
            }

            let config_str = toml::to_string(&Config::default())?;
            tokio::fs::write(&path, config_str).await?;
        }

        let bytes = tokio::fs::read(&path).await?;
        let config = toml::from_slice(&bytes)?;

        // TODO actually launch the watcher task

        let (tx, _) = watch::channel(Arc::new(config));
        Ok(Self { path, tx })
    }
    
    pub fn subscribe(&self) -> watch::Receiver<Arc<Config>> {
        self.tx.subscribe()
    }
}
