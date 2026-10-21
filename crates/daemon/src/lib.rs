use enigo::Key;
use tokio::sync::{broadcast, mpsc};
use tracing::info;
use daemon_core::action::{AudioTarget, Volume};
use crate::config::ConfigStore;
use crate::device::DeviceManager;
use crate::device::hid::{AsyncHidDevice, AsyncHidSource};
use crate::executor::{AudioBackend, Executor, KeyInjector};

mod event;
mod device;
mod executor;
mod config;

// TODO read config from CLI
pub async fn run() -> anyhow::Result<()> {
    let config = ConfigStore::load("config.toml").await?;
    let (dispatch_tx, dispatch_rx) = mpsc::channel(64);
    let (events_tx, _events_rx) = broadcast::channel(256);

    let executor = Executor::new(LoggingKeyInjector, LoggingAudio, dispatch_rx);
    let manager = DeviceManager::new(
        AsyncHidSource::new(),
        config.subscribe(),
        dispatch_tx,
        events_tx,
    );

    tokio::try_join!(executor.run(), manager.run())?;
    Ok(())
}

struct LoggingKeyInjector;
impl KeyInjector for LoggingKeyInjector {
    fn chord(&mut self, keys: &[Key]) -> anyhow::Result<()> {
        info!("Sending chord: {:?}", keys);
        Ok(())
    }

    fn text(&mut self, text: &str) -> anyhow::Result<()> {
        info!("Sending text: {:?}", text);
        Ok(())
    }
}

struct LoggingAudio;
impl AudioBackend for LoggingAudio {
    async fn set(&self, target: &AudioTarget, volume: Volume) -> anyhow::Result<()> {
        info!("Setting volume for {:?} to {}", target, volume);
        Ok(())
    }
}
