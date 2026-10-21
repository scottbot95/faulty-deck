use tokio::time::Instant;
use daemon_core::event::DeviceEvent;
use protocol::Serial;

#[derive(Debug, Clone)]
pub struct DeviceEventEnvelope {
    pub device: Serial,
    pub at: Instant,
    pub event: DeviceEvent,
}

impl DeviceEventEnvelope {
    pub fn new(device: Serial, event: DeviceEvent) -> Self {
        Self {
            device,
            at: Instant::now(),
            event,
        }
    }
}
