#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    display::DirtySprite,
    rtc::{Bm8563, DateTime},
    ui::{Label, StatusBar, Theme},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use esp_hal::time::Duration;
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

const SPRITE_W: u16 = 260;
const SPRITE_H: u16 = 88;
const SPRITE_ORIGIN: Point = Point::new(24, 112);

type RtcSprite = DirtySprite<Rgb565, SPRITE_W, SPRITE_H, { 260 * 88 }, 8>;

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut sprite = RtcSprite::new(Rgb565::BLACK).expect("valid RTC sprite");

    let mut parts = CoreS3::init_display(CoreS3DisplayResources {
        i2c0: peripherals.I2C0,
        i2c_sda: peripherals.GPIO12,
        i2c_scl: peripherals.GPIO11,
        spi2: peripherals.SPI2,
        lcd_sclk: peripherals.GPIO36,
        lcd_mosi: peripherals.GPIO37,
        lcd_dc: peripherals.GPIO35,
        lcd_cs: peripherals.GPIO3,
        tf_card_cs: peripherals.GPIO4,
    })
    .expect("display");

    let mut rtc = Bm8563::new(parts.internal_i2c);
    let init_ok = rtc.init().is_ok();
    let first_read = rtc.datetime().ok();
    let integrity_lost = rtc.clock_integrity_lost().ok();

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 BM8563 RTC demo",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 174))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "RTC LIVE READ",
        top_left: Point::new(24, 62),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");
    Label {
        text: if init_ok {
            "BM8563 init: OK addr 0x51"
        } else {
            "BM8563 init: FAILED addr 0x51"
        },
        top_left: Point::new(24, 88),
        color: if init_ok { Rgb565::CYAN } else { Rgb565::RED },
    }
    .draw(display)
    .expect("init");
    Rectangle::new(SPRITE_ORIGIN - Point::new(2, 2), Size::new(264, 92))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(0, 18, 12), 1))
        .draw(display)
        .ok();

    draw_rtc(&mut sprite, first_read, integrity_lost, 0);
    sprite
        .flush_dirty_at(display, SPRITE_ORIGIN)
        .expect("initial RTC flush");

    let delay = esp_hal::delay::Delay::new();
    let mut tick = 0u32;
    let mut last_second = first_read.map(|dt| dt.time.second);

    loop {
        delay.delay(Duration::from_millis(250));
        tick = tick.wrapping_add(1);
        let datetime = rtc.datetime().ok();
        let integrity_lost = rtc.clock_integrity_lost().ok();
        let second = datetime.map(|dt| dt.time.second);
        if tick.is_multiple_of(4) || second != last_second {
            draw_rtc(&mut sprite, datetime, integrity_lost, tick);
            sprite
                .flush_dirty_at(display, SPRITE_ORIGIN)
                .expect("flush RTC sprite");
            last_second = second;
        }
    }
}

fn draw_rtc<T>(sprite: &mut T, datetime: Option<DateTime>, integrity_lost: Option<bool>, tick: u32)
where
    T: DrawTarget<Color = Rgb565>,
{
    sprite.clear(Rgb565::BLACK).ok();

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    let mut line: String<64> = String::new();
    write!(&mut line, "tick: {tick}").unwrap();
    Text::new(&line, Point::new(0, 12), style).draw(sprite).ok();

    match datetime {
        Some(datetime) => {
            line.clear();
            write!(
                &mut line,
                "date: {:04}-{:02}-{:02} wd:{}",
                datetime.date.year, datetime.date.month, datetime.date.day, datetime.date.weekday
            )
            .unwrap();
            Text::new(&line, Point::new(0, 32), accent)
                .draw(sprite)
                .ok();

            line.clear();
            write!(
                &mut line,
                "time: {:02}:{:02}:{:02}",
                datetime.time.hour, datetime.time.minute, datetime.time.second
            )
            .unwrap();
            Text::new(&line, Point::new(0, 52), accent)
                .draw(sprite)
                .ok();
        }
        None => {
            Text::new("datetime read failed", Point::new(0, 32), error)
                .draw(sprite)
                .ok();
        }
    }

    match integrity_lost {
        Some(true) => Text::new("VL flag: LOST - time invalid", Point::new(0, 70), warn),
        Some(false) => Text::new("VL flag: OK - time valid", Point::new(0, 70), style),
        None => Text::new("VL flag: read failed", Point::new(0, 70), error),
    }
    .draw(sprite)
    .ok();
}
