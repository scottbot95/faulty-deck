use async_hid::{DeviceEvent, HidBackend};
use futures_lite::StreamExt;
use tracing::debug;

/// Gets a [`async_hid::Device`] handle for the FaultyDeck.
/// If the device isn't already connected, waits for a hotplug event.
async fn wait_for_device(backend: &HidBackend) -> anyhow::Result<async_hid::Device> {
    // Start watcher immediately so we don't miss a hotplug event
    // while enumerating already connected devices.
    let mut watcher = backend.watch()?;

    // Next, check if the device is already connected
    {
        let existing = backend.enumerate().await?
            .find(|d| d.matches(0xFF00, 0x01, 0x1209, 0x0001))
            .await;
        if let Some(device) = existing {
            debug!("Found existing device: {:?}", device);
            return Ok(device);
        }
    }

    // Lastly, wait for a matching Connected event.
    while let Some(event) = watcher.next().await {
        let DeviceEvent::Connected(id) = event else { continue; };
        let device = backend.query_devices(&id)
            .await?
            .find(|d| d.matches(0xFF00, 0x01, 0x1209, 0x0001));
        if let Some(device) = device {
            debug!("Device connected: {:?}", device);
            return Ok(device);
        }
    }

    Err(anyhow::anyhow!("HID hotplug stream ended unexpectedly"))
}

/// Waits for a FaultyDeck device to be connected, then runs a function.
/// If the function exits waits for the device to be connected again, then calls the function again.
pub async fn with_reconnect(
    backend: &HidBackend,
    mut f: impl AsyncFnMut(async_hid::Device) -> anyhow::Result<()>
) -> anyhow::Result<()> {
    loop {
        let device = wait_for_device(backend).await?;
        let _ = f(device).await;
        debug!("Device disconnected");
    }
}
