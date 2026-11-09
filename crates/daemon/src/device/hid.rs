use anyhow::Context;
use async_hid::{AsyncHidFeatureHandle, AsyncHidRead, AsyncHidWrite, DeviceEvent, DeviceFeatureHandle, DeviceId, DeviceReaderWriter, HidBackend};
use futures_lite::{Stream, StreamExt};
use tracing::{debug, trace};
use protocol::{DeviceInfo, DeviceState};
use protocol::command::Command;
use protocol::event::Event;
use crate::device::{DeviceHandle, DeviceSource, HotplugEvent};

pub struct AsyncHidSource {
    backend: HidBackend,
}

impl AsyncHidSource {
    pub fn new() -> Self {
        Self {
            backend: HidBackend::default(),
        }
    }
}

fn device_filter(device: &async_hid::Device) -> bool {
    device.matches(0xFF00, 0x01, 0x1209, 0x0001)
}

impl DeviceSource for AsyncHidSource {
    type Handle = AsyncHidDevice;

    async fn hotplug(&self) -> anyhow::Result<impl Stream<Item = HotplugEvent> + use<>> {
        // Start watcher immediately so we don't miss a hotplug event
        // while enumerating already connected devices.
        let backend = self.backend.clone();
        let mut watcher = backend.watch()?;

        let stream = async_stream::stream! {
            // Next, check if the device is already connected and emit it as a hotplug event in the stream
            {
                let existing = backend.enumerate()
                    .await
                    .unwrap()
                    .find(device_filter)
                    .await;
                if let Some(device) = existing {
                    debug!("Found existing device: {:?}", device);
                    // We shouldn't actually need clone here... not sure why we do
                    yield HotplugEvent::Arrived(device.id.clone());
                }
            }

            // Lastly, listen for hotplug events
            while let Some(event) = watcher.next().await {
                match event {
                    DeviceEvent::Connected(id) => {
                        let device = backend.query_devices(&id)
                            .await
                            .unwrap()
                            .find(device_filter);
                        if let Some(device) = device {
                            debug!("Device connected: {:?}", device);
                            yield HotplugEvent::Arrived(id);
                        }
                    }
                    DeviceEvent::Disconnected(id) => {
                        debug!("Device disconnected: {:?}", id);
                        yield HotplugEvent::Left(id)
                    }
                }

            }
        };

        Ok(stream)
    }

    async fn open(&self, id: &DeviceId) -> anyhow::Result<Self::Handle> {
        let device = self.backend.query_devices(id)
            .await?
            .find(device_filter)
            .context("Device not found")?;

        AsyncHidDevice::open(device).await
    }
}

pub struct AsyncHidDevice {
    feature_handle: DeviceFeatureHandle,
    device_handle: DeviceReaderWriter,
}

impl AsyncHidDevice {
    async fn open(device: async_hid::Device) -> anyhow::Result<Self> {
        Ok(Self {
            feature_handle: device.open_feature_handle().await?,
            device_handle: device.open().await?,
        })
    }
}

impl DeviceHandle for AsyncHidDevice {
    async fn info(&mut self) -> anyhow::Result<DeviceInfo> {
        let mut buf = [0; 65]; // 1 byte for report ID, plus 64 bytes payload
        self.feature_handle.read_feature_report(&mut buf).await?;
        let device_info = protocol::decode(&buf[1..])?;
        Ok(device_info)
    }

    async fn send(&mut self, command: &Command) -> anyhow::Result<()> {
        let report = protocol::encode(command)?;
        self.device_handle.write_output_report(&report).await?;
        Ok(())
    }

    async fn event(&mut self) -> anyhow::Result<Event> {
        let mut buf: protocol::Report = [0; _];
        self.device_handle.read_input_report(&mut buf).await?;
        Ok(protocol::decode::<Event>(&buf)?)
    }
}
