#![no_std]
#![no_main]

use core_s3::{CoreS3, bsp::CoreS3DisplayResources, display::DirtySprite};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use esp_hal::time::Duration;

esp_bootloader_esp_idf::esp_app_desc!();

const SPRITE_WIDTH: u16 = 128;
const SPRITE_HEIGHT: u16 = 64;
const BLOCK_SIZE: u32 = 14;

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    type AnimationSprite = DirtySprite<Rgb565, SPRITE_WIDTH, SPRITE_HEIGHT, { 128 * 64 }, 8>;
    let mut sprite = AnimationSprite::new(Rgb565::BLACK).expect("valid framebuffer dimensions");

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
            "Animated sprite with partial LCD blits",
            Rgb565::YELLOW,
        )
        .expect("draw validation screen");

    Rectangle::new(Point::new(18, 78), Size::new(284, 122))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(&mut parts.display)
        .expect("clear example area");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    Text::new("Off-screen sprite: 128x64", Point::new(28, 94), style)
        .draw(&mut parts.display)
        .expect("draw sprite label");
    Text::new(
        "Only old/new block rects are flushed",
        Point::new(28, 110),
        accent,
    )
    .draw(&mut parts.display)
    .expect("draw dirty label");

    let sprite_origin = Point::new(96, 128);
    Rectangle::new(sprite_origin - Point::new(2, 2), Size::new(132, 68))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::YELLOW, 1))
        .draw(&mut parts.display)
        .expect("draw sprite frame");

    draw_background(&mut sprite);
    draw_block(&mut sprite, Point::new(0, 24), Rgb565::YELLOW);
    sprite
        .flush_dirty_at(&mut parts.display, sprite_origin)
        .expect("draw initial sprite");

    let delay = esp_hal::delay::Delay::new();
    let mut x = 0i32;
    let mut dx = 2i32;
    let y = 24i32;

    loop {
        delay.delay(Duration::from_millis(33));

        let old = Point::new(x, y);
        x += dx;
        let max_x = i32::from(SPRITE_WIDTH) - BLOCK_SIZE as i32;
        if x <= 0 || x >= max_x {
            x = x.clamp(0, max_x);
            dx = -dx;
        }
        let new = Point::new(x, y);

        erase_block(&mut sprite, old);
        draw_block(&mut sprite, new, Rgb565::YELLOW);
        sprite
            .flush_dirty_at(&mut parts.display, sprite_origin)
            .expect("flush dirty animation regions");
    }
}

fn draw_background<T>(sprite: &mut T)
where
    T: DrawTarget<Color = Rgb565>,
{
    sprite.clear(Rgb565::BLACK).ok();

    for y in (0..SPRITE_HEIGHT).step_by(8) {
        Rectangle::new(
            Point::new(0, i32::from(y)),
            Size::new(u32::from(SPRITE_WIDTH), 1),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::new(2, 4, 8)))
        .draw(sprite)
        .ok();
    }
}

fn erase_block<T>(sprite: &mut T, top_left: Point)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(top_left, Size::new(BLOCK_SIZE, BLOCK_SIZE))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(sprite)
        .ok();

    let grid_y = top_left.y + 8 - top_left.y.rem_euclid(8);
    if grid_y < top_left.y + BLOCK_SIZE as i32 {
        Rectangle::new(Point::new(top_left.x, grid_y), Size::new(BLOCK_SIZE, 1))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(2, 4, 8)))
            .draw(sprite)
            .ok();
    }
}

fn draw_block<T>(sprite: &mut T, top_left: Point, color: Rgb565)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(top_left, Size::new(BLOCK_SIZE, BLOCK_SIZE))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(sprite)
        .ok();
    Rectangle::new(top_left + Point::new(3, 3), Size::new(8, 8))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
        .draw(sprite)
        .ok();
}
