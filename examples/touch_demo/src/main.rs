#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    touch::{Ft6336u, TouchPhase},
    ui::{Label, StatusBar, Theme},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use esp_hal::time::Duration;
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
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

    let mut touch = Ft6336u::new(parts.internal_i2c);
    let touch_ready = touch.init().is_ok();
    let delay = esp_hal::delay::Delay::new();

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 FT6336U touch demo",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "TOUCH DEMO",
        top_left: Point::new(24, 62),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");
    Label {
        text: "Touch or drag on the screen",
        top_left: Point::new(24, 88),
        color: Rgb565::WHITE,
    }
    .draw(display)
    .expect("instruction");
    Label {
        text: if touch_ready {
            "FT6336U init: OK"
        } else {
            "FT6336U init: FAILED"
        },
        top_left: Point::new(24, 106),
        color: if touch_ready {
            Rgb565::CYAN
        } else {
            Rgb565::RED
        },
    }
    .draw(display)
    .expect("init status");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let mut last_point = None;

    loop {
        delay.delay(Duration::from_millis(40));

        Rectangle::new(Point::new(24, 126), Size::new(260, 78))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
            .draw(display)
            .expect("clear status area");

        match touch.read_report() {
            Ok(report) => {
                if let Some(event) = report.events.into_iter().flatten().next() {
                    if let Some(old) = last_point {
                        Circle::new(old - Point::new(5, 5), 10)
                            .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
                            .draw(display)
                            .expect("erase old marker");
                    }
                    last_point = Some(event.point);
                    Circle::new(event.point - Point::new(5, 5), 10)
                        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
                        .draw(display)
                        .expect("draw marker");

                    let mut line: String<64> = String::new();
                    let phase = match event.phase {
                        TouchPhase::Down => "down",
                        TouchPhase::Move => "move",
                        TouchPhase::Up => "up",
                    };
                    write!(
                        &mut line,
                        "touch {} id={} x={} y={}",
                        phase, event.id, event.point.x, event.point.y
                    )
                    .unwrap();
                    Text::new(&line, Point::new(24, 144), accent)
                        .draw(display)
                        .expect("touch line");
                } else {
                    Text::new("waiting for touch...", Point::new(24, 144), style)
                        .draw(display)
                        .expect("waiting");
                }
            }
            Err(_) => {
                Text::new(
                    "touch read failed",
                    Point::new(24, 144),
                    MonoTextStyle::new(&FONT_6X10, Rgb565::RED),
                )
                .draw(display)
                .expect("read failed");
            }
        }
    }
}
