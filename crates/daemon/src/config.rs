use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::watch;
use daemon_core::config::Config;

pub struct ConfigStore {
    path: PathBuf,
    tx: watch::Sender<Arc<Config>>
}

impl ConfigStore {
    /// Loads the config from the given path. Also launches a background task to watch the file for changes.
    pub async fn load(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
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
