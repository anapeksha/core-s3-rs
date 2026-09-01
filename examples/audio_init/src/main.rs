#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    audio::{Aw88298, Es7210, MicrophoneConfig, SpeakerConfig},
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

    let mut mic = Es7210::new(parts.internal_i2c);
    let es_probe_before = mic.read_register(0x00).ok();
    let es_init = mic.init(MicrophoneConfig::DEFAULT).is_ok();
    let es_gain = mic.read_register(0x22).ok();
    let i2c = mic.release();

    let mut speaker = Aw88298::new(i2c);
    let aw_probe_before = speaker.read_register16(0x00).ok();
    let aw_init = speaker.init(SpeakerConfig::DEFAULT).is_ok();
    let aw_volume = speaker.read_register16(0x04).ok();
    let _i2c = speaker.release();

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 audio init demo",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "AUDIO I2C CONFIG",
        top_left: Point::new(24, 56),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");

    draw_audio_status(
        display,
        es_probe_before,
        es_init,
        es_gain,
        aw_probe_before,
        aw_init,
        aw_volume,
    );

    loop {
        core::hint::spin_loop();
    }
}

fn draw_audio_status<T>(
    display: &mut T,
    es_probe_before: Option<u8>,
    es_init: bool,
    es_gain: Option<u8>,
    aw_probe_before: Option<u16>,
    aw_init: bool,
    aw_volume: Option<u16>,
) where
    T: DrawTarget<Color = Rgb565>,
{
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    let mut line: String<64> = String::new();
    write!(
        &mut line,
        "ES7210 0x40 probe:{} init:{}",
        HexByte(es_probe_before),
        if es_init { "OK" } else { "FAIL" }
    )
    .unwrap();
    Text::new(&line, Point::new(24, 86), if es_init { ok } else { error })
        .draw(display)
        .ok();

    line.clear();
    write!(&mut line, "ES7210 gain reg 0x22: {}", HexByte(es_gain)).unwrap();
    Text::new(
        &line,
        Point::new(24, 104),
        if es_gain.is_some() { ok } else { warn },
    )
    .draw(display)
    .ok();

    line.clear();
    write!(
        &mut line,
        "AW88298 0x36 probe:{} init:{}",
        HexWord(aw_probe_before),
        if aw_init { "OK" } else { "FAIL" }
    )
    .unwrap();
    Text::new(&line, Point::new(24, 126), if aw_init { ok } else { error })
        .draw(display)
        .ok();

    line.clear();
    write!(&mut line, "AW88298 volume reg 0x04: {}", HexWord(aw_volume)).unwrap();
    Text::new(
        &line,
        Point::new(24, 144),
        if aw_volume.is_some() { ok } else { warn },
    )
    .draw(display)
    .ok();

    Text::new(
        "I2S pins: BCK34 WS33 DO13 DI14 MCLK0",
        Point::new(24, 170),
        style,
    )
    .draw(display)
    .ok();
    Text::new(
        "Format: 16 kHz, 16-bit, std I2S",
        Point::new(24, 188),
        style,
    )
    .draw(display)
    .ok();
    Text::new(
        "DMA capture/playback is app-owned",
        Point::new(24, 206),
        warn,
    )
    .draw(display)
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

struct HexWord(Option<u16>);

impl core::fmt::Display for HexWord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(value) => write!(f, "0x{value:04X}"),
            None => f.write_str("--"),
        }
    }
}
