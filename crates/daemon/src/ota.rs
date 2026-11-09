use std::path::{Path, PathBuf};
use std::pin::pin;
use std::time::Duration;
use anyhow::{bail, Context, Result, anyhow};
use async_hid::DeviceId;
use sha2::{Digest, Sha256};
use tokio::time::{sleep, timeout};
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, error, info, warn};
use protocol::command::{Command, RebootMagic};
use protocol::{UpdateResult, PROTOCOL_VERSION, VERSION_STRING_MAX_LEN, UPDATE_CHUNK_LEN};
use protocol::event::{Event, UpdateError, UpdateEvent};
use crate::device::{DeviceHandle, DeviceSource, HotplugEvent};
use crate::device::hid::{AsyncHidDevice, AsyncHidSource};

const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const EVENT_TIMEOUT: Duration = Duration::from_secs(2);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(50);
const COMMIT_TIMEOUT: Duration = Duration::from_secs(120);
const REBOOT_TIMEOUT: Duration = Duration::from_secs(45);
const CONFIRM_RETRY_DELAY: Duration = Duration::from_secs(1);
const SETTLE_DELAY: Duration = Duration::from_millis(500);
const MAX_CONFIRM_RETRIES: u32 = 5;
const MAX_REWINDS: u32 = 20;
const SECTOR_LEN: u32 = 4096;
const PROGRESS_INTERVAL: u32 = 64 * 1024;

const IMAGE_MAGIC: u8 = 0xE9;
const APP_DESC_OFFSET: usize = 0x20;
const APP_DESC_MAGIC: u32 = 0xABCD_5432;
const APP_DESC_VERSION: core::ops::Range<usize> = (16 + APP_DESC_OFFSET)..(48 + APP_DESC_OFFSET);
const MAX_IMAGE_SIZE: u32 = 0x400000; // Keep in sync with partitions.csv

const DAEMON_RUNNING_HINT: &str =
    "if a faulty-deck daemon is running it must be stopped to prevent issues during update.";

pub struct UpdateOpts {
    pub image: PathBuf,
}

#[derive(Debug)]
struct Image {
    data: Vec<u8>,
    sha256: [u8; 32],
    version: String,
}

impl Image {
    async fn load(path: &Path) -> Result<Self> {
        let data = tokio::fs::read(path)
            .await
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::parse(data)
            .with_context(|| format!("invalid firmware image {}", path.display()))
    }

    fn parse(data: Vec<u8>) -> Result<Self> {
        if data.first() != Some(&IMAGE_MAGIC) {
            bail!("missing image magic 0x{IMAGE_MAGIC:02X}");
        }
        if data.len() > MAX_IMAGE_SIZE as usize {
            bail!("image size exceeds maximum allowed size of {MAX_IMAGE_SIZE} bytes");
        }

        let magic = data
            .get(APP_DESC_OFFSET..APP_DESC_OFFSET + 4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()));
        if magic != Some(APP_DESC_MAGIC) {
            bail!("missing esp_app_desc magic at offset 0x{APP_DESC_OFFSET:X}");
        }

        let raw_version = data
            .get(APP_DESC_VERSION)
            .context("truncated esp_app_desc")?;
        let len = raw_version
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(raw_version.len());
        let full_version = std::str::from_utf8(&raw_version[..len]).context("app version is not UTF-8")?;
        let version = truncate_version(full_version);
        if version.len() < full_version.len() {
            warn!("Image version {full_version:?} is longer than {VERSION_STRING_MAX_LEN} bytes; sending {version:?}");
        }

        let sha256 = Sha256::digest(&data).into();
        Ok(Self {
            version: version.to_owned(),
            sha256,
            data,
        })
    }

    fn size(&self) -> u32 {
        self.data.len() as u32
    }
}

fn truncate_version(version: &str) -> &str {
    let mut end = version.len().min(VERSION_STRING_MAX_LEN);
    while !version.is_char_boundary(end) {
        end -= 1;
    }
    &version[..end]
}

pub async fn update(opts: UpdateOpts) -> Result<()> {
    let image = Image::load(&opts.image).await?;

    let source = AsyncHidSource::new();
    let mut hotplug = pin!(source.hotplug().await?);
    let id = next_arrival(&mut hotplug, DISCOVERY_TIMEOUT)
        .await
        .context("no faulty-deck device found")?;

    let mut dev = source.open(&id).await?;
    run_update(&mut dev, &image).await?;
    drop(dev);

    info!("Waiting for the device to reboot");
    let mut dev = reconnect(&source, &id, &mut hotplug).await?;
    verify(&mut dev, &image).await
}

/// Wait for the next device arrival event.
async fn next_arrival(
    hotplug: &mut (impl Stream<Item = HotplugEvent> + Unpin),
    wait: Duration,
) -> Result<DeviceId> {
    timeout(wait, async {
        while let Some(event) = hotplug.next().await {
            if let HotplugEvent::Arrived(id) = event {
                return Ok(id);
            }
        }
        Err(anyhow!("hotplug stream ended"))
    }).await.map_err(|_| anyhow!("timed out after {wait:?}"))?
}

/// Waits for a departure and then an arrival.
/// MUST reuse the same Hotplug stream as [`next_arrival`] to ensure no missed events
async fn reconnect<S: DeviceSource>(
    source: &S,
    orig_id: &DeviceId,
    hotplug: &mut (impl Stream<Item = HotplugEvent> + Unpin)
) -> Result<S::Handle> {
    let id = timeout(REBOOT_TIMEOUT, async {
        let mut left = false;
        while let Some(event) = hotplug.next().await {
            match event {
                HotplugEvent::Left(id) if &id == orig_id => left = true,
                // Just assume new device after original one left is the device reconnected.
                // Probably should verify the serial or something...
                HotplugEvent::Arrived(id) if left => return Ok(id),
                _ => {
                    // Ignore events for devices other than the one we're interested in
                }
            }
        }
        Err(anyhow!("hotplug stream ended"))
    }).await
        .unwrap_or_else(|_| Err(anyhow!("timed out after {REBOOT_TIMEOUT:?}")))?;

    source.open(&id).await
}

async fn run_update<H: DeviceHandle>(dev: &mut H, image: &Image) -> Result<()> {
    handshake(dev).await?;
    begin(dev, image).await?;
    send_image(dev, image).await?;
    commit(dev).await?;
    dev.send(&Command::Reboot(RebootMagic::new())).await?;
    Ok(())
}

async fn handshake<H: DeviceHandle>(dev: &mut H) -> Result<()> {
    dev.send(&Command::Sync).await?;
    let device_info = dev.info().await?;
    if device_info.protocol_version != PROTOCOL_VERSION {
        bail!(
            "device speaks protocol version {}, expected {PROTOCOL_VERSION}",
            device_info.protocol_version
        );
    }
    info!(
        serial=%device_info.serial,
        "Device is running firmware {} from slot {:?}",
        device_info.firmware_version,
        device_info.running_slot
    );
    if device_info.last_update_result != UpdateResult::None {
        warn!(
            "Previous update failed: {:?} (version {:?})",
            device_info.last_update_result,
            device_info.failed_version,
        )
    }

    Ok(())
}

async fn begin<H: DeviceHandle>(dev: &mut H, image: &Image) -> Result<()> {
    let command = Command::UpdateBegin {
        size: image.size(),
        sha256: image.sha256,
        version: image.version.as_str().try_into().map_err(|_| anyhow!("version too long"))?,
    };

    let mut retries = 0;
    let mut aborted = false;
    loop {
        dev.send(&command).await?;
        match next_update_event(dev, EVENT_TIMEOUT).await? {
            // Expected response, update is successfully started
            Some(UpdateEvent::Ack { offset: 0 }) => return Ok(()),
            // Device rejected update due to still being in a trial boot.
            // Send a sync to cause the device to confirm it booted correctly, then rety
            Some(UpdateEvent::Nak {
                 error: UpdateError::NotConfirmed,
                 ..
            }) if retries < MAX_CONFIRM_RETRIES => {
                retries += 1;
                info!("Running firmware is not confirmed yet. Sending `Sync` and retrying");
                dev.send(&Command::Sync).await?;
                sleep(CONFIRM_RETRY_DELAY).await;
            }
            Some(UpdateEvent::Nak {
                error: UpdateError::AlreadyStarted,
                ..
            }) if !aborted => {
                aborted = true;
                warn!("Device has an unfinished update. Aborting");
                abort(dev).await?;
            }
            Some(UpdateEvent::Nak { error, .. }) => bail!("device rejected the udpate: {error:?}"),
            Some(event) => bail!("Unexpected reply to UpdateBegin: {event:?}"),
            None => bail!("no reply to UpdateBegin. {DAEMON_RUNNING_HINT}")
        }
    }
}

/// Aborts an update and waits for the abort confirmation. Returns Err if unexpected response is given.
async fn abort<H: DeviceHandle>(dev: &mut H) -> Result<()> {
    dev.send(&Command::UpdateAbort).await?;
    match next_update_event(dev, EVENT_TIMEOUT).await? {
        Some(UpdateEvent::Ack { offset: 0 }) => Ok(()),
        Some(event) => bail!("Unexpected reply to UpdateAbort: {event:?}"),
        None => bail!("no reply to UpdateAbort. {DAEMON_RUNNING_HINT}")
    }
}

/// Sends an image to the device and waits for the confirmation.
/// The device only sends acks after a sector has been flush (4 KiB for all but last sector),
/// and silently drops commands when it's queue is full, so make sure we only send one sector's
/// worth of chunks at a time, then wait for the ack before starting the next sector.
async fn send_image<H: DeviceHandle>(dev: &mut H, image: &Image) -> Result<()> {
    let size = image.size();
    let mut offset = 0;
    let mut rewinds = 0;
    let mut next_progress = PROGRESS_INTERVAL;

    loop {
        let boundary = size.min((offset / SECTOR_LEN + 1) * SECTOR_LEN);
        let mut last_chunk = offset;
        while offset < boundary {
            last_chunk = offset;
            offset = send_chunk(dev, image, offset).await?;
        }

        let event = match next_update_event(dev, EVENT_TIMEOUT).await? {
            Some(event) => event,
            None => {
                debug!("No reply after chunk at {last_chunk}. Re-sending it");
                send_chunk(dev, image, last_chunk).await?;
                next_update_event(dev, EVENT_TIMEOUT)
                    .await?
                    .with_context(|| format!("device stopped replying at offset {offset}. {DAEMON_RUNNING_HINT}"))?
            }
        };

        match event {
            UpdateEvent::Ack { offset: acked} => {
                if acked != offset {
                    warn!("Device acked {acked} but host was at {offset}");
                }
                offset = acked;
                if offset >= next_progress || offset == size {
                    info!(
                        "Sent {offset}/{size} bytes ({}%)",
                        offset as u64 * 100 / size as u64
                    );
                    next_progress = (offset / PROGRESS_INTERVAL + 1) * PROGRESS_INTERVAL;
                }
                if offset == size {
                    return Ok(());
                }
            }
            UpdateEvent::Nak {
                expected_offset,
                error: UpdateError::BadOffset,
            } => {
                let expected = drain_naks(dev, expected_offset).await?;
                if expected != offset {
                    rewinds += 1;
                    if rewinds > MAX_REWINDS {
                        // Probably need to check for naks mid-sector if we ever see this.
                        bail!("too many rewinds. The device keeps dropping chunks.");
                    }
                }
                debug!("Device expected offset {expected}. Rewinding from {offset}");
                offset = expected;
            }
            UpdateEvent::Nak { error, .. } => bail!("device rejected chunk: {error:?}"),
            UpdateEvent::Committed => bail!("unexpected Committed event while sending the image")
        }
    }
}

async fn send_chunk<H: DeviceHandle>(dev: &mut H, image: &Image, offset: u32) -> Result<u32> {
    let start = offset as usize;
    let end = image.data.len().min(start + UPDATE_CHUNK_LEN);
    let data = image.data[start..end]
        .try_into()
        .expect("chunk must be <= UPDATE_CHUNK_LEN. This is a bug in the daemon");
    dev.send(&Command::UpdateChunk { offset, data }).await?;
    Ok(end as u32)
}

/// Every chunk sent after a dropped is Nak'd so the last Nak holds the offset to resume from.
async fn drain_naks<H: DeviceHandle>(dev: &mut H, mut expected_offset: u32) -> Result<u32> {
    while let Some(event) = next_update_event(dev, DRAIN_TIMEOUT).await? {
        match event {
            UpdateEvent::Nak {
                expected_offset: expected,
                ..
            } => expected_offset = expected,
            event => debug!("Discarding stale event: {event:?}"),
        }
    }
    Ok(expected_offset)
}

async fn commit<H: DeviceHandle>(dev: &mut H) -> Result<()> {
    let mut retried = false;
    loop {
        dev.send(&Command::UpdateCommit).await?;
        match next_update_event(dev, COMMIT_TIMEOUT).await? {
            Some(UpdateEvent::Committed) => {
                info!("Image verified and activated");
                return Ok(());
            }
            Some(UpdateEvent::Nak { error, .. }) => bail!("device rejected the image: {error:?}"),
            Some(event) => bail!("unexpected reply to UpdateCommit: {event:?}"),
            None if !retried => {
                warn!("No reply to UpdateCommit after {COMMIT_TIMEOUT:?}. Retrying");
            }
            None => bail!("no reply to UpdateCommit after resending it"),
        }
        retried = true;
    }
}


/// Verify the device is running the expected firmware version
async fn verify<H: DeviceHandle>(dev: &mut H, image: &Image) -> Result<()> {
    dev.send(&Command::Sync).await?;
    sleep(SETTLE_DELAY).await;
    let device_info = dev.info().await?;
    info!(
        "Device is running firmware {} from slot {:?}",
        device_info.firmware_version,
        device_info.running_slot
    );

    match device_info.last_update_result {
        UpdateResult::None if device_info.firmware_version.as_str() == image.version => {
            info!("Update complete");
            Ok(())
        }
        UpdateResult::None => bail!(
            "device is running firmware {} instead of {}",
            device_info.firmware_version,
            image.version,
        ),
        result => {
            error!(
                "Update failed: {result:?} (version {:?})",
                device_info.failed_version,
            );
            clear_update_result(dev).await;
            bail!("device rolled back to {}", device_info.firmware_version)
        }
    }
}

async fn clear_update_result<H: DeviceHandle>(dev: &mut H) {
    if let Err(e) = dev.send(&Command::ClearUpdateResult).await {
        warn!("Failed to clear the update result: {e}");
        return;
    }
    match next_update_event(dev, EVENT_TIMEOUT).await {
        Ok(Some(UpdateEvent::Ack { .. })) => {} // Expected, just consume the event
        Ok(event) => warn!("Unexpected reply to ClearUpdateResult: {event:?}"),
        Err(e) => warn!("Failed to clear the udpate result: {e}"),
    }
}

/// Waits for the next update event. Returns `Ok(None)` if the timeout is reached.
///
/// State reports are skipped
async fn next_update_event<H: DeviceHandle>(
    dev: &mut H,
    wait: Duration,
) -> Result<Option<UpdateEvent>> {
    timeout(wait, async {
        loop {
            match dev.event().await? {
                Event::Update(event) => return Ok(event),
                Event::State(_) => {}
            }
        }
    }).await.ok().transpose()
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use protocol::{DeviceInfo, Serial, Slot};
    use protocol::event::UpdateError;
    use super::*;

    struct MockDevice {
        commands: Vec<Command>,
        script: VecDeque<Option<Event>>,
        waits_after: Vec<usize>,
        info: DeviceInfo,
    }

    impl MockDevice {
        fn new(script: impl IntoIterator<Item = Option<UpdateEvent>>) -> Self {
            Self {
                commands: vec![],
                script: script.into_iter().map(|e| e.map(Event::Update)).collect(),
                waits_after: vec![],
                info: DeviceInfo {
                    protocol_version: PROTOCOL_VERSION,
                    firmware_version: "1.0.0".try_into().unwrap(),
                    buttons: 8,
                    sliders: 4,
                    serial: Serial::try_from(&[1, 2, 3, 4, 5, 6][..]).unwrap(),
                    running_slot: Some(Slot::Ota0),
                    last_update_result: UpdateResult::None,
                    failed_version: None,
                },
            }
        }

        fn summary(&self) -> Vec<String> {
            self.commands
                .iter()
                .map(|command| match command {
                    Command::Sync => "Sync".to_owned(),
                    Command::UpdateBegin { size, .. } => format!("Begin {size}"),
                    Command::UpdateChunk { offset, data } => {
                        format!("Chunk {offset}+{}", data.len())
                    }
                    Command::UpdateCommit => "Commit".to_owned(),
                    Command::Reboot(_) => "Reboot".to_owned(),
                    other => format!("{other:?}")
                })
                .collect()
        }
    }

    impl  DeviceHandle for MockDevice {
        async fn info(&mut self) -> Result<DeviceInfo> {
            Ok(self.info.clone())
        }

        async fn send(&mut self, command: &Command) -> Result<()> {
            self.commands.push(command.clone());
            Ok(())
        }

        async fn event(&mut self) -> Result<Event> {
            self.waits_after.push(self.commands.len());
            match self.script.pop_front().flatten() {
                Some(event) => Ok(event),
                None => std::future::pending().await,
            }
        }
    }

    fn mock_image(len: usize, version: &str) -> Vec<u8> {
        let mut data = vec![0u8; len];
        data[0] = IMAGE_MAGIC;
        data[0x20..0x24].copy_from_slice(&APP_DESC_MAGIC.to_le_bytes());
        data[0x30..0x30 + version.len()].copy_from_slice(version.as_bytes());
        data
    }

    fn ack(offset: u32) -> Option<UpdateEvent> {
        Some(UpdateEvent::Ack { offset })
    }

    #[test]
    fn parses_image() {
        let image = Image::parse(mock_image(0x80, "1.2.3")).unwrap();
        assert_eq!(image.version, "1.2.3");
        assert_eq!(image.size(), 0x80);
    }

    #[test]
    fn rejects_missing_app_desc_magic() {
        let mut data = mock_image(0x80, "1.2.3");
        data[0x20] = 0;
        assert!(Image::parse(data).is_err());
    }

    #[test]
    fn rejects_wrong_image_magic() {
        let mut data = mock_image(0x80, "1.2.3");
        data[0] = 0;
        assert!(Image::parse(data).is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn sends_small_image() {
        let image = Image::parse(mock_image(100, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([ack(0), ack(100), Some(UpdateEvent::Committed)]);

        run_update(&mut dev, &image).await.unwrap();

        assert_eq!(
            dev.summary(),
            [
                "Sync",
                "Begin 100",
                "Chunk 0+56",
                "Chunk 56+44",
                "Commit",
                "Reboot"
            ]
        )
    }

    #[tokio::test(start_paused = true)]
    async fn waits_for_ack_after_each_sector() {
        let image = Image::parse(mock_image(4200, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([ack(0), ack(4144), ack(4200), Some(UpdateEvent::Committed)]);

        run_update(&mut dev, &image).await.unwrap();

        assert_eq!(dev.waits_after, [2, 76, 77, 78]);
        let summary = dev.summary();
        assert_eq!(summary.len(), 79);
        assert_eq!(summary[75], "Chunk 4088+56");
        assert_eq!(summary[76], "Chunk 4144+56");
        assert_eq!(summary[77], "Commit");
    }

    #[tokio::test(start_paused = true)]
    async fn rewinds_on_bad_offset() {
        let image = Image::parse(mock_image(100, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([
            ack(0),
            Some(UpdateEvent::Nak {
                expected_offset: 56,
                error: UpdateError::BadOffset,
            }),
            None,
            ack(100),
            Some(UpdateEvent::Committed),
        ]);

        run_update(&mut dev, &image).await.unwrap();

        assert_eq!(
            dev.summary(),
            [
                "Sync",
                "Begin 100",
                "Chunk 0+56",
                "Chunk 56+44",
                "Chunk 56+44",
                "Commit",
                "Reboot"
            ]
        )
    }

    #[tokio::test(start_paused = true)]
    async fn resends_last_chunk_on_timeout() {
        let image = Image::parse(mock_image(100, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([ack(0), None, ack(100), Some(UpdateEvent::Committed)]);

        run_update(&mut dev, &image).await.unwrap();

        assert_eq!(
            dev.summary(),
            [
                "Sync",
                "Begin 100",
                "Chunk 0+56",
                "Chunk 56+44",
                "Chunk 56+44",
                "Commit",
                "Reboot"
            ]
        )
    }

    #[tokio::test(start_paused = true)]
    async fn fails_after_second_timeout() {
        let image = Image::parse(mock_image(100, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([ack(0), None, None]);

        let err = run_update(&mut dev, &image).await.unwrap_err();

        assert!(
            err.to_string()
                .starts_with("device stopped replying at offset 100"),
            "{err}"
        )
    }

    #[tokio::test(start_paused = true)]
    async fn retries_when_not_confirmed() {
        let image = Image::parse(mock_image(100, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([
            Some(UpdateEvent::Nak {
                expected_offset: 0,
                error: UpdateError::NotConfirmed
            }),
            ack(0),
            ack(100),
            Some(UpdateEvent::Committed),
        ]);

        run_update(&mut dev, &image).await.unwrap();

        assert_eq!(
            dev.summary(),
            [
                "Sync",
                "Begin 100",
                "Sync", // Restart process
                "Begin 100",
                "Chunk 0+56",
                "Chunk 56+44",
                "Commit",
                "Reboot"
            ]
        )
    }

    #[tokio::test(start_paused = true)]
    async fn resends_commit_on_timeout() {
        let image = Image::parse(mock_image(100, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([
            ack(0),
            ack(100),
            None,
            Some(UpdateEvent::Committed),
        ]);

        run_update(&mut dev, &image).await.unwrap();

        assert_eq!(
            dev.summary(),
            [
                "Sync",
                "Begin 100",
                "Chunk 0+56",
                "Chunk 56+44",
                "Commit",
                "Commit",
                "Reboot"
            ]
        )
    }

    #[tokio::test(start_paused = true)]
    async fn aborts_and_retries_when_already_started() {
        let image = Image::parse(mock_image(100, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([
            Some(UpdateEvent::Nak {
                expected_offset: 0,
                error: UpdateError::AlreadyStarted,
            }),
            ack(0),
            ack(0),
            ack(100),
            Some(UpdateEvent::Committed),
        ]);

        run_update(&mut dev, &image).await.unwrap();

        assert_eq!(
            dev.summary(),
            [
                "Sync",
                "Begin 100",
                "UpdateAbort",
                "Begin 100",
                "Chunk 0+56",
                "Chunk 56+44",
                "Commit",
                "Reboot"
            ]
        )
    }

    #[tokio::test(start_paused = true)]
    async fn verify_succeeds_on_matching_version() {
        let image = Image::parse(mock_image(0x80, "1.0.0")).unwrap();
        let mut dev = MockDevice::new([]);

        verify(&mut dev, &image).await.unwrap();

        assert_eq!(dev.summary(), ["Sync"]);
    }

    #[tokio::test(start_paused = true)]
    async fn verify_fails_on_version_mismatch() {
        let image = Image::parse(mock_image(0x80, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([]);

        let err = verify(&mut dev, &image).await.unwrap_err();

        assert_eq!(
            err.to_string(),
            "device is running firmware 1.0.0 instead of 1.2.3"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn verify_clears_failed_update_result() {
        let image = Image::parse(mock_image(0x80, "1.2.3")).unwrap();
        let mut dev = MockDevice::new([ack(0)]);
        dev.info.last_update_result = UpdateResult::Aborted;
        dev.info.failed_version = Some("1.2.3".try_into().unwrap());

        let err = verify(&mut dev, &image).await.unwrap_err();

        assert_eq!(err.to_string(), "device rolled back to 1.0.0");
        assert_eq!(dev.summary(), ["Sync", "ClearUpdateResult"]);
    }
}
