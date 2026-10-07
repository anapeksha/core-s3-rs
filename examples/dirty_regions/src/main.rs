#![no_std]
#![no_main]

use core_s3::{
    CoreS3,
    bsp::{
        CoreS3DisplayOnPoweredSharedSpiResources, CoreS3InternalI2cResources,
        CoreS3SdOnSharedSpiResources, CoreS3SharedSpiParts, CoreS3SharedSpiResources,
    },
    display::DirtySprite,
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use esp_hal::time::Duration;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const SPRITE_WIDTH: u16 = 128;
const SPRITE_HEIGHT: u16 = 64;
const BLOCK_SIZE: u32 = 14;
const COLOR_PATTERN_ORIGIN: Point = Point::new(246, 128);
const COLOR_PATTERN_SIZE: u32 = 32;

static SHARED_SPI: StaticCell<CoreS3SharedSpiParts> = StaticCell::new();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    type AnimationSprite = DirtySprite<Rgb565, SPRITE_WIDTH, SPRITE_HEIGHT, { 128 * 64 }, 2>;
    let mut sprite = AnimationSprite::new(Rgb565::BLACK).expect("valid framebuffer dimensions");

    let shared_spi = SHARED_SPI.init(
        CoreS3::init_shared_spi(CoreS3SharedSpiResources {
            spi2: peripherals.SPI2,
            sclk: peripherals.GPIO36,
            mosi: peripherals.GPIO37,
            miso: peripherals.GPIO35,
        })
        .expect("shared LCD/TF SPI"),
    );
    let mut internal_i2c = CoreS3::init_internal_i2c(CoreS3InternalI2cResources {
        i2c0: peripherals.I2C0,
        i2c_sda: peripherals.GPIO12,
        i2c_scl: peripherals.GPIO11,
    })
    .expect("internal I2C");
    CoreS3::init_core_s3_power(&mut internal_i2c).expect("CoreS3 power");
    let _sd_parts = CoreS3::init_sd_on_shared_spi(CoreS3SdOnSharedSpiResources {
        shared_spi,
        tf_card_cs: peripherals.GPIO4,
    })
    .expect("TF-card CS ownership");
    let mut parts =
        CoreS3::init_display_on_powered_shared_spi(CoreS3DisplayOnPoweredSharedSpiResources {
            shared_spi,
            internal_i2c,
            lcd_cs: peripherals.GPIO3,
        })
        .expect("initialize CoreS3 display");

    parts.display.clear(Rgb565::BLACK).expect("clear");
    Rectangle::new(Point::new(0, 0), Size::new(320, 24))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::YELLOW))
        .draw(&mut parts.display)
        .expect("header");
    let header = MonoTextStyle::new(&FONT_6X10, Rgb565::BLACK);
    Text::new("DIRTY REGIONS", Point::new(8, 16), header)
        .draw(&mut parts.display)
        .expect("header text");

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
        "Overflow merges; BE colors at right",
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

    // Three disjoint invalidations exceed the two-region capacity. The sprite
    // deterministically falls back to one bounding rectangle, never stale pixels.
    sprite
        .invalidate(Rectangle::new(Point::new(0, 0), Size::new(2, 2)))
        .expect("invalidate first region");
    sprite
        .invalidate(Rectangle::new(Point::new(62, 30), Size::new(2, 2)))
        .expect("invalidate second region");
    sprite
        .invalidate(Rectangle::new(Point::new(126, 62), Size::new(2, 2)))
        .expect("overflow-safe invalidation");
    sprite
        .flush_dirty_at(&mut parts.display, sprite_origin)
        .expect("flush merged overflow region");

    let mut color_pattern = [0u8; (COLOR_PATTERN_SIZE * COLOR_PATTERN_SIZE * 2) as usize];
    fill_rgb565_be_pattern(&mut color_pattern);
    parts
        .display
        .blit_rgb565_be(
            &Rectangle::new(
                COLOR_PATTERN_ORIGIN,
                Size::new(COLOR_PATTERN_SIZE, COLOR_PATTERN_SIZE),
            ),
            &color_pattern,
        )
        .expect("zero-copy RGB565 pattern");

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

fn fill_rgb565_be_pattern(bytes: &mut [u8]) {
    let width = COLOR_PATTERN_SIZE as usize;
    for (index, pixel) in bytes.chunks_exact_mut(2).enumerate() {
        let x = index % width;
        let y = index / width;
        let raw = match (x >= width / 2, y >= width / 2) {
            (false, false) => Rgb565::RED.into_storage(),
            (true, false) => Rgb565::GREEN.into_storage(),
            (false, true) => Rgb565::BLUE.into_storage(),
            (true, true) => Rgb565::WHITE.into_storage(),
        };
        pixel.copy_from_slice(&raw.to_be_bytes());
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

    let end_y = top_left.y + BLOCK_SIZE as i32;
    let mut grid_y = top_left.y + (-top_left.y).rem_euclid(8);
    while grid_y < end_y {
        Rectangle::new(Point::new(top_left.x, grid_y), Size::new(BLOCK_SIZE, 1))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(2, 4, 8)))
            .draw(sprite)
            .ok();
        grid_y += 8;
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
