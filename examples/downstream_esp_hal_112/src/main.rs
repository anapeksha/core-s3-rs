#![no_std]
#![no_main]

use core_s3::{
    CoreS3,
    bsp::{
        CoreS3DisplayOnSharedSpiResources, CoreS3GatewayH2Resources, CoreS3SdOnSharedSpiResources,
        CoreS3SharedSpiParts, CoreS3SharedSpiResources,
    },
};
use embedded_graphics::prelude::*;
use esp_backtrace as _;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

static SHARED_SPI: StaticCell<CoreS3SharedSpiParts> = StaticCell::new();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let shared_spi = CoreS3::init_shared_spi(CoreS3SharedSpiResources {
        spi2: peripherals.SPI2,
        sclk: peripherals.GPIO36,
        mosi: peripherals.GPIO37,
        miso: peripherals.GPIO35,
    })
    .expect("shared CoreS3 LCD/TF SPI bus");
    let shared_spi = SHARED_SPI.init(shared_spi);

    let mut display_parts = CoreS3::init_display_on_shared_spi(CoreS3DisplayOnSharedSpiResources {
        shared_spi,
        i2c0: peripherals.I2C0,
        i2c_sda: peripherals.GPIO12,
        i2c_scl: peripherals.GPIO11,
        lcd_cs: peripherals.GPIO3,
    })
    .expect("display on shared SPI");

    let sd_parts = CoreS3::init_sd_on_shared_spi(CoreS3SdOnSharedSpiResources {
        shared_spi,
        tf_card_cs: peripherals.GPIO4,
    })
    .expect("TF card SPI device on shared SPI");
    let sd_card = sd_parts.into_sdmmc();
    let _capacity_probe = sd_card.num_bytes();

    let h2_parts = CoreS3::init_gateway_h2_openthread(CoreS3GatewayH2Resources {
        uart1: peripherals.UART1,
        tx: peripherals.GPIO1,
        rx: peripherals.GPIO2,
    })
    .expect("Gateway H2 OpenThread UART parts");

    esp_println::println!(
        "core-s3 downstream validation: display+sd+h2 baud={} max_frame={}",
        h2_parts.baud,
        h2_parts.max_frame_size
    );

    display_parts
        .display
        .clear(embedded_graphics::pixelcolor::Rgb565::BLACK)
        .ok();

    loop {
        core::hint::spin_loop();
    }
}
