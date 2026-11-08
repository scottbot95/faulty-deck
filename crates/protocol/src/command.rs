use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;
use crate::{UPDATE_CHUNK_LEN, VERSION_STRING_MAX_LEN};

/// Guard value for [`Command::EnterBootloader`] to ensure bad packets don't accidentally
/// reboot the device
const BOOTLOADER_MAGIC: [u8; 4] = *b"DFU!";
/// Guard value for [`Command::Reboot`] to ensure bad packets don't accidentally
/// reboot the device
const REBOOT_MAGIC: [u8; 4] = *b"RST!";

/// Command sent from the daemon to the device as SET_REPORT OUT feature
///
/// **IMPORTANT**: Since the command index is part of the wire protocol, reordering or removing
/// variants is a breaking change and must be done with care (renaming is fine).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Force a resync, causing the device to send a full state report
    Sync,

    /// Reboot the device into the bootloader
    EnterBootloader(BootloaderMagic),

    /// Start firmware update
    UpdateBeing {
        size: u32,
        sha256: [u8; 32],
        version: heapless::String<VERSION_STRING_MAX_LEN>,
    },
    /// Update a chunk of the firmware
    UpdateChunk {
        offset: u32,
        data: heapless::Vec<u8, UPDATE_CHUNK_LEN>,
    },
    /// Finalize firmware update.
    UpdateCommit,
    /// Abort firmware update, discarding all uploaded chunks.
    UpdateAbort,
    /// Resets [`DeviceInfo::last_update_result`] and [`DeviceInfo::failed_version`]
    ClearUpdateResult,
    /// Reboot the device
    Reboot(RebootMagic),
}

macro_rules! magic_number_impl {
    ($ty_name:ident, $value:expr, $name:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct $ty_name(());

        impl $ty_name {
            pub const fn new() -> Self {
                Self(())
            }
        }

        impl Default for $ty_name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl Serialize for $ty_name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                $value.serialize(serializer)
            }
        }

        impl<'de> Deserialize<'de> for $ty_name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>
            {
                let bytes = <[u8; 4]>::deserialize(deserializer)?;
                if bytes == $value {
                    Ok(Self(()))
                } else {
                    Err(D::Error::custom("invalid bootloader magic"))
                }
            }
        }
    }
}

magic_number_impl!(BootloaderMagic, BOOTLOADER_MAGIC, "bootloader");
magic_number_impl!(RebootMagic, REBOOT_MAGIC, "reboot");
