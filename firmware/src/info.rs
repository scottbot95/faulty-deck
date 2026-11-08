use core::cell::Cell;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use log::error;
use protocol::{Report, DeviceInfo};

static INFO: Mutex<CriticalSectionRawMutex, Cell<Report>> = Mutex::new(Cell::new([0; _]));

/// Store `info` as the device info.
pub fn set(info: &DeviceInfo) -> Result<(), postcard::Error> {
    let report = protocol::encode(info)?;
    INFO.lock(|cell| cell.set(report));
    Ok(())
}

/// Fetch the pre-encoded device info.
pub fn report() -> Report {
    INFO.lock(Cell::get)
}

/// Update the device info.
pub fn update(f: impl FnOnce(&mut DeviceInfo)) {
    INFO.lock(|cell| {
        let report = cell.get();
        let mut info = match protocol::decode(&report) {
            Ok(info) => info,
            Err(e) => {
                error!("failed to decode device info: {:?}", e);
                return;
            }
        };
        f(&mut info);
        match protocol::encode(&info) {
            Ok(report) => cell.set(report),
            Err(e) => error!("failed to encode device info: {:?}", e),
        }
    })
}
