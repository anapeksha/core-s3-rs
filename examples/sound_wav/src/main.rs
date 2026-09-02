#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    audio::{AudioSource, Aw88298, SpeakerConfig, WavPcm16},
    bsp::CoreS3DisplayResources,
    display::DirtySprite,
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
    Blocking,
    delay::Delay,
    dma::DmaTxBuf,
    dma_tx_buffer,
    i2s::master::{Channels, DataFormat, I2s, I2sTx, TdmConfig},
    time::{Duration, Rate},
};
use heapless::String;

const SAMPLE_RATE_HZ: u32 = 16_000;
const PROMPT: &str = "Core S three ready";
const SPRITE_W: u16 = 272;
const SPRITE_H: u16 = 84;
const SPRITE_ORIGIN: Point = Point::new(24, 122);

type SoundSprite = DirtySprite<Rgb565, SPRITE_W, SPRITE_H, { 272 * 84 }, 8>;

struct SoundAsset {
    name: &'static str,
    wav: &'static [u8],
}

const SOUNDS: [SoundAsset; 4] = [
    SoundAsset {
        name: "neutral",
        wav: include_bytes!("../assets/neutral.wav"),
    },
    SoundAsset {
        name: "low",
        wav: include_bytes!("../assets/low.wav"),
    },
    SoundAsset {
        name: "high",
        wav: include_bytes!("../assets/high.wav"),
    },
    SoundAsset {
        name: "robot",
        wav: include_bytes!("../assets/robot.wav"),
    },
];

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

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

    let mut speaker = Aw88298::new(parts.internal_i2c);
    let aw_probe = speaker.read_register16(0x00).ok();
    let aw_init = speaker.init(SpeakerConfig::DEFAULT).is_ok();
    let aw_volume = speaker.read_register16(0x0C).ok();
    let _i2c = speaker.release();

    let display = &mut parts.display;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 sound_wav demo",
    }
    .draw(display, Theme::DARK)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::MAGENTA, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "WAV SOUND PLAYBACK",
        top_left: Point::new(24, 56),
        color: Rgb565::MAGENTA,
    }
    .draw(display)
    .expect("title");

    draw_static_info(display, aw_probe, aw_init, aw_volume);

    let Ok(mut tx_buffer) = dma_tx_buffer!(4092) else {
        draw_i2s_error(display);
        loop {
            core::hint::spin_loop();
        }
    };
    let Ok(i2s) = I2s::new(
        peripherals.I2S1,
        peripherals.DMA_CH1,
        TdmConfig::new_tdm_philips()
            .with_sample_rate(Rate::from_hz(SAMPLE_RATE_HZ))
            .with_data_format(DataFormat::Data16Channel16)
            .with_channels(Channels::STEREO),
    ) else {
        draw_i2s_error(display);
        loop {
            core::hint::spin_loop();
        }
    };

    let mut i2s_tx = i2s
        .i2s_tx
        .with_bclk(peripherals.GPIO34)
        .with_ws(peripherals.GPIO33)
        .with_dout(peripherals.GPIO13)
        .build();

    let mut sprite = SoundSprite::new(Rgb565::BLACK).expect("valid sound sprite");
    let mut sound_index = 0usize;
    let mut play_count = 0u32;

    loop {
        let sound = &SOUNDS[sound_index];
        play_count = play_count.wrapping_add(1);
        draw_sound_state(display, &mut sprite, sound.name, play_count, true);

        let played = if let Ok(mut wav) = WavPcm16::new(sound.wav) {
            match play_source(i2s_tx, tx_buffer, &mut wav) {
                Some((next_i2s_tx, next_tx_buffer, played)) => {
                    i2s_tx = next_i2s_tx;
                    tx_buffer = next_tx_buffer;
                    played
                }
                None => {
                    draw_sound_state(display, &mut sprite, sound.name, play_count, false);
                    loop {
                        core::hint::spin_loop();
                    }
                }
            }
        } else {
            false
        };
        draw_sound_state(display, &mut sprite, sound.name, play_count, played);

        delay.delay(Duration::from_millis(1_200));

        sound_index = (sound_index + 1) % SOUNDS.len();
    }
}

fn draw_static_info<T>(
    display: &mut T,
    aw_probe: Option<u16>,
    aw_init: bool,
    aw_volume: Option<u16>,
) where
    T: DrawTarget<Color = Rgb565>,
{
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);

    let mut line: String<64> = String::new();
    write!(
        &mut line,
        "AW88298 probe:{} init:{} vol:{}",
        HexWord(aw_probe),
        if aw_init { "OK" } else { "FAIL" },
        HexWord(aw_volume)
    )
    .unwrap();
    Text::new(&line, Point::new(24, 86), if aw_init { ok } else { error })
        .draw(display)
        .ok();

    Text::new("I2S1: BCK34 WS33 DO13 @16k", Point::new(24, 104), style)
        .draw(display)
        .ok();
    Text::new("Embedded WAV speech from flash", Point::new(24, 218), warn)
        .draw(display)
        .ok();
}

fn draw_i2s_error<T>(display: &mut T)
where
    T: DrawTarget<Color = Rgb565>,
{
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);
    Text::new("I2S setup failed", Point::new(24, 140), error)
        .draw(display)
        .ok();
}

fn draw_sound_state<T>(
    display: &mut T,
    sprite: &mut SoundSprite,
    sound_name: &str,
    play_count: u32,
    played: bool,
) where
    T: DrawTarget<Color = Rgb565>,
{
    sprite.clear(Rgb565::BLACK);
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::MAGENTA);
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    let mut line: String<64> = String::new();
    write!(&mut line, "Prompt: \"{}\"", PROMPT).unwrap();
    Text::new(&line, Point::new(0, 12), style).draw(sprite).ok();

    line.clear();
    write!(&mut line, "WAV asset: {}", sound_name).unwrap();
    Text::new(&line, Point::new(0, 32), accent)
        .draw(sprite)
        .ok();

    line.clear();
    write!(&mut line, "Play #: {}", play_count).unwrap();
    Text::new(&line, Point::new(0, 52), style).draw(sprite).ok();

    Text::new(
        if played {
            "Playback: OK"
        } else {
            "Playback: FAIL"
        },
        Point::new(0, 72),
        if played { ok } else { error },
    )
    .draw(sprite)
    .ok();

    sprite.flush_dirty_at(display, SPRITE_ORIGIN).ok();
}

fn play_source<'d, T>(
    mut i2s_tx: I2sTx<'d, Blocking>,
    mut tx_buffer: DmaTxBuf,
    source: &mut T,
) -> Option<(I2sTx<'d, Blocking>, DmaTxBuf, bool)>
where
    T: AudioSource,
{
    let mut any_audio = false;
    loop {
        let has_more = fill_stereo_i16_buffer(tx_buffer.as_mut_slice(), source);
        tx_buffer.set_length(tx_buffer.capacity());
        if !has_more && any_audio {
            break;
        }
        any_audio |= has_more;

        let Ok(transfer) = i2s_tx.write(tx_buffer) else {
            return None;
        };
        let (result, next_i2s_tx, next_tx_buffer) = transfer.wait();
        if result.is_err() {
            return None;
        }
        i2s_tx = next_i2s_tx;
        tx_buffer = next_tx_buffer;

        if !has_more {
            break;
        }
    }
    Some((i2s_tx, tx_buffer, any_audio))
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

struct HexWord(Option<u16>);

impl core::fmt::Display for HexWord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(value) => write!(f, "0x{value:04X}"),
            None => f.write_str("--"),
        }
    }
}
