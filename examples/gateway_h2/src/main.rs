#![no_std]
#![no_main]

use core_s3::gateway_h2::{
    GatewayH2,
    matter::{MatterOverThreadConfig, MatterServerConfig, ThreadDatasetConfig},
};
use esp_backtrace as _;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    let gateway = GatewayH2::GROVE_UART;

    let matter = MatterServerConfig::new(0xFFF1, 0x8001, 3840, 20_202_021, "CoreS3 Gateway H2");
    let thread = ThreadDatasetConfig::new(
        "core-s3-thread",
        0x1234,
        [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        15,
        [0xAA; 16],
    );
    let setup = MatterOverThreadConfig::new(gateway, matter, thread);

    esp_println::println!(
        "Gateway H2 UART tx=GPIO{} rx=GPIO{} matter-port={}",
        setup.gateway.host_uart.tx.0,
        setup.gateway.host_uart.rx.0,
        setup.matter.port
    );

    loop {
        core::hint::spin_loop();
    }
}
