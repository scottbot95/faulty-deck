use protocol::{DeviceInfo, DeviceState};
use crate::event::DeviceEvent;
use crate::{ButtonId, SliderId};

/// Tracks state changes on the device and generates events based on those changes.
pub struct DeviceTracker {
    info: DeviceInfo,
    prev: DeviceState,
}

impl DeviceTracker {
    pub fn new(info: DeviceInfo) -> Self {
        Self {
            info,
            prev: DeviceState::default(),
        }
    }

    /// Resync this [`DeviceTracker`] to some state.
    /// Previous state is entirely discarded and not diffed against.
    ///
    /// Will emit slider events, but never button events.
    pub fn resync(&mut self, state: DeviceState) -> Vec<DeviceEvent> {
        self.prev = state;

        let mut events = Vec::with_capacity(state.sliders.len());
        for (i, next) in self.prev.sliders.iter().enumerate() {
            events.push(DeviceEvent::Slider {
                id: SliderId(i as u8),
                value: *next,
            });
        }

        events
    }

    /// Applies a new state to this [`DeviceTracker`], generating events based on the changes.
    pub fn apply(&mut self, next: DeviceState) -> Vec<DeviceEvent> {
        let mut events = Vec::new();

        self.diff_sliders(&mut events, &next);
        self.diff_buttons(&mut events, &next);

        self.prev = next;

        events
    }

    fn diff_sliders(&self, events: &mut Vec<DeviceEvent>, next: &DeviceState) {
        let zipped = self.prev.sliders.iter().zip(next.sliders.iter());
        for (i, (prev, next)) in zipped.enumerate() {
            if prev != next {
                events.push(DeviceEvent::Slider {
                    id: SliderId(i as u8),
                    value: *next,
                });
            }
        }
    }

    fn diff_buttons(&self, events: &mut Vec<DeviceEvent>, next: &DeviceState) {
        let len = self.prev.buttons.bit_width().max(next.buttons.bit_width()) as u8;
        let prev = (0..len).map(|i| self.prev.button(i));
        let next = (0..len).map(|i| next.button(i));

        for (i, (prev, next)) in prev.zip(next).enumerate() {
            if prev != next {
                events.push(DeviceEvent::Button {
                    id: ButtonId(i as u8),
                    pressed: next,
                });
            }
        }
    }
}
