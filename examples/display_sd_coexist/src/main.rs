#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::{
        CoreS3DisplayOnPoweredSharedSpiResources, CoreS3InternalI2cResources,
        CoreS3SdOnSharedSpiResources, CoreS3SharedSpiParts, CoreS3SharedSpiResources,
    },
    display::DirtySprite,
    ui::{StatusBar, Theme},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_sdmmc::{Block, BlockDevice, BlockIdx};
use esp_backtrace as _;
use heapless::String;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const ITERATIONS: u32 = 250;
/// Raw validation block used by this hardware test.
///
/// WARNING: this example temporarily overwrites this sector. It restores the
/// sector after each verification pass, but it should still be run only with a
/// disposable/test microSD card or a card image you can recreate.
const TEST_BLOCK: BlockIdx = BlockIdx(4096);
const SPRITE_W: u16 = 176;
const SPRITE_H: u16 = 72;
const SPRITE_ORIGIN: Point = Point::new(72, 92);
const INDICATOR_AREA: Rectangle = Rectangle::new(Point::new(252, 156), Size::new(16, 16));

type StatusSprite = DirtySprite<Rgb565, SPRITE_W, SPRITE_H, { 176 * 72 }, 8>;

static SHARED_SPI: StaticCell<CoreS3SharedSpiParts> = StaticCell::new();
static STATUS_SPRITE: StaticCell<StatusSprite> = StaticCell::new();

#[esp_hal::main]
fn main() -> ! {
    esp_println::println!("CoreS3 display/SD cooperative smoke boot");
    esp_println::println!(
        "WARNING: raw SD validation temporarily overwrites and restores block {}",
        TEST_BLOCK.0
    );

    let peripherals = esp_hal::init(esp_hal::Config::default());
    let sprite = STATUS_SPRITE.init(StatusSprite::new(Rgb565::BLACK).expect("status sprite"));

    let shared_spi = CoreS3::init_shared_spi(CoreS3SharedSpiResources {
        spi2: peripherals.SPI2,
        sclk: peripherals.GPIO36,
        mosi: peripherals.GPIO37,
        miso: peripherals.GPIO35,
    })
    .expect("shared LCD/TF SPI");
    let shared_spi = SHARED_SPI.init(shared_spi);

    let mut internal_i2c = CoreS3::init_internal_i2c(CoreS3InternalI2cResources {
        i2c0: peripherals.I2C0,
        i2c_sda: peripherals.GPIO12,
        i2c_scl: peripherals.GPIO11,
    })
    .expect("internal I2C");
    CoreS3::init_core_s3_power(&mut internal_i2c).expect("CoreS3 power rails");
    CoreS3::power_cycle_tf_card_rail(&mut internal_i2c).expect("TF-card ALDO4 power-cycle");

    let mut sd_parts = CoreS3::init_sd_on_shared_spi(CoreS3SdOnSharedSpiResources {
        shared_spi,
        tf_card_cs: peripherals.GPIO4,
    })
    .expect("SD SPI device");
    sd_parts
        .spi_device
        .prepare_for_card_acquire()
        .expect("SD acquire prep");
    let sd = sd_parts.into_sdmmc();
    let blocks = sd.num_blocks().expect("SD num_blocks");
    esp_println::println!("CoreS3 SD acquired: blocks={}", blocks.0);

    let mut display_parts =
        CoreS3::init_display_on_powered_shared_spi(CoreS3DisplayOnPoweredSharedSpiResources {
            shared_spi,
            internal_i2c,
            lcd_cs: peripherals.GPIO3,
        })
        .expect("display on powered shared SPI");
    let display = &mut display_parts.display;
    draw_static_display(display).expect("static display");
    draw_status(sprite, display, 0, 0, 0).expect("initial status");
    esp_println::println!("CoreS3 LCD initialized and drawn");

    let mut original = Block::new();
    let mut pattern = Block::new();
    let mut verify = Block::new();
    let mut reads = 0_u32;
    let mut writes = 0_u32;
    let mut errors = 0_u32;

    for iteration in 1..=ITERATIONS {
        draw_batched_indicator(display, iteration, false).expect("pre-SD LCD update");

        if let Err(err) = sd.read(core::slice::from_mut(&mut original), TEST_BLOCK, "preserve") {
            errors += 1;
            esp_println::println!(
                "CoreS3 SD read-before-write failed: iter={}, err={:?}",
                iteration,
                err
            );
            draw_status(sprite, display, writes, reads, errors).ok();
            break;
        }
        reads += 1;
        esp_println::println!("CoreS3 SD read-before-write OK: iter={}", iteration);

        fill_pattern(&mut pattern, iteration);
        if let Err(err) = sd.write(core::slice::from_ref(&pattern), TEST_BLOCK) {
            errors += 1;
            esp_println::println!("CoreS3 SD write failed: iter={}, err={:?}", iteration, err);
            draw_status(sprite, display, writes, reads, errors).ok();
            break;
        }
        writes += 1;

        if let Err(err) = sd.read(core::slice::from_mut(&mut verify), TEST_BLOCK, "verify") {
            errors += 1;
            esp_println::println!(
                "CoreS3 SD verify read failed: iter={}, err={:?}",
                iteration,
                err
            );
            draw_status(sprite, display, writes, reads, errors).ok();
            break;
        }
        reads += 1;
        if verify.contents != pattern.contents {
            errors += 1;
            esp_println::println!("CoreS3 SD verify mismatch: iter={}", iteration);
            draw_status(sprite, display, writes, reads, errors).ok();
            break;
        }

        if let Err(err) = sd.write(core::slice::from_ref(&original), TEST_BLOCK) {
            errors += 1;
            esp_println::println!(
                "CoreS3 SD restore failed: iter={}, err={:?}",
                iteration,
                err
            );
            draw_status(sprite, display, writes, reads, errors).ok();
            break;
        }

        draw_batched_indicator(display, iteration, true).expect("post-SD LCD update");
        draw_status(sprite, display, writes, reads, errors).expect("status update");
        esp_println::println!("CoreS3 SD/LCD cooperative PASS: iter={}", iteration);
    }

    esp_println::println!(
        "CoreS3 SD/LCD cooperative COMPLETE: writes={}, reads={}, errors={}",
        writes,
        reads,
        errors
    );

    loop {
        core::hint::spin_loop();
    }
}

fn draw_static_display<D>(display: &mut D) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK)?;
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 display + SD coexist",
    }
    .draw(display, Theme::DARK)?;
    Rectangle::new(Point::new(28, 48), Size::new(264, 148))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(display)?;
    let title = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    Text::new("DISPLAY + SD TEST", Point::new(88, 72), title).draw(display)?;
    Ok(())
}

fn draw_status<D>(
    sprite: &mut StatusSprite,
    display: &mut D,
    writes: u32,
    reads: u32,
    errors: u32,
) -> Result<(), D::Error>
where
    D: DrawTarget<Color = Rgb565>,
{
    sprite.clear(Rgb565::BLACK);
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);
    let err = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);
    let mut line: String<48> = String::new();

    write!(&mut line, "Writes: {writes:06}").unwrap();
    Text::new(&line, Point::new(0, 14), style).draw(sprite).ok();
    line.clear();
    write!(&mut line, "Reads:  {reads:06}").unwrap();
    Text::new(&line, Point::new(0, 34), style).draw(sprite).ok();
    line.clear();
    write!(&mut line, "Errors: {errors}").unwrap();
    Text::new(&line, Point::new(0, 54), if errors == 0 { ok } else { err })
        .draw(sprite)
        .ok();

    sprite.flush_dirty_at(display, SPRITE_ORIGIN)
}

fn draw_batched_indicator(
    display: &mut core_s3::bsp::CoreS3SharedDisplay,
    iteration: u32,
    verified: bool,
) -> Result<
    (),
    core_s3::display::DisplayError<core_s3::bsp::CoreS3SharedSdSpiError, core::convert::Infallible>,
> {
    let mut bytes = [0u8; 16 * 16 * 2];
    let primary = if verified {
        Rgb565::GREEN.into_storage()
    } else {
        Rgb565::YELLOW.into_storage()
    };
    let accent = if iteration & 1 == 0 {
        Rgb565::CYAN.into_storage()
    } else {
        Rgb565::WHITE.into_storage()
    };
    for (index, pixel) in bytes.chunks_exact_mut(2).enumerate() {
        let x = index % 16;
        let y = index / 16;
        let raw = if x == y || x + y == 15 {
            accent
        } else {
            primary
        };
        pixel.copy_from_slice(&raw.to_be_bytes());
    }
    display.blit_rgb565_be(&INDICATOR_AREA, &bytes)
}

fn fill_pattern(block: &mut Block, iteration: u32) {
    let magic = *b"CS3SDMUX";
    block.contents[..magic.len()].copy_from_slice(&magic);
    block.contents[8..12].copy_from_slice(&iteration.to_le_bytes());
    for (index, byte) in block.contents[12..].iter_mut().enumerate() {
        *byte = (iteration as u8).wrapping_add(index as u8).rotate_left(1);
    }
}
