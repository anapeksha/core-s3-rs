#![no_std]
#![no_main]

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    ui::{Label, ProgressBar, Theme},
};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
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

    parts.display.clear(Rgb565::BLACK).expect("clear");
    Label {
        text: "CoreS3 widgets",
        top_left: Point::new(20, 30),
        color: Rgb565::CYAN,
    }
    .draw(&mut parts.display)
    .expect("label");
    ProgressBar {
        bounds: Rectangle::new(Point::new(20, 60), Size::new(180, 18)),
        value: 72,
    }
    .draw(&mut parts.display, Theme::DARK)
    .expect("progress");

    loop {
        core::hint::spin_loop();
    }
}
