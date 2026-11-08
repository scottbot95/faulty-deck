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
use embassy_time::{with_timeout, Duration, Timer};
use esp_hal::analog::adc::{Adc, AdcCalBasic, AdcConfig, Attenuation};
use esp_hal::clock::CpuClock;
use esp_hal::efuse::MacAddress;
use esp_hal::gpio::{AnyPin, Input, InputConfig, Pull};
use esp_hal::peripherals::{ADC1, GPIO1, GPIO2, GPIO4, GPIO5, TIMG1};
use esp_hal::rmt::Rmt;
use esp_hal::timer::timg::{MwdtStage, TimerGroup, Wdt};
use heapless::String;
use log::{error, info};
use smart_leds::{brightness, colors};
use static_cell::StaticCell;
use firmware::{boot, updater};
use firmware::boot::BootKind;
use firmware::commands::spawn_command_task;
use firmware::smart_led::{unsafe_write_led, SmartLed, SMART_LED_RATE};
use protocol::{DeviceInfo, MAX_SLIDER_VALUE, PROTOCOL_VERSION, VERSION_STRING_MAX_LEN};
use firmware::usb::init_usb;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);

    unsafe_write_led(brightness([colors::RED].into_iter(), 1));

    match boot::boot_kind() {
        BootKind::Trial | BootKind::Unknown => loop {},
        BootKind::Normal | BootKind::RolledBack => {
            // Disable the watchdog to prevent marking this partition as bad and resetting
            Wdt::<TIMG1<'static>>::new().disable();
            loop {}
        }
    }
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

const FULL_VERSION: &str = env!("CARGO_PKG_VERSION");

const _: () = assert!(
    FULL_VERSION.len() <= VERSION_STRING_MAX_LEN,
    "CARGO_PKG_VERSION is longer than VERSION_STR_CAP: shorten the version or raise the capacity"
);

// 1% of the available range (max+1 since we need the size of the range and zero is included)
const SLIDER_DEADBAND: u16 = (MAX_SLIDER_VALUE + 1) / 100;
const BUTTON_COUNT: usize = 16;
const SLIDER_COUNT: u8 = 4;
const DEBOUNCE: Duration = Duration::from_millis(10);
const STATUS_LED_BRIGHTNESS: u8 = 10;
/// Timeout to wait after the first boot of a new image before marking it as good
const CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout for the watchdog to wait before marking ota partition as bad and resetting the device
const WATCHDOG_TIMEOUT: esp_hal::time::Duration = esp_hal::time::Duration::from_secs(5);
/// Interval to feed the watchdog
const WATCHDOG_FEED_INTERVAL: Duration = Duration::from_secs(1);

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
    let mut peripherals = esp_hal::init(config);

    // Immediately start the RTOS so we can start using async code
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let boot_info = boot::check(peripherals.FLASH.reborrow());
    let boot_kind = boot_info.kind;

    let mut wdt = TimerGroup::new(peripherals.TIMG1).wdt;
    wdt.set_timeout(MwdtStage::Stage0, WATCHDOG_TIMEOUT);
    wdt.enable();
    spawner.spawn(watchdog_task(wdt).unwrap());

    // Build LED immediately after starting the RTOS so we can use it to indicate boot state
    let rmt = Rmt::new(peripherals.RMT, SMART_LED_RATE).unwrap().into_async();
    let mut led = SmartLed::new(rmt.channel0, peripherals.GPIO48);
    let boot_color = if boot_kind == BootKind::Trial { colors::BLUE } else { colors::ORANGE };
    led.write(boot_color, STATUS_LED_BRIGHTNESS).await;
    info!("boot: Initializing peripherals");

    // Strapping pins (can mess with boot so we should avoid using them outside of boot)
    reserve_pins!(peripherals; GPIO0, GPIO3, GPIO45, GPIO46);
    // These GPIO pins are in use by some feature of the module and should not be used.
    reserve_pins!(peripherals; GPIO27, GPIO28, GPIO29, GPIO30, GPIO31, GPIO32, GPIO33, GPIO34, GPIO35, GPIO36, GPIO37);

    let button_pins: [AnyPin<'static>; BUTTON_COUNT] = [
        peripherals.GPIO6.into(),
        peripherals.GPIO7.into(),
        peripherals.GPIO15.into(),
        peripherals.GPIO16.into(),
        peripherals.GPIO17.into(),
        peripherals.GPIO18.into(),
        peripherals.GPIO11.into(),
        peripherals.GPIO12.into(),
        peripherals.GPIO13.into(),
        peripherals.GPIO21.into(),
        peripherals.GPIO42.into(),
        peripherals.GPIO41.into(),
        peripherals.GPIO40.into(),
        peripherals.GPIO39.into(),
        peripherals.GPIO38.into(),
        peripherals.GPIO14.into(),
    ];

    let mac = esp_hal::efuse::base_mac_address();
    let serial = serial_string(&mac);
    let info = DeviceInfo {
        protocol_version: PROTOCOL_VERSION,
        firmware_version: String::try_from(FULL_VERSION).unwrap(),
        buttons: button_pins.len() as u8,
        sliders: SLIDER_COUNT,
        serial: mac.as_bytes().try_into().unwrap(),
        running_slot: boot_info.running_slot,
        last_update_result: boot_info.last_update_result,
        failed_version: boot_info.failed_version,
    };

    info!("boot: fw=v{} serial={serial}", info.firmware_version);
    led.write(colors::YELLOW, STATUS_LED_BRIGHTNESS).await;

    firmware::info::set(&info).expect("DeviceInfo too large");

    let cmd_tx = spawn_command_task(&spawner, peripherals.FLASH);
    init_usb(
        &spawner,
        peripherals.USB_FS,
        peripherals.GPIO20,
        peripherals.GPIO19,
        serial,
        &info,
        cmd_tx
    ).expect("Failed to initialize USB");

    let pull_up = InputConfig::default().with_pull(Pull::Up);
    for (id, pin) in button_pins.into_iter().enumerate() {
        spawner.spawn(button_task(Input::new(pin, pull_up), id as u8).unwrap());
    }

    spawner.spawn(slider_task(peripherals.ADC1, peripherals.GPIO1, peripherals.GPIO2, peripherals.GPIO4, peripherals.GPIO5).unwrap());

    match boot_kind {
        BootKind::Normal | BootKind::Unknown => led.write(colors::GREEN, STATUS_LED_BRIGHTNESS).await,
        BootKind::Trial => {
            led.write(colors::BLUE, STATUS_LED_BRIGHTNESS).await;
            spawner.spawn(confirm_task(led).unwrap());
        }
        BootKind::RolledBack => led.write(colors::MAGENTA, STATUS_LED_BRIGHTNESS).await,
    }
    info!("boot: Done!");
}

/// Marks the running image valid once the host has synced or after [`CONFIRM_TIMEOUT`]
#[embassy_executor::task]
async fn confirm_task(mut led: SmartLed<'static, esp_hal::Async>) {
    if with_timeout(CONFIRM_TIMEOUT, boot::HOST_SYNCED.wait()).await.is_err() {
        info!("no host sync within {}s", CONFIRM_TIMEOUT.as_secs());
    }
    updater::CONFIRM.signal(());
    match updater::CONFIRMED.wait().await {
        Ok(()) => {
            info!("image marked valid");
            led.write(colors::GREEN, STATUS_LED_BRIGHTNESS).await;
        }
        Err(e) => error!("failed to mark image valid: {:?}", e)
    }
}

#[embassy_executor::task]
async fn watchdog_task(mut wdt: Wdt<TIMG1<'static>>) {
    loop {
        wdt.feed();
        Timer::after(WATCHDOG_FEED_INTERVAL).await;
    }
}

#[embassy_executor::task(pool_size = BUTTON_COUNT)]
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
async fn slider_task(adc: ADC1<'static>, pot0: GPIO1<'static>, pot1: GPIO2<'static>, pot2: GPIO4<'static>, pot3: GPIO5<'static>) {
    let mut config = AdcConfig::new();

    let mut pot0 = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(pot0, Attenuation::_11dB);
    let _pot1 = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(pot1, Attenuation::_11dB);
    let _pot2 = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(pot2, Attenuation::_11dB);
    let _pot3 = config.enable_pin_with_cal::<_, AdcCalBasic<_>>(pot3, Attenuation::_11dB);

    let mut adc = Adc::new(adc, config).into_async();

    loop {
        // FIXME pots 1, 2, and 3 aren't working cuz I fucked the wiring
        let readings = [
            adc.read_oneshot(&mut pot0).await,
            0,
            // adc.read_oneshot(&mut pot1).await,
            0,
            // adc.read_oneshot(&mut pot2).await,
            0,
            // adc.read_oneshot(&mut pot3).await,
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