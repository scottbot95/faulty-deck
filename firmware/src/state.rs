use core::cell::Cell;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use protocol::{DeviceState, Report};

static CURRENT: Mutex<CriticalSectionRawMutex, Cell<DeviceState>> =
    Mutex::new(Cell::new(DeviceState::INITIAL));
static CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub fn current() -> DeviceState {
    CURRENT.lock(Cell::get)
}

pub fn report() -> Report {
    protocol::encode(&current()).expect("DeviceState too large")
}

/// Blocks until the device state has changed
pub async fn changed() {
    CHANGED.wait().await;
}

pub fn force_change_signal() {
    CHANGED.signal(());
}

/// Updates the device state. Will signal [`changed`] if the state was actually modified
pub fn update<R>(f: impl FnOnce(&mut DeviceState) -> R) -> R {
    let (changed, result) = CURRENT.lock(|cell| {
        let old = cell.get();
        let mut new = old;
        let result = f(&mut new);
        cell.set(new);
        (new != old, result)
    });
    if changed {
        let already_signaled = CHANGED.signaled();
        log::info!("state_update: Firing change signal. Backlogged: {}", already_signaled);
        CHANGED.signal(());
    }
    result
}
