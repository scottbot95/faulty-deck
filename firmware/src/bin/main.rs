#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::time::Rate;
use esp_hal::{clock::CpuClock, rmt::Rmt};
use esp_hal::analog::adc::{Adc, AdcCalBasic, AdcConfig, Attenuation};
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::timer::timg::TimerGroup;
use esp_hal_smartled::{RmtSmartLeds, WS2812_TIMING, buffer_size, color_order};
use smart_leds::{RGB8, brightness, SmartLedsWriteAsync};
use smart_leds::hsv::{hsv2rgb, Hsv};
use protocol::reading_to_hue;

#[panic_handler]
fn panic(panic_info: &core::panic::PanicInfo) -> ! {
    error!("{}", panic_info);
    loop {}
}

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.4.0
    // generator parameters: -o esp32s3 -o esp32s3-wroom-1-octal-psram -o unstable-hal -o embassy -o probe-rs -o defmt -o embedded-test -o wokwi -o vscode -o esp

    rtt_target::rtt_init_defmt!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let rng = esp_hal::rng::Rng::new();

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO3
    // - GPIO45
    // - GPIO46
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _gpio27 = peripherals.GPIO27;
    let _gpio28 = peripherals.GPIO28;
    let _gpio29 = peripherals.GPIO29;
    let _gpio30 = peripherals.GPIO30;
    let _gpio31 = peripherals.GPIO31;
    let _gpio32 = peripherals.GPIO32;
    let _gpio33 = peripherals.GPIO33;
    let _gpio34 = peripherals.GPIO34;
    let _gpio35 = peripherals.GPIO35;
    let _gpio36 = peripherals.GPIO36;
    let _gpio37 = peripherals.GPIO37;

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let mut adc1_config = AdcConfig::new();
    let mut adc_pin = adc1_config.enable_pin_with_cal::<_, AdcCalBasic<_>>(peripherals.GPIO4, Attenuation::_11dB);
    let mut adc1 = Adc::new(peripherals.ADC1, adc1_config).into_async();

    let button = Input::new(peripherals.GPIO21, InputConfig::default().with_pull(Pull::Up));

    info!("Embassy initialized!");

    // TODO: Spawn some tasks
    let _ = spawner;

    let rate = Rate::from_mhz(80);
    let rmt = Rmt::new(peripherals.RMT, rate)
        .unwrap()
        .into_async();

    let mut led = RmtSmartLeds::<
        { buffer_size::<RGB8>(1) },
        _,
        RGB8,
        color_order::Grb
    >::new(
        WS2812_TIMING,
        rmt.channel0,
        peripherals.GPIO48,
        rate
    ).unwrap();

    loop {
        if button.is_low() {
            info!("Button held, skipping");
            led.write(brightness([RGB8::default()].into_iter(), 1))
                .await
                .unwrap();
            Timer::after(Duration::from_millis(50)).await;
            continue;
        }

        let raw_value = adc1.read_oneshot(&mut adc_pin).await;
        let hue = reading_to_hue(raw_value);
        info!("ADC value: {}, hue: {}", raw_value, hue);
        let color = hsv2rgb(Hsv {
            hue,
            sat: u8::MAX,
            val: u8::MAX,
        });
        info!("Color: r:{} g:{} b:{}", color.r, color.g, color.b);
        led.write(brightness([color].into_iter(), 1))
            .await
            .unwrap();

        // let bytes = rng.random().to_be_bytes();
        // let color = RGB8 {
        //     r: bytes[1],
        //     g: bytes[2],
        //     b: bytes[3],
        // };
        // info!("Color: r:{} g:{} b:{}", color.r, color.g, color.b);
        // led.write(brightness([color].into_iter(), 1))
        //     .await
        //     .unwrap();
        Timer::after(Duration::from_millis(50)).await;
    }
}
