use std::cell::LazyCell;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use crate::config::ConfigStore;
use crate::device::DeviceManager;
use crate::device::hid::AsyncHidSource;
use crate::executor::{Executor, LoggingAudio, LoggingKeyInjector};
use tokio::sync::{broadcast, mpsc};
use crate::platform::enigo::EnigoKeyInjector;
use crate::platform::linux::PulseAudioBackend;

mod event;
mod device;
mod executor;
mod config;
mod platform;

#[cfg(target_os = "linux")]
pub type Etcetera = etcetera::app_strategy::Xdg;
#[cfg(target_os = "windows")]
pub type Etcetera = etcetera::app_strategy::Windows;

pub static ETCETERA: LazyLock<Etcetera> = LazyLock::new(|| {
    let args = etcetera::AppStrategyArgs {
        top_level_domain: "com".to_string(),
        author: "FaultyMuse".to_string(),
        app_name: "FaultyDeck".to_string(),
    };
    etcetera::choose_app_strategy(args).unwrap()
});

pub struct Opts {
    pub config_path: PathBuf,
}

pub async fn run(opts: Opts) -> anyhow::Result<()> {
    let config = ConfigStore::load(opts.config_path).await?;
    let (dispatch_tx, dispatch_rx) = mpsc::channel(64);
    let (events_tx, _events_rx) = broadcast::channel(256);

    let executor = Executor::new(
        EnigoKeyInjector::new()?,
        PulseAudioBackend::spawn(),
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
