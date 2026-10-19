use async_hid::{AsyncHidRead, HidBackend};
use clap::Parser;
use tracing::trace;
use crate::cli::Cli;
use crate::usb::with_reconnect;

mod cli;
mod usb;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_max_level(args.verbosity)
        .init();

    let backend = HidBackend::default();

    with_reconnect(&backend, |device: async_hid::Device| async move {
        let mut buf: protocol::Report = [0u8; 64];
        device.read_feature_report(&mut buf).await?;
        let vendor_info = protocol::decode::<protocol::DeviceInfo>(&buf)?;
        trace!("Handshake complete: {:?}", vendor_info);

        let mut handle = device.open().await?;

        loop {
            let mut buf: protocol::Report = [0u8; 64];
            handle.read_input_report(&mut buf).await?;
            let decoded = protocol::decode::<protocol::DeviceState>(&buf)?;
            trace!("Device state: {:?}", decoded);
        }
    }).await?;

    Ok(())
}