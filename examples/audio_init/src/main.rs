#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    audio::{AudioSource, Aw88298, Es7210, MicrophoneConfig, SpeakerConfig, Tone},
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
use esp_hal::{
    dma_buffers,
    i2s::master::{Channels, Config, DataFormat, I2s},
    time::Rate,
};
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
    let aw_volume = speaker.read_register16(0x0C).ok();
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

    let (i2s_ok, played_tone) = play_audio_smoke_test(
        peripherals.I2S1,
        peripherals.DMA_CH1,
        peripherals.GPIO34,
        peripherals.GPIO33,
        peripherals.GPIO13,
    );

    draw_audio_status(
        display,
        AudioStatus {
            es_probe_before,
            es_init,
            es_gain,
            aw_probe_before,
            aw_init,
            aw_volume,
            i2s_ok,
            played_tone,
        },
    );

    loop {
        core::hint::spin_loop();
    }
}

#[derive(Clone, Copy)]
struct AudioStatus {
    es_probe_before: Option<u8>,
    es_init: bool,
    es_gain: Option<u8>,
    aw_probe_before: Option<u16>,
    aw_init: bool,
    aw_volume: Option<u16>,
    i2s_ok: bool,
    played_tone: bool,
}

fn draw_audio_status<T>(display: &mut T, status: AudioStatus)
where
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
        HexByte(status.es_probe_before),
        if status.es_init { "OK" } else { "FAIL" }
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(24, 86),
        if status.es_init { ok } else { error },
    )
    .draw(display)
    .ok();

    line.clear();
    write!(
        &mut line,
        "ES7210 gain reg 0x22: {}",
        HexByte(status.es_gain)
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(24, 104),
        if status.es_gain.is_some() { ok } else { warn },
    )
    .draw(display)
    .ok();

    line.clear();
    write!(
        &mut line,
        "AW88298 0x36 probe:{} init:{}",
        HexWord(status.aw_probe_before),
        if status.aw_init { "OK" } else { "FAIL" }
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(24, 126),
        if status.aw_init { ok } else { error },
    )
    .draw(display)
    .ok();

    line.clear();
    write!(
        &mut line,
        "AW88298 volume reg 0x0C: {}",
        HexWord(status.aw_volume)
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(24, 144),
        if status.aw_volume.is_some() { ok } else { warn },
    )
    .draw(display)
    .ok();

    Text::new("I2S1 pins: BCK34 WS33 DO13", Point::new(24, 166), style)
        .draw(display)
        .ok();

    line.clear();
    write!(
        &mut line,
        "I2S TX:{} tone:{}",
        if status.i2s_ok { "OK" } else { "FAIL" },
        if status.played_tone { "OK" } else { "FAIL" }
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(24, 184),
        if status.i2s_ok && status.played_tone {
            ok
        } else {
            error
        },
    )
    .draw(display)
    .ok();

    Text::new("Expected: audible 440Hz beep", Point::new(24, 204), warn)
        .draw(display)
        .ok();
}

fn play_audio_smoke_test(
    i2s1: esp_hal::peripherals::I2S1<'_>,
    dma_ch1: esp_hal::peripherals::DMA_CH1<'_>,
    bclk: esp_hal::peripherals::GPIO34<'_>,
    ws: esp_hal::peripherals::GPIO33<'_>,
    dout: esp_hal::peripherals::GPIO13<'_>,
) -> (bool, bool) {
    let (_, _, tx_buffer, tx_descriptors) = dma_buffers!(0, 4092);
    let Ok(i2s) = I2s::new(
        i2s1,
        dma_ch1,
        Config::new_tdm_philips()
            .with_sample_rate(Rate::from_hz(16_000))
            .with_data_format(DataFormat::Data16Channel16)
            .with_channels(Channels::STEREO),
    ) else {
        return (false, false);
    };

    let mut i2s_tx = i2s
        .i2s_tx
        .with_bclk(bclk)
        .with_ws(ws)
        .with_dout(dout)
        .build(tx_descriptors);

    let mut tone = Tone::new(440, 1_000, 16_000, 10_000);
    let played_tone = play_source(&mut i2s_tx, tx_buffer, &mut tone);

    (true, played_tone)
}

fn play_source<T>(
    i2s_tx: &mut esp_hal::i2s::master::I2sTx<'_, esp_hal::Blocking>,
    tx_buffer: &mut [u8],
    source: &mut T,
) -> bool
where
    T: AudioSource,
{
    let mut any_audio = false;
    loop {
        let has_more = fill_stereo_i16_buffer(tx_buffer, source);
        if !has_more && any_audio {
            break;
        }
        any_audio |= has_more;

        let Ok(transfer) = i2s_tx.write_dma(&tx_buffer) else {
            return false;
        };
        if transfer.wait().is_err() {
            return false;
        }

        if !has_more {
            break;
        }
    }
    any_audio
}

fn fill_stereo_i16_buffer<T>(buffer: &mut [u8], source: &mut T) -> bool
where
    T: AudioSource,
{
    let mut emitted = false;
    for frame in buffer.chunks_exact_mut(4) {
        let sample = source.next_sample().unwrap_or(0);
        emitted |= sample != 0;
        let bytes = sample.to_le_bytes();
        frame[0] = bytes[0];
        frame[1] = bytes[1];
        frame[2] = bytes[0];
        frame[3] = bytes[1];
    }
    emitted
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
