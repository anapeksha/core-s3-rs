#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{CoreS3, bsp::CoreS3DisplayResources, display::DirtySprite};
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

    type WidgetSprite = DirtySprite<Rgb565, 96, 48, { 96 * 48 }, 16>;
    let mut sprite = WidgetSprite::new(Rgb565::BLACK).expect("valid framebuffer dimensions");

    Rectangle::new(Point::new(4, 4), Size::new(40, 16))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        .draw(&mut sprite)
        .unwrap();
    Rectangle::new(Point::new(52, 10), Size::new(24, 28))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
        .draw(&mut sprite)
        .unwrap();
    Rectangle::new(Point::new(8, 30), Size::new(84, 10))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLUE))
        .draw(&mut sprite)
        .unwrap();

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

    parts
        .display
        .draw_validation_screen(
            "DIRTY REGIONS",
            "Only changed sprite areas are tracked",
            Rgb565::YELLOW,
        )
        .expect("draw validation screen");

    Rectangle::new(Point::new(18, 78), Size::new(284, 118))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(&mut parts.display)
        .expect("clear example area");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    Text::new("Sprite framebuffer: 96x48", Point::new(28, 92), style)
        .draw(&mut parts.display)
        .expect("draw sprite label");
    Text::new("Yellow outlines = dirty rects", Point::new(28, 108), accent)
        .draw(&mut parts.display)
        .expect("draw dirty label");

    let sprite_origin = Point::new(28, 124);
    let sprite_ref = &sprite;
    parts
        .display
        .blit_pixels(
            &Rectangle::new(sprite_origin, Size::new(96, 48)),
            (0..48).flat_map(move |y| {
                (0..96).map(move |x| sprite_ref.pixel(Point::new(x, y)).unwrap_or(Rgb565::BLACK))
            }),
        )
        .expect("blit sprite");

    let mut count = 0u32;
    for region in sprite.dirty_regions() {
        count += 1;
        let outline = Rectangle::new(
            sprite_origin + region.top_left,
            Size::new(region.size.width, region.size.height),
        );
        outline
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::YELLOW, 2))
            .draw(&mut parts.display)
            .expect("draw dirty outline");
        esp_println::println!(
            "dirty: x={} y={} w={} h={}",
            region.top_left.x,
            region.top_left.y,
            region.size.width,
            region.size.height
        );
    }

    let mut summary: String<48> = String::new();
    write!(&mut summary, "Tracked dirty regions: {count}").unwrap();
    Text::new(&summary, Point::new(142, 146), accent)
        .draw(&mut parts.display)
        .expect("draw summary");

    loop {
        core::hint::spin_loop();
    }
}
