use esp_hal::DriverMode;
use esp_hal::peripherals::{GPIO48, RMT};
use esp_hal::rmt::{Rmt, TxChannelCreator};
use esp_hal::time::Rate;
use esp_hal_smartled::{buffer_size, color_order, RmtSmartLeds, WS2812_TIMING};
use esp_hal_smartled::color_order::Grb;
use smart_leds::{SmartLedsWrite, SmartLedsWriteAsync, RGB8, brightness};

pub const SMART_LED_RATE: Rate = Rate::from_mhz(80);

pub struct SmartLed<'a, Mode: DriverMode> {
    led: RmtSmartLeds<'a, { buffer_size::<RGB8>(1) }, Mode, RGB8, Grb>
}

impl<'a, Mode: DriverMode> SmartLed<'a, Mode> {
    pub fn new<Ch>(rmt_channel: Ch, led_pin: GPIO48<'a>) -> Self
    where
        Ch: TxChannelCreator<'a, Mode>,
    {
        let led = RmtSmartLeds::new(
            WS2812_TIMING,
            rmt_channel,
            led_pin,
            SMART_LED_RATE
        ).unwrap();

        Self {
            led
        }
    }
}

impl<'a> SmartLed<'a, esp_hal::Async> {
    pub async fn write(&mut self, color: RGB8, bright: u8) {
        // Errors shouldn't really be possible, so it's ok to just drop them
        let _ = self.led.write(brightness([color].into_iter(), bright)).await;
    }
}

impl<'a> SmartLed<'a, esp_hal::Blocking> {
    pub fn write(&mut self, color: RGB8, bright: u8) {
        // Errors shouldn't really be possible, so it's ok to just drop them
        let _ = self.led.write(brightness([color].into_iter(), bright));
    }
}

pub fn unsafe_write_led<T, I>(iterator: T)
where
    T: IntoIterator<Item = I>,
    I: Into<RGB8>
{
    // Sadly can't re-use SmartLed

    let rmt = unsafe { RMT::steal() };
    let led_pin = unsafe { GPIO48::steal() };
    let Ok(rmt) = Rmt::new(rmt, SMART_LED_RATE) else { return };

    let led = RmtSmartLeds::<
        { buffer_size::<RGB8>(1) },
        _,
        RGB8,
        color_order::Grb
    >::new(
        WS2812_TIMING,
        rmt.channel0,
        led_pin,
        SMART_LED_RATE
    );

    let Ok(mut led) = led else { return };

    let _ =led.write(iterator);
}
