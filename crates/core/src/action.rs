use std::fmt::{Display, Formatter};
use std::ops::Deref;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::key::KeyCode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// A single key chord. Will send press events for each key in order
    /// then release events for each key in reverse order.
    Key { chord: Vec<KeyCode> },
    /// Send a string of text to the system.
    Text { text: String },
    /// Run a program with the given arguments/working directory.
    Command {
        /// Program to execute
        program: PathBuf,
        /// Arguments to pass to the program.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Working directory to execute the program in. Inherits from the daemon directory if None.
        cwd: Option<PathBuf>,
        /// Wait for the program to exit before continuing processing **ANY** other actions.
        #[serde(default)]
        wait: bool,
    },
    Volume { target: AudioTarget, op: VolumeOp },

    /// Execute a series of actions in sequence
    Sequence {
        actions: Vec<Action>,
    },

    /// Simply sleeps for a given duration
    Delay {
        ms: u32,
        #[serde(default)]
        jitter: u32,
    }
}

/// Represents the target for audio actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioTarget {
    /// Audio sink (eg a speaker, headset)
    Sink(String),
    /// Audio source (eg a microphone)
    Source(String),
    // App(String),
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum VolumeOp {
    Set(Volume),
    ToggleMute,
}

/// Represents the volume level for audio actions.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Volume(f32);

impl Volume {
    pub fn from_slider(raw: u16, max: u16) -> Self {
        Self((raw as f32 / max as f32).clamp(0.0, 1.0))
    }
}

impl Display for Volume {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.0}%", self.0 * 100.0)
    }
}

impl Deref for Volume {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
