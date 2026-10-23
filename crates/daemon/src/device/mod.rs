pub mod hid;

use std::collections::HashMap;
use std::pin::pin;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use async_hid::DeviceId;
use tokio::select;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::{AbortHandle, JoinSet};
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, error, info, warn};
use daemon_core::action::{Action, Volume, VolumeOp};
use daemon_core::config::Config;
use daemon_core::event::DeviceEvent;
use daemon_core::tracker::DeviceTracker;
use protocol::{DeviceInfo, DeviceState, Serial, MAX_SLIDER_VALUE, PROTOCOL_VERSION};
use crate::event::DeviceEventEnvelope;

pub enum HotplugEvent {
    Arrived(DeviceId),
    Left(DeviceId),
}

/// Represents a source of HID device hotplug events.
pub trait DeviceSource {
    type Handle: DeviceHandle + Send + Sync + 'static;

    /// Listen for HID hotplug events.
    fn hotplug(&self) -> impl Future<Output=Result<impl Stream<Item = HotplugEvent> + use<Self>>> + Send;

    /// Open a connection to a HID device.
    fn open(&self, id: &DeviceId) -> impl Future<Output=Result<Self::Handle>> + Send;
}

/// Represents an open connection to an HID device.
pub trait DeviceHandle {
    /// Fetch the device's advertised metadata.
    fn info(&mut self) -> impl Future<Output=Result<DeviceInfo>> + Send;

    /// Fetch the current state of the device.
    fn state(&mut self) -> impl Future<Output=Result<DeviceState>> + Send;

    /// Blocks until the next input report is received.
    fn next(&mut self) -> impl Future<Output=Result<DeviceState>> + Send;
}

#[derive(Clone)]
pub struct SessionChannels {
    config: watch::Receiver<Arc<Config>>,
    dispatch: mpsc::Sender<Action>,
    events: broadcast::Sender<DeviceEventEnvelope>,
}

pub struct DeviceManager<S: DeviceSource> {
    source: S,
    sessions: HashMap<DeviceId, SessionHandle>,
    /// Set of running tasks, we keep this as well as [`sessions`] so that we can detect
    /// if a task exits early (eg unrecoverable read error)
    tasks: JoinSet<(DeviceId, Result<()>)>,
    /// Needed just to clone into sessions
    channels: SessionChannels,
}

impl<S: DeviceSource> DeviceManager<S> {

    pub fn new(
        source: S,
        config: watch::Receiver<Arc<Config>>,
        dispatch: mpsc::Sender<Action>,
        events: broadcast::Sender<DeviceEventEnvelope>
    ) -> Self {
        let sessions = HashMap::new();
        let tasks = JoinSet::new();

        let channels = SessionChannels {
            config,
            dispatch,
            events,
        };

        Self {
            source,
            sessions,
            tasks,
            channels
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Waiting for device...");
        let mut hotplug = pin!(self.source.hotplug().await?);
        loop {
            select! {
                Some(event) = hotplug.next() => {
                    self.handle_hotplug_event(event).await;
                }
                Some(Ok((device_id, result))) = self.tasks.join_next() => {
                    self.handle_session_exit(device_id, result).await;
                }
            }
        }
    }

    async fn handle_hotplug_event(&mut self, event: HotplugEvent) {
        match event {
            HotplugEvent::Arrived(device_id) => {
                debug!("Device {:?} arrived. Attempting to connect", device_id);
                // Open a new handle to the device
                match self.start_session(device_id.clone()).await {
                    Ok(session) => {
                        let serial = session.serial;
                        let old = self.sessions.insert(device_id, session);
                        if let Some(old) = old {
                            warn!("Device {} already had an active session. Aborting the old session", serial);
                            old.abort.abort();
                        }
                    }
                    Err(err) => {
                        error!("Failed to start session for device {:?}: {}", device_id, err);
                    }
                }
            }
            HotplugEvent::Left(device) => {
                if let Some(existing) = self.sessions.remove(&device) {
                    info!("Device {} disconnected. Aborting session.", existing.serial);
                    existing.abort.abort();
                }
            }
        }
    }

    async fn handle_session_exit(&mut self, device_id: DeviceId, result: Result<()>) {
        // Just remove the session from the map.
        // Don't need to abort it because the session has already exited.
        self.sessions.remove(&device_id);

        match result {
            Ok(()) => {
                info!("Device session ({device_id:?}) exited early");
            }
            Err(err) => {
                warn!("Device session ({device_id:?}) exited early with error: {}", err);
            }
        }
    }

    async fn start_session(&mut self, device_id: DeviceId) -> Result<SessionHandle> {
        let handle = self.source.open(&device_id).await?;
        let session = DeviceSession::from_handle(self.channels.clone(), handle).await?;
        let serial = session.device_info.serial;

        let abort = self.tasks.spawn(async move {
            (device_id, session.run().await)
        });

        Ok(SessionHandle { serial, abort })
    }
}

/// Wrapper around the tokio [`AbortHandle`] so we can cancel a session task when the device disconnects.
pub struct SessionHandle {
    serial: Serial,
    abort: AbortHandle,
}

pub struct DeviceSession<H: DeviceHandle> {
    device_info: DeviceInfo,
    handle: H,
    tracker: DeviceTracker,
    channels: SessionChannels,
}

impl<H: DeviceHandle> DeviceSession<H> {
    async fn from_handle(channels: SessionChannels, mut handle: H) -> Result<Self> {
        let device_info = handle.info().await?;
        info!(
            serial=%device_info.serial,
            "Device connected. Protocol version: {} Firmware version: {}",
            device_info.protocol_version,
            device_info.firmware_version,
        );

        if device_info.protocol_version != PROTOCOL_VERSION {
            warn!("Device using unsupported protocol version. Required: {}. {:?}", PROTOCOL_VERSION, device_info);
            return Err(anyhow!("Unsupported protocol version"));
        }

        let mut this = Self {
            device_info: device_info.clone(),
            handle,
            tracker: DeviceTracker::new(device_info),
            channels,
        };

        let initial_state = this.handle.state().await?;
        let events = this.tracker.resync(initial_state);
        this.emit_events(&events).await;

        Ok(this)
    }

    async fn run(mut self) -> Result<()> {
        loop {
            select! {
                state = self.handle.next() => {
                    match state {
                        Ok(state) => {
                            self.handle_new_state(state).await;
                        }
                        Err(err) => {
                            warn!("Error receiving device state: {}", err);
                        }
                    }
                }
                result = self.channels.config.changed() => {
                    self.handle_config_change(result.is_err())?;
                }
            }
        }
    }

    async fn handle_new_state(&mut self, new_state: DeviceState) {
        let events = self.tracker.apply(new_state);
        self.emit_events(&events).await;
    }

    fn handle_config_change(&mut self, error: bool) -> Result<()> {
        if error {
            error!("Config watcher died :(");
        } else {
            let errors = self.channels.config
                .borrow_and_update()
                .check_device(&self.device_info);
            if !errors.is_empty() {
                error!("Config errors: {:?}", errors);
                return Err(anyhow!("Invalid configuration for device: {:?}", errors));
            }
        }

        Ok(())
    }

    async fn emit_events(&self, events: &[DeviceEvent]) {
        for event in events {
            let envelope = DeviceEventEnvelope::new(self.device_info.serial, *event);
            // Events channel is purely informational, so we don't care if it's full
            let _ = self.channels.events.send(envelope);

            if let Some(action) = self.event_to_action(*event) {
                debug!("Device({}) emitting action: {:?}", self.device_info.serial, action);

                if let Err(e) = self.channels.dispatch.send(action).await {
                    error!("Failed to send event to dispatch channel: {:?}", e.0);
                };
            }
        }
    }

    fn event_to_action(&self, event: DeviceEvent) -> Option<Action> {
        let config = self.channels.config.borrow();
        match event {
            DeviceEvent::Button { id, pressed } => {
                // TODO pipe into gesture detection
                if pressed {
                    config.device.action(id).cloned()
                } else {
                    None
                }
            }
            DeviceEvent::Slider { id, value } => {
                let target = config.device.slider_target(id)?.clone();

                Some(Action::Volume {
                    target,
                    op: VolumeOp::Set(Volume::from_slider(value, MAX_SLIDER_VALUE))
                })
            }
        }
    }
}
