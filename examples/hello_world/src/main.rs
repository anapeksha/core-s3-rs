#![no_std]
#![no_main]

use core_s3::CoreS3;
use esp_backtrace as _;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());
    let board = CoreS3::board();

    esp_println::println!("{} on {}", board.name, board.chip);
    esp_println::println!("display: {}x{}", board.display.width, board.display.height);

    loop {
        core::hint::spin_loop();
    }
}
