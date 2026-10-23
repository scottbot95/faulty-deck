use embassy_usb::class::hid;
use embassy_usb::class::hid::{HidBootProtocol, HidReader, HidReaderWriter, HidSubclass, HidWriter, ReportId, RequestHandler};
use embassy_usb::control::OutResponse;
use embassy_usb::UsbDevice;
use esp_hal::peripherals::USB_FS;
use esp_hal::usb::otg::{Usb, UsbFsDm, UsbFsDp};
use esp_hal::usb::otg::embassy_usb_device::{Config as OtgConfig, Driver};
use static_cell::{ConstStaticCell, StaticCell};
use usbd_hid::descriptor::SerializedDescriptor;
use protocol::{Report, REPORT_LEN, DeviceInfo};
use crate::reports::VendorReport;

pub type UsbDriver = Driver<'static>;
pub type Device = UsbDevice<'static, UsbDriver>;
pub type HidIn = HidWriter<'static, UsbDriver, REPORT_LEN>;
pub type HidOut = HidReader<'static, UsbDriver, REPORT_LEN>;


/// Collection of various pieces of the USB stack for spawning independent tasks
pub struct UsbParts {
    pub device: Device,
    pub hid_in: HidIn,
    pub hid_out: HidOut,
}

struct FeatureHandler {
    device_info: Report,
}

impl FeatureHandler {
    pub fn new(info: &DeviceInfo) -> Result<Self, postcard::Error> {
        let device_info = protocol::encode(info)?;
        Ok(Self { device_info })
    }
}

impl RequestHandler for FeatureHandler {
    /// `In` is the host asking for the current input report. Used to sync state on connect
    /// `Feature` is the connect-time handshake.
    fn get_report(&mut self, id: ReportId, buf: &mut [u8]) -> Option<usize> {
        let report = match id {
            ReportId::In(_) => crate::state::report(),
            ReportId::Feature(_) => self.device_info,
            _ => return None,
        };
        let n = report.len().min(buf.len());
        buf[..n].copy_from_slice(&report[..n]);
        Some(n)
    }

    fn set_report(&mut self, _id: ReportId, _data: &[u8]) -> OutResponse {
        OutResponse::Accepted
    }
}

pub fn init_usb(
    usb: USB_FS<'static>,
    usb_dp: impl UsbFsDp + 'static,
    usb_dm: impl UsbFsDm + 'static,
    serial: &'static str,
    info: &DeviceInfo,
) -> Result<UsbParts, postcard::Error> {
    static EP_OUT_BUF: ConstStaticCell<[u8; 1024]> = ConstStaticCell::new([0; 1024]);
    static CONFIG_DESC: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; 256]);
    static BOS_DESC: ConstStaticCell<[u8; 256]> = ConstStaticCell::new([0; 256]);
    static CONTROL_BUF: ConstStaticCell<[u8; 64]> = ConstStaticCell::new([0; 64]);
    static HID_STATE: StaticCell<hid::State<'static>> = StaticCell::new();
    static FEATURE_HANDLER: StaticCell<FeatureHandler> = StaticCell::new();

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

    let hid_config = hid::Config {
        report_descriptor: VendorReport::desc(),
        request_handler: Some(FEATURE_HANDLER.init(FeatureHandler::new(info)?)),
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

    Ok(UsbParts {
        device: builder.build(),
        hid_in,
        hid_out,
    })
}
