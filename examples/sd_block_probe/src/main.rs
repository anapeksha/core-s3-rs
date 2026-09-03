#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    aw9523b::{Aw9523b, Port},
    bsp::{
        CoreS3DisplayOnPoweredSharedSpiResources, CoreS3InternalI2cResources,
        CoreS3SdOnSharedSpiResources, CoreS3SharedSpiParts, CoreS3SharedSpiResources,
    },
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
use heapless::String;
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const SPRITE_W: u16 = 288;
const SPRITE_H: u16 = 112;
const SPRITE_ORIGIN: Point = Point::new(16, 104);

type ProbeSprite = DirtySprite<Rgb565, SPRITE_W, SPRITE_H, { 288 * 112 }, 8>;

static SHARED_SPI: StaticCell<CoreS3SharedSpiParts> = StaticCell::new();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut sprite = ProbeSprite::new(Rgb565::BLACK).expect("valid SD probe sprite");

    let shared_spi = CoreS3::init_shared_spi(CoreS3SharedSpiResources {
        spi2: peripherals.SPI2,
        sclk: peripherals.GPIO36,
        mosi: peripherals.GPIO37,
        miso: peripherals.GPIO35,
    })
    .expect("shared LCD/TF SPI");
    let shared_spi = SHARED_SPI.init(shared_spi);

    let mut sd_parts = CoreS3::init_sd_on_shared_spi(CoreS3SdOnSharedSpiResources {
        shared_spi,
        tf_card_cs: peripherals.GPIO4,
    })
    .expect("SD SPI device");

    let mut internal_i2c = CoreS3::init_internal_i2c(CoreS3InternalI2cResources {
        i2c0: peripherals.I2C0,
        i2c_sda: peripherals.GPIO12,
        i2c_scl: peripherals.GPIO11,
    })
    .expect("internal I2C");
    CoreS3::init_core_s3_power(&mut internal_i2c).expect("CoreS3 power rails");
    CoreS3::power_cycle_tf_card_rail(&mut internal_i2c).expect("TF card rail power-cycle");
    sd_parts
        .spi_device
        .prepare_for_card_acquire()
        .expect("SD acquire prep");

    let sd_card = sd_parts.into_sdmmc();
    let capacity = sd_card.num_bytes();

    let mut aw9523 = Aw9523b::new(internal_i2c);
    let p0 = aw9523.read_input_port(Port::P0).ok();
    let present = p0.map(sd::core_s3_card_present_from_aw9523_p0);
    let internal_i2c = aw9523.release();

    esp_println::println!("AW9523 P0 input: {}", HexByte(p0));
    match present {
        Some(true) => esp_println::println!("Card detect: INSERTED"),
        Some(false) => esp_println::println!("Card detect: NOT INSERTED"),
        None => esp_println::println!("Card detect: READ FAILED"),
    }
    match capacity {
        Ok(bytes) => esp_println::println!("SD num_bytes(): {} bytes", bytes),
        Err(err) => esp_println::println!("SD block probe failed: {:?}", err),
    }

    let mut display_parts =
        CoreS3::init_display_on_powered_shared_spi(CoreS3DisplayOnPoweredSharedSpiResources {
            shared_spi,
            internal_i2c,
            lcd_cs: peripherals.GPIO3,
        })
        .expect("display on powered shared SPI");

    let display = &mut display_parts.display;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 SD block probe",
    }
    .draw(display, Theme::DARK)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "TF CARD BLOCK PROBE",
        top_left: Point::new(24, 56),
        color: Rgb565::CYAN,
    }
    .draw(display)
    .expect("title");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Text::new(
        "Shared SPI: SCLK36 MOSI37 MISO/DC35",
        Point::new(24, 84),
        style,
    )
    .draw(display)
    .ok();

    draw_probe_status(&mut sprite, p0, present, capacity);
    sprite.flush_dirty_at(display, SPRITE_ORIGIN).ok();
    esp_println::println!("LCD after SD probe: OK");

    loop {
        core::hint::spin_loop();
    }
}

fn draw_probe_status(
    sprite: &mut ProbeSprite,
    p0: Option<u8>,
    present: Option<bool>,
    capacity: Result<u64, embedded_sdmmc::sdcard::Error>,
) {
    sprite.clear(Rgb565::BLACK);
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    let mut line: String<96> = String::new();
    write!(&mut line, "AW9523 P0 input: {}", HexByte(p0)).unwrap();
    Text::new(&line, Point::new(0, 14), style).draw(sprite).ok();

    let (detect_text, detect_style) = match present {
        Some(true) => ("Card detect: INSERTED", ok),
        Some(false) => ("Card detect: NOT INSERTED", warn),
        None => ("Card detect: READ FAILED", error),
    };
    Text::new(detect_text, Point::new(0, 36), detect_style)
        .draw(sprite)
        .ok();

    line.clear();
    match capacity {
        Ok(bytes) => {
            write!(&mut line, "SD num_bytes(): {bytes} bytes").unwrap();
            Text::new(&line, Point::new(0, 62), ok).draw(sprite).ok();
            line.clear();
            write!(&mut line, "Approx capacity: {} MiB", bytes / 1024 / 1024).unwrap();
            Text::new(&line, Point::new(0, 84), ok).draw(sprite).ok();
        }
        Err(err) => {
            write!(&mut line, "SD block probe failed: {err:?}").unwrap();
            Text::new(&line, Point::new(0, 62), error).draw(sprite).ok();
            Text::new(
                "Check inserted FAT/SD card + GPIO35 mux",
                Point::new(0, 84),
                warn,
            )
            .draw(sprite)
            .ok();
        }
    }
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
