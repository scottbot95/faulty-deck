use std::fmt::{Display, Formatter};
use std::str::FromStr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, strum::EnumString, strum::Display, strum::EnumIter)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum NamedKey {
    #[strum(serialize = "ctrl", serialize = "control")]
    Ctrl,
    Shift,
    Alt,
    #[strum(serialize = "meta", serialize = "super")]
    Meta,
    Enter,
    #[strum(serialize = "esc", serialize = "escape")]
    Escape,
    Tab,
    Space,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    VolumeUp,
    VolumeDown,
    Mute,
    PlayPause,
    NextTrack,
    PrevTrack,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum KeyCode {
    Named(NamedKey),
    Char(char),
}

impl FromStr for KeyCode {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() == 1 {
            Ok(KeyCode::Char(s.chars().next().unwrap()))
        } else {
            Ok(KeyCode::Named(NamedKey::from_str(s)?))
        }
    }
}

impl Display for KeyCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyCode::Named(named) => {
                write!(f, "{}", named)
            }
            KeyCode::Char(c) => {
                write!(f, "{}", c)
            }
        }
    }
}

impl TryFrom<String> for KeyCode {
    type Error = <KeyCode as FromStr>::Err;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        KeyCode::from_str(&value)
    }
}

impl From<KeyCode> for String {
    fn from(value: KeyCode) -> Self {
        value.to_string()
    }
}