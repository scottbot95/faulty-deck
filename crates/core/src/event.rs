use crate::{ButtonId, SliderId};

#[derive(Debug, Copy, Clone)]
pub enum DeviceEvent {
    Button {
        id: ButtonId,
        pressed: bool,
    },
    Slider {
        id: SliderId,
        value: u16,
    },
}
