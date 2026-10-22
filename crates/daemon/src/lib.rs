use crate::config::ConfigStore;
use crate::device::DeviceManager;
use crate::device::hid::AsyncHidSource;
use crate::executor::{Executor, LoggingAudio, LoggingKeyInjector};
use tokio::sync::{broadcast, mpsc};
use crate::platform::enigo::EnigoKeyInjector;

mod event;
mod device;
mod executor;
mod config;
mod platform;

// TODO read config from CLI
pub async fn run() -> anyhow::Result<()> {
    let config = ConfigStore::load("config.toml").await?;
    let (dispatch_tx, dispatch_rx) = mpsc::channel(64);
    let (events_tx, _events_rx) = broadcast::channel(256);

    let executor = Executor::new(
        EnigoKeyInjector::new()?,
        LoggingAudio,
        dispatch_rx
    );
    let manager = DeviceManager::new(
        AsyncHidSource::new(),
        config.subscribe(),
        dispatch_tx,
        events_tx,
    );

    tokio::try_join!(executor.run(), manager.run())?;
    Ok(())
}
