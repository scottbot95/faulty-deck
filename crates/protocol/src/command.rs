use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;

/// Guard value for `Command::EnterBootloader` to ensure bad packets don't accidentally
/// reboot the device
const BOOTLOADER_MAGIC: [u8; 4] = *b"DFU!";

/// Command sent from the daemon to the device as SET_REPORT OUT feature
///
/// **IMPORTANT**: Since the command index is part of the wire protocol, reordering or removing
/// variants is a breaking change and must be done with care (renaming is fine).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Force a resync, causing the device to send a full state report
    Sync,

    /// Reboot the device into the bootloader
    EnterBootloader(BootloaderMagic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootloaderMagic(());

impl BootloaderMagic {
    pub const fn new() -> Self {
        Self(())
    }
}

impl Default for BootloaderMagic {
    fn default() -> Self {
        Self::new()
    }
}

impl Serialize for BootloaderMagic {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BOOTLOADER_MAGIC.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BootloaderMagic {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error>
    {
        let bytes = <[u8; 4]>::deserialize(deserializer)?;
        if bytes == BOOTLOADER_MAGIC {
            Ok(Self(()))
        } else {
            Err(D::Error::custom("invalid bootloader magic"))
        }
    }
}
