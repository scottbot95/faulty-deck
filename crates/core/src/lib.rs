use serde::{Deserialize, Serialize};
use protocol::MAX_SLIDERS;

pub mod event;
pub mod tracker;
pub mod action;
pub mod config;
pub mod key;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct ButtonId(pub u8);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SliderId(pub u8);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PageId(String);

pub struct DeviceState {
    /// Bitmask of buttons currently pressed
    pub buttons: u16,
    /// Array of slider values. Not all sliders are necessarily used.
    pub sliders: [u16; MAX_SLIDERS],
}
