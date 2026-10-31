use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use anyhow::Context;
use notify::{EventKind, RecursiveMode, Watcher};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};
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

        let config = read_config(&path).await?;
        let (tx, _) = watch::channel(Arc::new(config));

        spawn_watcher(&path, tx.clone())?;

        Ok(Self { path, tx })
    }
    
    pub fn subscribe(&self) -> watch::Receiver<Arc<Config>> {
        self.tx.subscribe()
    }
}

async fn read_config(path: &Path) -> anyhow::Result<Config> {
    let bytes = tokio::fs::read(path).await?;
    Ok(toml::from_slice(&bytes)?)
}

fn spawn_watcher(config_path: &Path, tx: watch::Sender<Arc<Config>>) -> anyhow::Result<()> {
    // Watch the parent directory as some editors will write to a temp file then replace the directory
    // instead of updating the file in place
    let watch_dir = match config_path.parent() {
        // If `config_path` has a parent,
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |e| {
        let _ = event_tx.send(e);
    })?;
    watcher.watch(&watch_dir, RecursiveMode::NonRecursive)?;

    let config_path = config_path.to_path_buf();
    tokio::spawn(async move {
        let _watcher = watcher; // capture the watcher so it doesn't get dropped
        while let Some(e) = event_rx.recv().await {
            let event = match e {
                Ok(e) => e,
                Err(e) => {
                    warn!("Error watching config file: {}", e);
                    continue;
                }
            };

            // Ignore reads and events for files other than the config file
            let is_relevant = !matches!(event.kind, EventKind::Access(_))
                && event.paths.iter().any(|p| p.file_name() == config_path.file_name());
            if !is_relevant {
                continue;
            }

            // Debounce events.
            // Note this means rapid prolonged saves will never fire an update to the ConfigStore
            loop {
                match tokio::time::timeout(Duration::from_millis(100), event_rx.recv()).await {
                    Ok(Some(_)) => continue,
                    Ok(None) => return, // watcher dropped
                    Err(_) => break,    // quiet period elapsed
                }
            }

            match read_config(&config_path).await {
                Ok(config) => {
                    info!("Config reloaded from {}", config_path.display());
                    // send_replace works even if there are no receivers unlike `send`
                    tx.send_replace(Arc::new(config));
                }
                Err(e) => {
                    warn!("Error reading config file. Keeping previous\n{:#}", e);
                }
            }
        }
    });

    Ok(())
}