#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    aw9523b::{Aw9523b, Port},
    bsp::CoreS3DisplayResources,
    display::DirtySprite,
    sd,
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
use esp_hal::{delay::Delay, time::Duration};
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

const SPRITE_W: u16 = 272;
const SPRITE_H: u16 = 96;
const SPRITE_ORIGIN: Point = Point::new(24, 112);

type SdSprite = DirtySprite<Rgb565, SPRITE_W, SPRITE_H, { 272 * 96 }, 8>;

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();
    let mut sprite = SdSprite::new(Rgb565::BLACK).expect("valid SD sprite");

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

    let mut aw9523 = Aw9523b::new(parts.internal_i2c);
    let display = &mut parts.display;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 SD card demo",
    }
    .draw(display, Theme::DARK)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "TF CARD DETECT",
        top_left: Point::new(24, 56),
        color: Rgb565::CYAN,
    }
    .draw(display)
    .expect("title");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Text::new(
        "SPI pins: SCLK36 MOSI37 MISO35 CS4",
        Point::new(24, 84),
        style,
    )
    .draw(display)
    .ok();
    Text::new(
        "Card-detect: AW9523B P0.4 active-low",
        Point::new(24, 100),
        style,
    )
    .draw(display)
    .ok();

    let mut tick = 0u32;
    loop {
        let p0 = aw9523.read_input_port(Port::P0).ok();
        let present = p0.map(sd::core_s3_card_present_from_aw9523_p0);
        draw_sd_status(&mut sprite, p0, present, tick);
        sprite.flush_dirty_at(display, SPRITE_ORIGIN).ok();
        tick = tick.wrapping_add(1);
        delay.delay(Duration::from_millis(250));
    }
}

fn draw_sd_status(sprite: &mut SdSprite, p0: Option<u8>, present: Option<bool>, tick: u32) {
    sprite.clear(Rgb565::BLACK);
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    let mut line: String<64> = String::new();
    write!(&mut line, "tick: {tick}").unwrap();
    Text::new(&line, Point::new(0, 12), style).draw(sprite).ok();

    line.clear();
    write!(&mut line, "AW9523 P0 input: {}", HexByte(p0)).unwrap();
    Text::new(
        &line,
        Point::new(0, 32),
        if p0.is_some() { style } else { error },
    )
    .draw(sprite)
    .ok();

    let (status, color) = match present {
        Some(true) => ("SD card: INSERTED", ok),
        Some(false) => ("SD card: NOT INSERTED", warn),
        None => ("SD card: DETECT READ FAILED", error),
    };
    Text::new(status, Point::new(0, 56), color)
        .draw(sprite)
        .ok();

    Text::new(
        "Filesystem init is app-owned on shared SPI",
        Point::new(0, 80),
        warn,
    )
    .draw(sprite)
    .ok();
}

struct HexByte(Option<u8>);

impl core::fmt::Display for HexByte {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(value) => write!(f, "0x{value:02X}"),
            None => f.write_str("--"),
        }
    }
}
