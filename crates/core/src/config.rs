use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use protocol::DeviceInfo;
use crate::action::{Action, AudioTarget};
use crate::{ButtonId, SliderId};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    pub device: DeviceConfig,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    pub buttons: BTreeMap<ButtonId, ButtonBinding>,
    pub sliders: BTreeMap<SliderId, SliderBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonBinding {
    pub tap: Option<Action>,
    // TODO add gestures
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SliderBinding {
    #[serde(flatten)]
    pub target: AudioTarget,
}

impl Config {
    pub fn check_device(&self, info: &DeviceInfo) -> Vec<String> {
        let mut errors = Vec::new();

        let device_config = &self.device;
        if device_config.buttons.len() > info.buttons as usize {
            errors.push(format!("Config contains {} buttons, device only has {}", device_config.buttons.len(), info.buttons));
        }
        if device_config.sliders.len() > info.sliders as usize {
            errors.push(format!("Config contains {} sliders, device only has {}", device_config.sliders.len(), info.sliders));
        }

        errors
    }
}

impl DeviceConfig {
    pub fn action(&self, button: ButtonId) -> Option<&Action> {
        self.buttons.get(&button)
            .and_then(|b| b.tap.as_ref())
    }

    pub fn slider_target(&self, slider: SliderId) -> Option<&AudioTarget> {
        self.sliders.get(&slider)
            .map(|s| &s.target)
    }
}
