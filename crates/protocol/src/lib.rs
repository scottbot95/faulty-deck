#![no_std]

#[cfg(feature = "std")]
extern crate std;

use core::array::TryFromSliceError;
use core::fmt::{Display, Formatter};
use serde::{Deserialize, Serialize};

pub fn reading_to_hue(raw_value: u16) -> u8 {
    ((raw_value as u32 * 240 / 360) >> 4) as u8
}

pub const PROTOCOL_VERSION: u16 = 1;
pub const REPORT_LEN: usize = 64;
pub const MAX_SLIDERS: usize = 4;

pub const MAX_SLIDER_VALUE: u16 = 4095;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Serial([u8; 6]);

impl TryFrom<&[u8]> for Serial {
    type Error = TryFromSliceError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let serial = value.try_into()?;
        Ok(Serial(serial))
    }
}

impl Display for Serial {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        for (i, b) in self.0.iter().enumerate() {
            if i != 0 {
                f.write_str(":")?;
            }
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// Input report: the complete current state of the controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceState {
    /// Bitmask of buttons currently pressed. Not all buttons are necessarily used.
    pub buttons: u16,
    /// Array of slider values. Not all sliders are necessarily used.
    pub sliders: [u16; MAX_SLIDERS],
}

/// Output report: a command to the device from the daemon
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Force a resync, causing the device to send a full state report
    Sync
}

impl DeviceState {
    pub const INITIAL: Self = Self { buttons: 0, sliders: [0; MAX_SLIDERS] };

    pub fn button(&self, id: u8) -> bool {
        self.buttons & (1 << id) != 0
    }

    pub fn set_button(&mut self, id: u8, pressed: bool) {
        let bit = 1 << id;
        if pressed {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Protocol version spoken by the device
    pub protocol_version: u16,

    /// Firmware version of the device
    ///
    /// (Major, Minor, Patch)
    pub firmware_version: (u8, u8, u8),

    /// Number of buttons on the device
    pub buttons: u8,

    /// Number of sliders on the device
    pub sliders: u8,

    /// Serial number of the device. Typically just the MAC address efuse
    pub serial: Serial,
}

pub type Report = [u8; REPORT_LEN];

/// Encodes a value into a [`Report`].
pub fn encode<T: Serialize>(value: &T) -> Result<Report, postcard::Error> {
    let mut report = [0u8; REPORT_LEN];
    postcard::to_slice(value, &mut report)?;
    Ok(report)
}

/// Decodes a [`Report`] into a value.
pub fn decode<'a, T: Deserialize<'a>>(report: &'a [u8]) -> Result<T, postcard::Error> {
    postcard::from_bytes(report)
}
