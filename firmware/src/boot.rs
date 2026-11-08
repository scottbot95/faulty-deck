use core::sync::atomic::{AtomicU8, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
use esp_bootloader_esp_idf::partitions;
use esp_bootloader_esp_idf::partitions::{Error, PARTITION_TABLE_MAX_LEN, AppPartitionSubType, PartitionType};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use heapless::String;
use log::{info, warn};
use protocol::{Slot, UpdateResult, VERSION_STRING_MAX_LEN};

const APP_DESC_OFFSET: u32 = 0x20;
const APP_DESC_MAGIC: u32 = 0xABCD_5432;
const APP_DESC_VERSION: core::ops::Range<usize> = 16..48;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BootKind {
    /// Normal boot.
    ///
    /// Does NOT necessarily mean the ota process happened normally, but means we should not try to rollback.
    Normal = 0,
    /// First boot of a fresh firmware update
    Trial = 1,
    /// Boot after a rollback
    RolledBack = 2,
    /// Unknown boot state.
    ///
    /// Should only exist briefly before [`check`] is called to read the boot kind from flash.
    Unknown = 3,
}

impl BootKind {
    pub const fn from_u8(x: u8) -> BootKind {
        match x {
            x if x == BootKind::Normal as u8 => BootKind::Normal,
            x if x == BootKind::Trial as u8 => BootKind::Trial,
            x if x == BootKind::RolledBack as u8 => BootKind::RolledBack,
            _ => BootKind::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BootInfo {
    pub kind: BootKind,
    pub running_slot: Option<Slot>,
    pub last_update_result: UpdateResult,
    pub failed_version: Option<String<VERSION_STRING_MAX_LEN>>
}

static BOOT_KIND: AtomicU8 = AtomicU8::new(BootKind::Unknown as u8);

pub static HOST_SYNCED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Get the loaded boot kind
pub fn boot_kind() -> BootKind {
    BootKind::from_u8(BOOT_KIND.load(Ordering::Relaxed))
}

fn slot(app: AppPartitionSubType) -> Option<Slot> {
    match app {
        AppPartitionSubType::Factory => Some(Slot::Factory),
        AppPartitionSubType::Ota0 => Some(Slot::Ota0),
        AppPartitionSubType::Ota1 => Some(Slot::Ota1),
        _ => None,
    }
}

/// Check the boot kind from flash
pub fn check(flash: FLASH<'_>) -> BootInfo {
    let mut flash = FlashStorage::new(flash);
    let mut buffer: [u8; PARTITION_TABLE_MAX_LEN] = [0; _];

    let mut running_slot = None;
    match partitions::read_partition_table(&mut flash, &mut buffer) {
        Ok(table) => match table.booted_partition() {
            Ok(Some(booted)) => {
                info!("boot: booted partition {}", booted.label_as_str());
                if let PartitionType::App(app) = booted.partition_type() {
                    running_slot = slot(app);
                }
            },
            Ok(None) => warn!("boot: booted partition not found in partition table"),
            Err(e) => warn!("boot: failed to determine booted partition {:?}", e),
        }
        Err(e) => warn!("boot: failed to read partition table: {:?}", e),
    }

    let mut selected_partition = None;
    let (kind, last_update_result) = match OtaUpdater::new(&mut flash, &mut buffer) {
        Ok(mut ota) => {
            // Check which partition is selected
            match ota.selected_partition() {
                Ok(selected) => {
                    info!("boot: selected partition {:?}", selected);
                    selected_partition = Some(selected);
                },
                Err(e) => warn!("boot: failed to read selected partition: {:?}", e),
            }
            match ota.current_ota_state() {
                Ok(state) => {
                    info!("boot: ota state {:?}", state);
                    match state {
                        // Technically New should never happen with rollback enabled, but include it just in case
                        OtaImageState::New
                        | OtaImageState::PendingVerify => (BootKind::Trial, UpdateResult::None),
                        OtaImageState::Aborted => (BootKind::RolledBack, UpdateResult::Aborted),
                        OtaImageState::Invalid => (BootKind::RolledBack, UpdateResult::Invalid),
                        _ => (BootKind::Normal, UpdateResult::None),
                    }
                }
                Err(e) => {
                    info!("boot: otadata is empty {:?}", e);
                    (BootKind::Normal, UpdateResult::None)
                }
            }
        }
        Err(e) => {
            info!("boot: no usable otadata partition: {:?}", e);
            (BootKind::Normal, UpdateResult::None)
        }
    };

    let failed_version = match selected_partition {
        Some(selected) if kind == BootKind::RolledBack && slot(selected) != running_slot => {
            // Read the firmware version of the failed partition from flash
            read_app_version(&mut flash, &mut buffer, selected)
        }
        _ => None,
    };
    if let Some(version) = &failed_version {
        warn!("boot: update to v{} failed ({:?})", version, last_update_result);
    }

    info!("boot: boot kind {:?}", kind);
    BOOT_KIND.store(kind as u8, Ordering::Relaxed);
    BootInfo {
        kind,
        running_slot,
        last_update_result,
        failed_version,
    }
}

fn read_app_version(
    flash: &mut FlashStorage<'_>,
    buffer: &mut [u8],
    app: AppPartitionSubType,
) -> Option<String<VERSION_STRING_MAX_LEN>> {
    let table = partitions::read_partition_table(flash, buffer).ok()?;
    let entry = table.iter().find(|entry| entry.partition_type() == PartitionType::App(app))?;
    let mut desc = [0u8; 64];
    entry.as_flash_region(flash).read(APP_DESC_OFFSET, &mut desc).ok()?;

    let magic = u32::from_le_bytes([desc[0], desc[1], desc[2], desc[3]]);
    if magic != APP_DESC_MAGIC {
        return None;
    }

    let raw = &desc[APP_DESC_VERSION];
    // find sub-slice of raw ending in a null byte (or the full string if no null-byte)
    let raw = &raw[..raw.iter().position(|&b| b == 0).unwrap_or(raw.len())];
    let raw = match core::str::from_utf8(raw) {
        Ok(version) => version,
        Err(e) => core::str::from_utf8(&raw[..e.valid_up_to()]).ok()?,
    };

    let mut version = String::new();
    for c in raw.chars() {
        if version.push(c).is_err() {
            break;
        }
    }
    Some(version)
}

pub fn confirm(flash: &mut FlashStorage) -> Result<(), Error> {
    let mut buffer: [u8; PARTITION_TABLE_MAX_LEN] = [0; _];

    OtaUpdater::new(flash, &mut buffer)?.set_current_ota_state(OtaImageState::Valid)?;
    BOOT_KIND.store(BootKind::Normal as u8, Ordering::Relaxed);
    Ok(())
}
