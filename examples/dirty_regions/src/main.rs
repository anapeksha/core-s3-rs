#![no_std]
#![no_main]

use core_s3::display::DirtySprite;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use esp_backtrace as _;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let _peripherals = esp_hal::init(esp_hal::Config::default());

    type WidgetSprite = DirtySprite<Rgb565, 96, 48, { 96 * 48 }, 16>;
    let mut sprite = WidgetSprite::new(Rgb565::BLACK).expect("valid framebuffer dimensions");

    Rectangle::new(Point::new(4, 4), Size::new(40, 16))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        .draw(&mut sprite)
        .unwrap();

    for region in sprite.dirty_regions() {
        esp_println::println!(
            "dirty: x={} y={} w={} h={}",
            region.top_left.x,
            region.top_left.y,
            region.size.width,
            region.size.height
        );
    }

    loop {
        core::hint::spin_loop();
    }
}
