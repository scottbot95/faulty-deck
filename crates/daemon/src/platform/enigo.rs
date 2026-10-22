use enigo::{Direction, Keyboard};
use daemon_core::key::{KeyCode, NamedKey};
use crate::executor::KeyInjector;

pub struct EnigoKeyInjector {
    enigo: enigo::Enigo,
}

impl EnigoKeyInjector {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            enigo: enigo::Enigo::new(&enigo::Settings::default())?,
        })
    }
}

impl KeyInjector for EnigoKeyInjector {
    fn chord(&mut self, keys: &[KeyCode]) -> anyhow::Result<()> {
        let keys = keys.iter()
            .map(key_code_to_enigo)
            .collect::<Vec<_>>();

        // Press all keys in order
        for key in &keys {
            self.enigo.key(*key, Direction::Press)?;
        }
        // Release the keys in reverse order
        for key in keys.iter().rev() {
            self.enigo.key(*key, Direction::Release)?;
        }

        Ok(())
    }

    fn text(&mut self, text: &str) -> anyhow::Result<()> {
        self.enigo.text(text)?;
        Ok(())
    }
}

fn key_code_to_enigo(code: &KeyCode) -> enigo::Key {
    match code {
        KeyCode::Named(NamedKey::Ctrl) => enigo::Key::Control,
        KeyCode::Named(NamedKey::Shift) => enigo::Key::Shift,
        KeyCode::Named(NamedKey::Alt) => enigo::Key::Alt,
        KeyCode::Named(NamedKey::Meta) => enigo::Key::Meta,
        KeyCode::Named(NamedKey::Enter) => enigo::Key::Return,
        KeyCode::Named(NamedKey::Escape) => enigo::Key::Escape,
        KeyCode::Named(NamedKey::Tab) => enigo::Key::Tab,
        KeyCode::Named(NamedKey::Space) => enigo::Key::Space,
        KeyCode::Named(NamedKey::Backspace) => enigo::Key::Backspace,
        KeyCode::Named(NamedKey::Delete) => enigo::Key::Delete,
        KeyCode::Named(NamedKey::Up) => enigo::Key::UpArrow,
        KeyCode::Named(NamedKey::Down) => enigo::Key::DownArrow,
        KeyCode::Named(NamedKey::Left) => enigo::Key::LeftArrow,
        KeyCode::Named(NamedKey::Right) => enigo::Key::RightArrow,
        KeyCode::Named(NamedKey::Home) => enigo::Key::Home,
        KeyCode::Named(NamedKey::End) => enigo::Key::End,
        KeyCode::Named(NamedKey::PageUp) => enigo::Key::PageUp,
        KeyCode::Named(NamedKey::PageDown) => enigo::Key::PageDown,
        KeyCode::Named(NamedKey::F1) => enigo::Key::F1,
        KeyCode::Named(NamedKey::F2) => enigo::Key::F2,
        KeyCode::Named(NamedKey::F3) => enigo::Key::F3,
        KeyCode::Named(NamedKey::F4) => enigo::Key::F4,
        KeyCode::Named(NamedKey::F5) => enigo::Key::F5,
        KeyCode::Named(NamedKey::F6) => enigo::Key::F6,
        KeyCode::Named(NamedKey::F7) => enigo::Key::F7,
        KeyCode::Named(NamedKey::F8) => enigo::Key::F8,
        KeyCode::Named(NamedKey::F9) => enigo::Key::F9,
        KeyCode::Named(NamedKey::F10) => enigo::Key::F10,
        KeyCode::Named(NamedKey::F11) => enigo::Key::F11,
        KeyCode::Named(NamedKey::F12) => enigo::Key::F12,
        KeyCode::Named(NamedKey::F13) => enigo::Key::F13,
        KeyCode::Named(NamedKey::F14) => enigo::Key::F14,
        KeyCode::Named(NamedKey::F15) => enigo::Key::F15,
        KeyCode::Named(NamedKey::F16) => enigo::Key::F16,
        KeyCode::Named(NamedKey::F17) => enigo::Key::F17,
        KeyCode::Named(NamedKey::F18) => enigo::Key::F18,
        KeyCode::Named(NamedKey::F19) => enigo::Key::F19,
        KeyCode::Named(NamedKey::F20) => enigo::Key::F20,
        KeyCode::Named(NamedKey::F21) => enigo::Key::F21,
        KeyCode::Named(NamedKey::F22) => enigo::Key::F22,
        KeyCode::Named(NamedKey::F23) => enigo::Key::F23,
        KeyCode::Named(NamedKey::F24) => enigo::Key::F24,
        KeyCode::Named(NamedKey::VolumeUp) => enigo::Key::VolumeUp,
        KeyCode::Named(NamedKey::VolumeDown) => enigo::Key::VolumeDown,
        KeyCode::Named(NamedKey::Mute) => enigo::Key::VolumeMute,
        KeyCode::Named(NamedKey::PlayPause) => enigo::Key::MediaPlayPause,
        KeyCode::Named(NamedKey::NextTrack) => enigo::Key::MediaNextTrack,
        KeyCode::Named(NamedKey::PrevTrack) => enigo::Key::MediaPrevTrack,
        KeyCode::Char(c) => enigo::Key::Unicode(*c),
    }
}
