#![no_std]
#![no_main]

use core_s3::gateway_h2::GatewayH2;
use esp_backtrace as _;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    let gateway = GatewayH2::GROVE_UART;

    esp_println::println!(
        "Gateway H2 UART tx=GPIO{} rx=GPIO{}",
        gateway.host_uart.tx.0,
        gateway.host_uart.rx.0
    );

    loop {
        core::hint::spin_loop();
    }
}
