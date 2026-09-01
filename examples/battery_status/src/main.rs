#![no_std]
#![no_main]

use core_s3::{CoreS3, bsp::CoreS3DisplayResources};
use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
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

    parts
        .display
        .draw_validation_screen("BATTERY", "AXP2101 status smoke test", Rgb565::GREEN)
        .expect("draw");
    loop {
        core::hint::spin_loop();
    }
}
