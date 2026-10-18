#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use core::fmt::Write;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcCalBasic, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::efuse::MacAddress;
use esp_hal::gpio::{AnyPin, Input, InputConfig, Pull};
use esp_hal::peripherals::{ADC1, GPIO1};
use esp_hal::rmt::Rmt;
use esp_hal::timer::timg::TimerGroup;
use heapless::String;
use log::{error, info, trace, warn};
use smart_leds::{brightness, colors};
use static_cell::StaticCell;
use firmware::smart_led::{unsafe_write_led, SmartLed, SMART_LED_RATE};
use protocol::{DeviceInfo, PROTOCOL_VERSION, REPORT_LEN};
use firmware::usb::{init_usb, Device, HidIn, HidOut};

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);

    unsafe_write_led(brightness([colors::RED].into_iter(), 1));

    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// TODO load this from Cargo.toml. Maybe need to write a source module in build.rs?
const FIRMWARE_VERSION: (u8, u8, u8) = (0, 1, 0);
// 1% of the availalbe range
const SLIDER_DEADBAND: u16 = 4096 / 100; // TODO make this configurable
const BUTTON_COUNT: usize = 4;
const SLIDER_COUNT: u8 = 3;
const DEBOUNCE: Duration = Duration::from_millis(10);

macro_rules! reserve_pins {
    ($peripherals:ident; $($pin:ident),+ $(,)?) => {
        let _reserved = ($($peripherals.$pin,)+);
    };
}

#[esp_rtos::main]
#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    info!("boot: Hello!");

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // Immediately start the RTOS so we can start using async code
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // Build LED immediately after starting the RTOS so we can use it to indicate boot state
    let rmt = Rmt::new(peripherals.RMT, SMART_LED_RATE).unwrap().into_async();
    let mut led = SmartLed::new(rmt.channel0, peripherals.GPIO48);
    led.write(colors::ORANGE, 10).await;
    info!("boot: Initializing peripherals");

    // Strapping pins (can mess with boot so we should avoid using them outside of boot)
    reserve_pins!(peripherals; GPIO0, GPIO3, GPIO45, GPIO46);
    // These GPIO pins are in use by some feature of the module and should not be used.
    reserve_pins!(peripherals; GPIO27, GPIO28, GPIO29, GPIO30, GPIO31, GPIO32, GPIO33, GPIO34, GPIO35, GPIO36, GPIO37);

    let button_pins: [AnyPin<'static>; BUTTON_COUNT] = [
        peripherals.GPIO4.into(),
        peripherals.GPIO5.into(),
        peripherals.GPIO6.into(),
        peripherals.GPIO7.into(),
    ];

    let mac = esp_hal::efuse::base_mac_address();
    let serial = serial_string(&mac);

    info!("boot: fw=v{}.{}.{} serial={serial}", FIRMWARE_VERSION.0, FIRMWARE_VERSION.1, FIRMWARE_VERSION.2);
    led.write(colors::YELLOW, 10).await;

    let info = DeviceInfo {
        protocol_version: PROTOCOL_VERSION,
        firmware_version: FIRMWARE_VERSION,
        buttons: button_pins.len() as u8,
        sliders: SLIDER_COUNT,
        serial: mac.as_bytes().try_into().unwrap(),
    };
    let usb = init_usb(peripherals.USB_FS, peripherals.GPIO20, peripherals.GPIO19, serial, &info)
        .expect("Failed to initialize USB");

    static USB_DEVICE: StaticCell<Device> = StaticCell::new();

    spawner.spawn(usb_task(USB_DEVICE.init(usb.device)).unwrap());
    spawner.spawn(hid_writer_task(usb.hid_in).unwrap());
    spawner.spawn(hid_out_drain(usb.hid_out).unwrap());

    let pull_up = InputConfig::default().with_pull(Pull::Up);
    for (id, pin) in button_pins.into_iter().enumerate() {
        spawner.spawn(button_task(Input::new(pin, pull_up), id as u8).unwrap());
    }

    spawner.spawn(slider_task(peripherals.ADC1, peripherals.GPIO1).unwrap());

    led.write(colors::GREEN, 10).await;
    info!("boot: Done!");
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
        info!("hid_writer: Waiting for state change");
        firmware::state::changed().await;
        info!("hid_writer: Writing output report");
        if hid.write(&firmware::state::report()).await.is_err() {
            warn!("hid_writer: HID report failed, waiting until ready");
            // If the write failed, that means we don't have an active connection to a host
            hid.ready().await;
            info!("hid_writer: HidOut ready");
            let _ = hid.write(&firmware::state::report()).await;
            info!("hid_writer: HID report sent");
        }
    }
}

/// Consumes events from the HID device and discards them since we don't receive any commands from the host.
#[embassy_executor::task]
async fn hid_out_drain(mut hid: HidOut) {
    let mut buf = [0u8; REPORT_LEN];
    loop {
        let result = hid.read(&mut buf).await;
        match result {
            Ok(_) => {
                trace!("hid_out_drain: HID output report received");
            }
            Err(e) => {
                warn!("hid_out_drain: HID output report failed. Waiting for ready: {:?}", e);
                hid.ready().await;
                info!("hid_out_drain: HidOut ready");
            }
        }
    }
}

#[embassy_executor::task(pool_size = BUTTON_COUNT as usize)]
async fn button_task(mut pin: Input<'static>, id: u8) {
    // Configure initial button state
    firmware::state::update(|s| s.set_button(id, pin.is_low()));
    loop {
        // Wait for changes, then update state
        pin.wait_for_any_edge().await;
        Timer::after(DEBOUNCE).await;
        let pressed = pin.is_low();
        firmware::state::update(|s| s.set_button(id, pressed));
        info!("button_task: button {} {}", id, if pressed { "down" } else { "up" });
    }
}

#[embassy_executor::task]
async fn slider_task(adc: ADC1<'static>, pot0: GPIO1<'static>) {
    let mut config = AdcConfig::new();
    let mut pot0 = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(pot0, Attenuation::_11dB);
    // let mut pot1 = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(pot1, Attenuation::_11dB);
    let mut adc = Adc::new(adc, config).into_async();

    loop {
        let readings = [
            adc.read_oneshot(&mut pot0).await,
            // adc.read_oneshot(&mut pot1).await
        ];
        let changed = firmware::state::update(|s| {
            let mut changed = false;
            for (slot, value) in s.sliders.iter_mut().zip(readings) {
                if slot.abs_diff(value) >= SLIDER_DEADBAND {
                    *slot = value;
                    changed = true;
                }
            }
            changed.then_some(s.sliders)
        });
        if let Some(changed) = changed {
            info!("slider_task: sliders changed: {:?}", changed);
        }
        Timer::after(Duration::from_millis(10)).await;
    }
}

fn serial_string(mac: &MacAddress) -> &'static str {
    static SERIAL: StaticCell<String<18>> = StaticCell::new();
    let mut s = String::new();
    let _ = write!(s, "{}", mac);
    SERIAL.init(s).as_str()
}