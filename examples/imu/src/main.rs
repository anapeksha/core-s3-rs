#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    motion::{Bmi270, Bmi270Config, Vector3},
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

    let mut imu = Bmi270::new(parts.internal_i2c);
    let chip_id = imu.chip_id().ok();
    let imu_init = imu.init(Bmi270Config::DEFAULT).is_ok();
    let delay = esp_hal::delay::Delay::new();

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 BMI270 IMU demo",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "IMU LIVE READ",
        top_left: Point::new(24, 62),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");

    let mut chip_line: String<48> = String::new();
    match chip_id {
        Some(id) => write!(&mut chip_line, "BMI270 chip id: 0x{:02X}", id).unwrap(),
        None => chip_line.push_str("BMI270 chip id: read failed").unwrap(),
    }
    Label {
        text: &chip_line,
        top_left: Point::new(24, 88),
        color: if chip_id == Some(0x24) {
            Rgb565::CYAN
        } else {
            Rgb565::YELLOW
        },
    }
    .draw(display)
    .expect("chip id");
    Label {
        text: if imu_init {
            "BMI270 init: OK"
        } else {
            "BMI270 init: FAILED"
        },
        top_left: Point::new(24, 106),
        color: if imu_init { Rgb565::CYAN } else { Rgb565::RED },
    }
    .draw(display)
    .expect("init");

    let mut last_accel = None;
    let mut last_gyro = None;

    loop {
        delay.delay(Duration::from_millis(200));
        let accel = imu.acceleration_raw().ok();
        let gyro = imu.gyroscope_raw().ok();
        if accel != last_accel || gyro != last_gyro {
            draw_imu(display, accel, gyro);
            last_accel = accel;
            last_gyro = gyro;
        }
    }
}

fn draw_imu<T>(display: &mut T, accel: Option<Vector3>, gyro: Option<Vector3>)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(20, 126), Size::new(274, 76))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    if let Some(a) = accel {
        let mut line: String<64> = String::new();
        write!(&mut line, "acc raw x={:>6} y={:>6} z={:>6}", a.x, a.y, a.z).unwrap();
        Text::new(&line, Point::new(24, 144), accent)
            .draw(display)
            .ok();
    } else {
        Text::new("acc read failed", Point::new(24, 144), error)
            .draw(display)
            .ok();
    }

    if let Some(g) = gyro {
        let mut line: String<64> = String::new();
        write!(&mut line, "gyr raw x={:>6} y={:>6} z={:>6}", g.x, g.y, g.z).unwrap();
        Text::new(&line, Point::new(24, 164), style)
            .draw(display)
            .ok();
    } else {
        Text::new("gyro read failed", Point::new(24, 164), error)
            .draw(display)
            .ok();
    }

    Text::new(
        "Move/tilt CoreS3: values should change",
        Point::new(24, 190),
        style,
    )
    .draw(display)
    .ok();
}
