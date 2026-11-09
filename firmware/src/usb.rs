use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, TrySendError};
use embassy_usb::class::hid;
use embassy_usb::class::hid::{HidBootProtocol, HidReader, HidReaderWriter, HidSubclass, HidWriter, ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb::UsbDevice;
use esp_hal::peripherals::USB_FS;
use esp_hal::usb::otg::{Usb, UsbFsDm, UsbFsDp};
use esp_hal::usb::otg::embassy_usb_device::{Config as OtgConfig, Driver};
use log::{debug, error, info, trace, warn};
use static_cell::{ConstStaticCell, StaticCell};
use usbd_hid::descriptor::SerializedDescriptor;
use protocol::{REPORT_LEN, DeviceInfo, Report};
use protocol::event::Event;
use crate::commands::CmdSender;
use crate::reports::VendorReport;

pub type UsbDriver = Driver<'static>;
pub type Device = UsbDevice<'static, UsbDriver>;
pub type HidIn = HidWriter<'static, UsbDriver, REPORT_LEN>;
pub type HidOut = HidReader<'static, UsbDriver, REPORT_LEN>;

pub static EVENTS: Channel<CriticalSectionRawMutex, Event, 4> = Channel::new();

/// Collection of various pieces of the USB stack for spawning independent tasks
pub struct UsbParts {
    pub device: Device,
    pub hid_in: HidIn,
    pub hid_out: HidOut,
}

#[derive(Copy, Clone)]
struct FeatureHandler {
    cmd_tx: CmdSender,
}

impl FeatureHandler {
    fn new(cmd_tx: CmdSender) -> Self {
        Self { cmd_tx }
    }
}

impl RequestHandler for FeatureHandler {
    /// `In` is the host asking for the current input report. Used to sync state on connect
    /// `Feature` is the connect-time handshake.
    fn get_report(&mut self, id: ReportId, buf: &mut [u8]) -> Option<usize> {
        let report = match id {
            ReportId::In(_) => crate::state::report(),
            ReportId::Feature(_) => crate::info::report(),
            _ => return None,
        };
        let n = report.len().min(buf.len());
        buf[..n].copy_from_slice(&report[..n]);
        Some(n)
    }

    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        match id {
            ReportId::Out(0) => {
                let Ok(cmd) = protocol::decode(data) else {
                    warn!("Received invalid command from host");
                    return OutResponse::Rejected;
                };
                if let Err(TrySendError::Full(cmd)) = self.cmd_tx.try_send(cmd) {
                    warn!("Command channel full, dropping command {:?}", cmd);
                }
                OutResponse::Accepted
            }
            _ => {
                OutResponse::Rejected
            }
        }
    }
}

pub fn init_usb(
    spawner: &Spawner,
    usb: USB_FS<'static>,
    usb_dp: impl UsbFsDp + 'static,
    usb_dm: impl UsbFsDm + 'static,
    serial: &'static str,
    info: &DeviceInfo,
    cmd_tx: CmdSender,
) -> Result<(), postcard::Error> {
    static EP_OUT_BUF: ConstStaticCell<[u8; 1024]> = ConstStaticCell::new([0; 1024]);
    static CONFIG_DESC: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; 256]);
    static BOS_DESC: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; 256]);
    static CONTROL_BUF: ConstStaticCell<[u8; 64]> = ConstStaticCell::new([0; 64]);
    static HID_STATE: StaticCell<hid::State<'static>> = StaticCell::new();
    static FEATURE_HANDLER: StaticCell<FeatureHandler> = StaticCell::new();
    static USB_DEVICE: StaticCell<Device> = StaticCell::new();

    crate::info::set(info)?;

    let usb = Usb::new_fs(usb, usb_dp, usb_dm);
    let driver = Driver::new(usb, EP_OUT_BUF.take(), OtgConfig::default());

    let mut config = embassy_usb::Config::new(0x1209, 0x0001);
    config.manufacturer = Some("FaultyMuse");
    config.product = Some("FaultyDeck");
    config.serial_number = Some(serial);
    config.max_power = 100;

    let mut builder = embassy_usb::Builder::new(
        driver,
        config,
        CONFIG_DESC.take(),
        BOS_DESC.take(),
        &mut [],
        CONTROL_BUF.take()
    );

    let handler = FEATURE_HANDLER.init(FeatureHandler::new(cmd_tx));
    let handler_2 = *handler;
    let hid_config = hid::Config {
        report_descriptor: VendorReport::desc(),
        request_handler: Some(handler),
        poll_ms: 1,
        max_packet_size: REPORT_LEN as u16,
        hid_subclass: HidSubclass::No,
        hid_boot_protocol: HidBootProtocol::None,
    };
    let hid = HidReaderWriter::<_, REPORT_LEN, REPORT_LEN>::new(
        &mut builder,
        HID_STATE.init_with(hid::State::new),
        hid_config
    );
    let (hid_out, hid_in) = hid.split();
    let device = USB_DEVICE.init(builder.build());

    spawner.spawn(usb_task(device).unwrap());
    spawner.spawn(hid_writer_task(hid_in).unwrap());
    spawner.spawn(hid_out_drain(hid_out, handler_2).unwrap());

    Ok(())
}



/// Run Embassy USB stack.
#[embassy_executor::task]
async fn usb_task(device: &'static mut Device) {
    info!("Starting USB stack");
    device.run().await
}

/// Consumes events from the [`EventReceiver`] and writes them to the HID device.
#[embassy_executor::task]
#[allow(
    clippy::large_stack_frames,
    reason = "IDK why this needs so much..."
)]
async fn hid_writer_task(mut hid: HidIn) {
    loop {
        debug!("hid_writer: Waiting for state change");
        match select(crate::state::changed(), EVENTS.receive()).await {
            Either::First(()) => {
                info!("hid_writer: Writing output report");
                write_report(&mut hid, crate::state::report).await;

                // Non-blocking quick loop to drain events
                while let Ok(event) = EVENTS.try_receive() {
                    write_event(&mut hid, &event).await;
                }
            }
            Either::Second(event) => write_event(&mut hid, &event).await,
        }
    }
}

async fn write_event(hid: &mut HidIn, event: &Event) {
    trace!("hid_writer: Writing event {:?}", event);
    match protocol::encode(event) {
        Ok(report) => write_report(hid, || report).await,
        Err(e) => error!("hid_writer: failed to encode event: {:?}", e),
    }
}

async fn write_report(hid: &mut HidIn, report_fn: impl Fn() -> Report) {
    if hid.write(&report_fn()).await.is_err() {
        warn!("hid_writer: HID report failed, waiting until ready");
        // If the write failed, that means we don't have an active connection to a host
        hid.ready().await;
        info!("hid_writer: HidOut ready");
        let _ = hid.write(&report_fn()).await;
        info!("hid_writer: HID report sent");
    }
}

/// Consumes commands from the HID host and executes them
#[embassy_executor::task]
async fn hid_out_drain(hid: HidOut, mut handler: FeatureHandler) {
    hid.run(false, &mut handler).await;
}
