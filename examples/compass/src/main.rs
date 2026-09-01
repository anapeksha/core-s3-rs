#![no_std]
#![no_main]

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    ui::{Label, ProgressBar, StatusBar, Theme},
};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use esp_backtrace as _;

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

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 v0.3 hardware smoke",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 174))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "COMPASS",
        top_left: Point::new(24, 62),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");
    Label {
        text: "BMM150 magnetometer",
        top_left: Point::new(24, 88),
        color: Rgb565::WHITE,
    }
    .draw(display)
    .expect("line1");
    Label {
        text: "via BMI270 aux bus",
        top_left: Point::new(24, 106),
        color: Rgb565::CYAN,
    }
    .draw(display)
    .expect("line2");
    Label {
        text: "Display-rendered verification",
        top_left: Point::new(24, 132),
        color: Rgb565::WHITE,
    }
    .draw(display)
    .expect("line3");
    ProgressBar {
        bounds: Rectangle::new(Point::new(24, 162), Size::new(190, 18)),
        value: 100,
    }
    .draw(display, theme)
    .expect("progress");

    loop {
        core::hint::spin_loop();
    }
}
