use core::sync::atomic::{AtomicU8, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use esp_bootloader_esp_idf::ota::OtaImageState;
use esp_bootloader_esp_idf::ota_updater::OtaUpdater;
use esp_bootloader_esp_idf::partitions;
use esp_bootloader_esp_idf::partitions::{Error, PartitionTable, PARTITION_TABLE_MAX_LEN, PartitionEntry, AppPartitionSubType};
use esp_hal::peripherals::FLASH;
use esp_storage::FlashStorage;
use log::{info, warn};

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

static BOOT_KIND: AtomicU8 = AtomicU8::new(BootKind::Unknown as u8);

pub static HOST_SYNCED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Get the loaded boot kind
pub fn boot_kind() -> BootKind {
    BootKind::from_u8(BOOT_KIND.load(Ordering::Relaxed))
}

/// Check the boot kind from flash
pub fn check(flash: FLASH<'_>) -> BootKind {
    let mut flash = FlashStorage::new(flash);
    let mut buffer: [u8; PARTITION_TABLE_MAX_LEN] = [0; _];

    match partitions::read_partition_table(&mut flash, &mut buffer) {
        Ok(table) => match table.booted_partition() {
            Ok(Some(booted)) => info!("boot: booted partition {}", booted.label_as_str()),
            Ok(None) => warn!("boot: booted partition not found in partition table"),
            Err(e) => warn!("boot: failed to determine booted partition {:?}", e),
        }
        Err(e) => warn!("boot: failed to read partition table: {:?}", e),
    }

    let kind = match OtaUpdater::new(&mut flash, &mut buffer) {
        Ok(mut ota) => {
            // Log the selected partition
            match ota.selected_partition() {
                Ok(selected) => info!("boot: selected partition {:?}", selected),
                Err(e) => warn!("boot: failed to read selected partition: {:?}", e),
            }
            match ota.current_ota_state() {
                Ok(state) => {
                    info!("boot: ota state {:?}", state);
                    match state {
                        // Technically New should never happen with rollback enabled, but include it just in case
                        OtaImageState::New
                        | OtaImageState::PendingVerify => BootKind::Trial,
                        OtaImageState::Aborted
                        | OtaImageState::Invalid => BootKind::RolledBack,
                        _ => BootKind::Normal
                    }
                }
                Err(e) => {
                    info!("boot: otadata is empty {:?}", e);
                    BootKind::Normal
                }
            }
        }
        Err(e) => {
            info!("boot: no usable otadata partition: {:?}", e);
            BootKind::Normal
        }
    };

    info!("boot: boot kind {:?}", kind);
    BOOT_KIND.store(kind as u8, Ordering::Relaxed);
    kind
}

pub fn confirm(flash: FLASH<'_>) -> Result<(), Error> {
    let mut flash = FlashStorage::new(flash);
    let mut buffer: [u8; PARTITION_TABLE_MAX_LEN] = [0; _];

    OtaUpdater::new(&mut flash, &mut buffer)?.set_current_ota_state(OtaImageState::Valid)?;
    BOOT_KIND.store(BootKind::Normal as u8, Ordering::Relaxed);
    Ok(())
}
