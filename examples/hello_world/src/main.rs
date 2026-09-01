#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
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
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let board = CoreS3::board();

    esp_println::println!("{} on {}", board.name, board.chip);
    esp_println::println!("display: {}x{}", board.display.width, board.display.height);

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
    .expect("initialize CoreS3 display");

    parts.display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 hello_world",
    }
    .draw(&mut parts.display, Theme::DARK)
    .expect("status bar");
    Rectangle::new(Point::new(12, 36), Size::new(296, 168))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(&mut parts.display)
        .expect("border");
    Label {
        text: "HELLO WORLD",
        top_left: Point::new(34, 64),
        color: Rgb565::CYAN,
    }
    .draw(&mut parts.display)
    .expect("title");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);

    Text::new("Crate-owned CoreS3 init:", Point::new(34, 98), accent)
        .draw(&mut parts.display)
        .expect("draw title");
    Text::new("AXP2101 power + AW9523 reset", Point::new(34, 116), style)
        .draw(&mut parts.display)
        .expect("draw power line");
    Text::new("ILI9342 LCD over ESP-HAL SPI", Point::new(34, 132), style)
        .draw(&mut parts.display)
        .expect("draw lcd line");

    let mut dimensions: String<32> = String::new();
    write!(
        &mut dimensions,
        "Display: {}x{} RGB565",
        board.display.width, board.display.height
    )
    .unwrap();
    Text::new(&dimensions, Point::new(34, 150), style)
        .draw(&mut parts.display)
        .expect("draw dimensions");

    loop {
        core::hint::spin_loop();
    }
}
