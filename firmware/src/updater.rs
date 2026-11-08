use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::TrySendError;
use embassy_sync::signal::Signal;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
use esp_bootloader_esp_idf::partitions;
use esp_bootloader_esp_idf::partitions::{AppPartitionSubType, FlashStorage, PartitionEntry, PARTITION_TABLE_MAX_LEN, Error, PartitionType};
use esp_hal::peripherals::{FLASH,};
use log::{error, info, warn};
use sha2::{Digest, Sha256};
use static_cell::ConstStaticCell;
use protocol::command::Command;
use protocol::event::{Event, UpdateError, UpdateEvent};
use protocol::UpdateResult;
use crate::boot;
use crate::boot::BootKind;
use crate::usb::EVENTS;

const SECTOR_LEN: usize = 4096;
const IMAGE_MAGIC: u8 = 0xE9;

pub enum Request {
    Command(Command),
    Confirm,
}

pub static CONFIRM: Signal<CriticalSectionRawMutex, ()> = Signal::new();
pub static CONFIRMED: Signal<CriticalSectionRawMutex, Result<(), Error>> = Signal::new();

#[derive(Copy, Clone)]
struct Transfer {
    size: u32,
    sha256: [u8; 32],
    target: AppPartitionSubType,
    partition: PartitionEntry,
    written: u32,
    buffered: usize,
}

impl Transfer {
    fn received(&self) -> u32 {
        self.written + self.buffered as u32
    }
}

#[derive(Copy, Clone)]
enum UpdaterState {
    Idle,
    Receiving(Transfer),
    Committed(AppPartitionSubType),
}

pub struct Updater {
    flash: FlashStorage<'static>,
    table: &'static mut [u8; PARTITION_TABLE_MAX_LEN],
    sector: &'static mut [u8; SECTOR_LEN],
    state: UpdaterState,
}

impl Updater {
    pub fn new(flash: FLASH<'static>) -> Self {
        static TABLE: ConstStaticCell<[u8; PARTITION_TABLE_MAX_LEN]> = ConstStaticCell::new([0; _]);
        static SECTOR: ConstStaticCell<[u8; SECTOR_LEN]> = ConstStaticCell::new([0; _]);

        Self {
            flash: FlashStorage::new(flash),
            table: TABLE.take(),
            sector: SECTOR.take(),
            state: UpdaterState::Idle,
        }
    }

    pub fn confirm(&mut self) {
        let result = boot::confirm(&mut self.flash);
        CONFIRMED.signal(result);
    }

    pub async fn handle(&mut self, command: Command) {
        let result = match command {
            Command::Sync | Command::EnterBootloader(_) | Command::Reboot(_) => {
                warn!("Unexpected command sent to OTA Updater");
                return;
            }
            Command::UpdateBegin { size, sha256, version } => self.begin(size, sha256, &version),
            Command::UpdateChunk { offset, data } => self.chunk(offset, &data).await,
            Command::UpdateCommit => self.commit().await,
            Command::UpdateAbort => {
                if matches!(self.state, UpdaterState::Receiving(_)) {
                    info!("updater: update aborted");
                }
                let reverted = match self.state {
                    UpdaterState::Committed(_) => self.reconcile().map_err(slot_error),
                    _ => Ok(())
                };
                reverted.map(|()| {
                    self.state = UpdaterState::Idle;
                    Some(UpdateEvent::Ack { offset: 0 })
                })
            }
            Command::ClearUpdateResult => self.clear_update_result(),
        };

        let event = match result {
            Ok(Some(event)) => event,
            Ok(None) => return,
            Err(error) => {
                warn!("updater: command rejected: {:?}", error);
                let expected_offset = match &self.state {
                    UpdaterState::Receiving(transfer) => transfer.received(),
                    _ => 0
                };
                UpdateEvent::Nak { expected_offset, error }
            }
        };

        if let Err(TrySendError::Full(event)) = EVENTS.try_send(Event::Update(event)) {
            warn!("updater: event queue full, dropping {:?}", event);
        }
    }

    fn begin(&mut self, size: u32, sha256: [u8; 32], version: &str) -> Result<Option<UpdateEvent>, UpdateError> {
        if matches!(self.state, UpdaterState::Receiving(_)) {
            return Err(UpdateError::AlreadyStarted);
        }
        self.state = UpdaterState::Idle;
        // Activating a new slot while the running one is still unverified can leave otadata with no valid
        // entry after a failed boot, thus reject firmware updates if we haven't verified this firmware yet.
        if boot::boot_kind() == BootKind::Trial {
            return Err(UpdateError::NotConfirmed);
        }

        self.reconcile().map_err(slot_error)?;

        let mut ota = OtaUpdater::new(&mut self.flash, &mut *self.table).map_err(slot_error)?;
        let (capacity, target) = {
            let (region, target) = ota.next_partition().map_err(slot_error)?;
            (region.partition_size(), target)
        };
        if size == 0 || size as usize > capacity {
            return Err(UpdateError::TooLarge);
        }

        let partition = partitions::read_partition_table(&mut self.flash, &mut *self.table)
            .map_err(flash_error)?
            .find_partition(PartitionType::App(target))
            .map_err(flash_error)?
            .ok_or(UpdateError::NoSlot)?;

        info!("updater: receiving v{} ({} bytes) into {:?}", version, size, target);
        self.state = UpdaterState::Receiving(Transfer {
            size,
            sha256,
            target,
            partition,
            written: 0,
            buffered: 0,
        });
        Ok(Some(UpdateEvent::Ack { offset: 0 }))
    }

    async fn chunk(&mut self, offset: u32, data: &[u8]) -> Result<Option<UpdateEvent>, UpdateError> {
        let UpdaterState::Receiving(mut transfer) = self.state else {
            return Err(UpdateError::NotStarted);
        };
        if offset != transfer.received() {
            return Err(UpdateError::BadOffset);
        }
        if data.len() as u32 > transfer.size - transfer.received() {
            return Err(UpdateError::SizeMismatch);
        }

        let mut data = data;
        let mut flushed = false;
        while !data.is_empty() {
            let n = data.len().min(SECTOR_LEN - transfer.buffered);
            self.sector[transfer.buffered..transfer.buffered + n].copy_from_slice(&data[..n]);
            transfer.buffered += n;
            data = &data[n..];

            if transfer.buffered == SECTOR_LEN || transfer.received() == transfer.size {
                if let Err(e) = self.flush(&mut transfer).await {
                    self.state = UpdaterState::Idle;
                    return Err(flash_error(e));
                }
                flushed = true;
            }
        }

        self.state = UpdaterState::Receiving(transfer);
        Ok(flushed.then(|| UpdateEvent::Ack { offset: transfer.received() }))
    }

    async fn flush(&mut self, transfer: &mut Transfer) -> Result<(), Error> {
        self.sector[transfer.buffered..].fill(0xFF);
        transfer.partition
            .as_flash_region(&mut self.flash)
            .write(transfer.written, &self.sector[..])?;
        transfer.written += transfer.buffered as u32;
        transfer.buffered = 0;

        // Flushing is a blocking operation which may have taken a while.
        // Yield back to the runtime to ensure other tasks get a chance to run.
        embassy_futures::yield_now().await;
        Ok(())
    }

    async fn commit(&mut self) -> Result<Option<UpdateEvent>, UpdateError> {
        let transfer = match self.state {
            UpdaterState::Receiving(transfer) => transfer,
            UpdaterState::Committed(slot) => {
                info!("updater: commit repeated for {:?}", slot);
                return Ok(Some(UpdateEvent::Committed));
            }
            UpdaterState::Idle => return Err(UpdateError::NotStarted),
        };
        if transfer.received() != transfer.size {
            return Err(UpdateError::SizeMismatch);
        }

        let result = match self.verify(&transfer).await {
            Ok(()) => self.activate(transfer.target),
            Err(e) => Err(e)
        };
        if let Err(e) = result {
            self.state = UpdaterState::Idle;
            return Err(e);
        }
        self.state = UpdaterState::Committed(transfer.target);
        info!("updater: update committed to {:?}", transfer.target);
        Ok(Some(UpdateEvent::Committed))
    }

    async fn verify(&mut self, transfer: &Transfer) -> Result<(), UpdateError> {
        let mut hasher = Sha256::new();
        let mut offset = 0;
        while offset < transfer.size {
            let n = (transfer.size - offset).min(SECTOR_LEN as u32) as usize;
            // re-use the sector buffer since it's not needed anymore
            let chunk = &mut self.sector[..n];
            transfer.partition
                .as_flash_region(&mut self.flash)
                .read(offset, chunk)
                .map_err(flash_error)?;
            if offset == 0 && chunk[0] != IMAGE_MAGIC {
                return Err(UpdateError::BadImage);
            }
            hasher.update(&*chunk);
            offset += n as u32;
            yield_now().await;
        }

        let digest: [u8; 32] = hasher.finalize().into();
        if digest != transfer.sha256 {
            return Err(UpdateError::HashMismatch);
        }

        Ok(())
    }

    fn activate(&mut self, target: AppPartitionSubType) -> Result<(), UpdateError> {
        let mut ota = OtaUpdater::new(&mut self.flash, &mut *self.table).map_err(slot_error)?;
        let (_, next) = ota.next_partition().map_err(slot_error)?;
        if next != target {
            error!("updater: next partition changed from {:?} to {:?}", target, next);
            return Err(UpdateError::NoSlot);
        }
        // mark the other slot as the active slot and set it's state to New (eg not validated)
        // The device must be reset still in order to actually run the new firmware.
        ota.activate_next_partition().map_err(slot_error)?;
        ota.set_current_ota_state(OtaImageState::New).map_err(flash_error)?;
        Ok(())
    }

    fn clear_update_result(&mut self) -> Result<Option<UpdateEvent>, UpdateError> {
        if matches!(self.state, UpdaterState::Idle) {
            self.reconcile().map_err(slot_error)?;
        }
        crate::info::update(|info| {
            info.last_update_result = UpdateResult::None;
            info.failed_version = None;
        });
        info!("updater: update result cleared");
        Ok(Some(UpdateEvent::Ack { offset: 0 }))
    }

    /// After a rollback, otadata still selects the dead slot with an Aborted/Invalid flag, so every boot
    /// report the rollback again. Selecting the running slot (and marking it valid) clears that.
    fn reconcile(&mut self) -> Result<(), Error> {
        let Some(booted) = self.booted_app()? else {
            return Ok(());
        };
        if booted == AppPartitionSubType::Factory {
            return Ok(());
        }
        let mut ota = OtaUpdater::new(&mut self.flash, &mut *self.table)?;
        if ota.selected_partition()? != booted {
            info!("updater: selecting booted partition {:?}", booted);
            ota.ota_data()?.set_current_app_partition(booted)?;
            ota.set_current_ota_state(OtaImageState::Valid)?;
        }
        Ok(())
    }

    fn booted_app(&mut self) -> Result<Option<AppPartitionSubType>, Error> {
        let table = partitions::read_partition_table(&mut self.flash, &mut self.table[..])?;
        let booted_type = table.booted_partition()?.map(|entry| entry.partition_type());
        Ok(match booted_type {
            Some(PartitionType::App(app)) => Some(app),
            _ => None,
        })
    }
}

fn flash_error(e: Error) -> UpdateError {
    error!("updater: flash error {:?}", e);
    UpdateError::Flash
}

fn slot_error(e: Error) -> UpdateError {
    match e {
        Error::Invalid | Error::InvalidState => {
            warn!("updater: no usable update slot: {:?}", e);
            UpdateError::NoSlot
        }
        _ => flash_error(e)
    }
}
