use serde::{Deserialize, Serialize};
use crate::DeviceState;

#[cfg(doc)]
use crate::command::Command;

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Event {
    State(DeviceState),
    Update(UpdateEvent),
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum UpdateEvent {
    /// Bytes [0, offset) are accepted. Send the next chunk as `offset`.
    Ack { offset: u32 },
    /// Chunk sent at unexpected offset.
    Nak { expected_offset: u32, error: UpdateError },
    /// Image verified and saved. The host should send [`Command::Reboot`] to activate the new firmware.
    Committed,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum UpdateError {
    NotStarted,
    AlreadyStarted,
    TooLarge,
    BadOffset,
    SizeMismatch,
    BadImage,
    HashMismatch,
    Flash,
    NoSlot,
    /// The running image has not been confirmed yet. Send [`Command::Sync`] and retry
    NotConfirmed,
}
