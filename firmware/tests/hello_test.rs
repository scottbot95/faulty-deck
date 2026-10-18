//! Demo test suite using embedded-test
//!
//! You can run this using `cargo test` as usual.

#![no_std]
#![no_main]

esp_bootloader_esp_idf::esp_app_desc!();

#[cfg(test)]
#[embedded_test::tests(executor = esp_rtos::embassy::Executor::new())]
mod tests {
    use usbd_hid::descriptor::SerializedDescriptor;
    use firmware::reports::VendorReport;
    use defmt::assert_eq;

    #[init]
    fn init() {
        rtt_target::rtt_init_defmt!();


        let peripherals = esp_hal::init(esp_hal::Config::default());

        let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
        esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    }

    #[test]
    async fn descriptor_bytes_are_stable() {
        const EXPECTED: &[u8] = &[
            0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01,
            0x09, 0x01, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x40, 0x81, 0x02,
            0x09, 0x02, 0x91, 0x02,
            0x09, 0x03, 0xB1, 0x02,
            0xC0,
        ];

        assert_eq!(VendorReport::desc(), EXPECTED);
    }
}
