#![no_std]
#![no_main]

use core_s3::{
    CoreS3,
    bsp::{CoreS3DisplayResources, CoreS3GatewayH2Resources},
    gateway_h2::transport::{FrameKind, H2Frame},
};
use embedded_graphics::{pixelcolor::Rgb565, prelude::RgbColor};
use esp_backtrace as _;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut display_parts = CoreS3::init_display(CoreS3DisplayResources {
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

    let h2 = CoreS3::init_gateway_h2(CoreS3GatewayH2Resources {
        uart1: peripherals.UART1,
        tx: peripherals.GPIO1,
        rx: peripherals.GPIO2,
    })
    .expect("gateway h2 uart");

    let frame = H2Frame::<32>::new(FrameKind::Request, 1, b"state");
    let _ = frame;
    let _ = h2;

    display_parts
        .display
        .draw_validation_screen("H2 TRANSPORT", "framing + UART initialized", Rgb565::CYAN)
        .expect("draw");
    loop {
        core::hint::spin_loop();
    }
}
