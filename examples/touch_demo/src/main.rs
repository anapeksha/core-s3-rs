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
    let error_style = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);
    let status_line = Rectangle::new(Point::new(24, 132), Size::new(260, 18));
    let mut last_point = None;
    let mut last_state = TouchDisplayState::Waiting;

    draw_status_line(display, status_line, "waiting for touch...", style);

    loop {
        delay.delay(Duration::from_millis(40));

        match touch.read_report() {
            Ok(report) => {
                if let Some(event) = report.events.into_iter().flatten().next() {
                    if let Some(old) = last_point
                        && old != event.point
                    {
                        erase_marker(display, old);
                    }

                    let next_state = TouchDisplayState::Touch {
                        phase: event.phase,
                        id: event.id,
                        x: event.point.x,
                        y: event.point.y,
                    };

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

                    if next_state != last_state {
                        draw_status_line(display, status_line, &line, accent);
                        last_state = next_state;
                    }

                    if matches!(event.phase, TouchPhase::Up) {
                        erase_marker(display, event.point);
                        last_point = None;
                    } else {
                        draw_marker(display, event.point);
                        last_point = Some(event.point);
                    }
                } else if last_state != TouchDisplayState::Waiting {
                    if let Some(old) = last_point {
                        erase_marker(display, old);
                        last_point = None;
                    }
                    draw_status_line(display, status_line, "waiting for touch...", style);
                    last_state = TouchDisplayState::Waiting;
                }
            }
            Err(_) => {
                if last_state != TouchDisplayState::ReadFailed {
                    if let Some(old) = last_point {
                        erase_marker(display, old);
                        last_point = None;
                    }
                    draw_status_line(display, status_line, "touch read failed", error_style);
                    last_state = TouchDisplayState::ReadFailed;
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TouchDisplayState {
    Waiting,
    ReadFailed,
    Touch {
        phase: TouchPhase,
        id: u8,
        x: i32,
        y: i32,
    },
}

fn draw_status_line<T>(
    display: &mut T,
    area: Rectangle,
    text: &str,
    style: MonoTextStyle<'_, Rgb565>,
) where
    T: DrawTarget<Color = Rgb565>,
{
    area.into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();
    Text::new(text, area.top_left + Point::new(0, 12), style)
        .draw(display)
        .ok();
}

fn draw_marker<T>(display: &mut T, point: Point)
where
    T: DrawTarget<Color = Rgb565>,
{
    Circle::new(point - Point::new(5, 5), 10)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
        .draw(display)
        .ok();
}

fn erase_marker<T>(display: &mut T, point: Point)
where
    T: DrawTarget<Color = Rgb565>,
{
    Circle::new(point - Point::new(6, 6), 12)
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();
}
