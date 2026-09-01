//! Audio codec configuration helpers for CoreS3.
//!
//! CoreS3 uses an ES7210 microphone ADC and AW88298 speaker amplifier. This
//! module configures the I²C-controlled devices, documents the expected I²S
//! format, and provides small `no_std` sample-source helpers for raw PCM, tones,
//! and simple text-to-audio prompts. High-throughput I²S clocks/DMA remain in
//! the HAL/application layer.

use embedded_hal::i2c::I2c;

use crate::devices;

const SINE_64_Q15: [i16; 64] = [
    0, 3212, 6393, 9512, 12539, 15446, 18205, 20787, 23170, 25329, 27245, 28898, 30273, 31357,
    32138, 32610, 32767, 32610, 32138, 31357, 30273, 28898, 27245, 25329, 23170, 20787, 18205,
    15446, 12539, 9512, 6393, 3212, 0, -3212, -6393, -9512, -12539, -15446, -18205, -20787, -23170,
    -25329, -27245, -28898, -30273, -31357, -32138, -32610, -32767, -32610, -32138, -31357, -30273,
    -28898, -27245, -25329, -23170, -20787, -18205, -15446, -12539, -9512, -6393, -3212,
];

/// Common audio sample rates supported by the CoreS3 audio helpers.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleRate {
    /// 8 kHz, suitable for very small voice prompts.
    Rate8K,
    /// 16 kHz, the default low-bandwidth voice/microphone rate.
    Rate16K,
    /// 24 kHz.
    Rate24K,
    /// 32 kHz.
    Rate32K,
    /// 44.1 kHz.
    Rate44K1,
    /// 48 kHz.
    Rate48K,
}

/// I²S sample width used for codec configuration.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleWidth {
    /// 16-bit samples.
    Bits16,
    /// 24-bit samples packed according to the selected HAL I²S mode.
    Bits24,
    /// 32-bit samples packed according to the selected HAL I²S mode.
    Bits32,
}

/// I²S frame alignment mode for the codec control path.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum I2sMode {
    /// Standard Philips I²S timing.
    Standard,
    /// Left-justified timing.
    LeftJustified,
}

/// Board-level I²S format expected by the ES7210/AW88298 path.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I2sAudioFormat {
    /// Audio sample rate.
    pub sample_rate: SampleRate,
    /// Bits per sample.
    pub sample_width: SampleWidth,
    /// I²S frame alignment mode.
    pub mode: I2sMode,
    /// Number of microphone slots/channels expected from ES7210.
    pub microphone_channels: u8,
    /// Number of speaker slots/channels expected by the playback path.
    pub speaker_channels: u8,
}

impl I2sAudioFormat {
    /// Conservative default format for board examples: 16 kHz, 16-bit standard I²S.
    pub const DEFAULT: Self = Self {
        sample_rate: SampleRate::Rate16K,
        sample_width: SampleWidth::Bits16,
        mode: I2sMode::Standard,
        microphone_channels: 2,
        speaker_channels: 1,
    };

    /// Return the sample rate in hertz.
    pub const fn sample_rate_hz(self) -> u32 {
        match self.sample_rate {
            SampleRate::Rate8K => 8_000,
            SampleRate::Rate16K => 16_000,
            SampleRate::Rate24K => 24_000,
            SampleRate::Rate32K => 32_000,
            SampleRate::Rate44K1 => 44_100,
            SampleRate::Rate48K => 48_000,
        }
    }
}

/// ES7210 microphone ADC configuration.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MicrophoneConfig {
    /// Expected I²S format for microphone samples.
    pub format: I2sAudioFormat,
    /// Analog/digital gain request in dB, clamped to the safe supported range.
    pub gain_db: u8,
}

impl MicrophoneConfig {
    /// Default CoreS3 microphone configuration.
    pub const DEFAULT: Self = Self {
        format: I2sAudioFormat::DEFAULT,
        gain_db: 24,
    };
}

/// AW88298 speaker amplifier configuration.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpeakerConfig {
    /// Expected I²S format for speaker playback samples.
    pub format: I2sAudioFormat,
    /// Whether the amplifier should be enabled during initialization.
    pub enabled: bool,
    /// Speaker volume percentage-like value used by the AW88298 helper.
    ///
    /// M5Unified writes `100` for full-volume CoreS3 smoke tests; values above
    /// `100` are clamped by this helper.
    pub volume: u8,
}

impl SpeakerConfig {
    /// Default CoreS3 speaker amplifier configuration.
    pub const DEFAULT: Self = Self {
        format: I2sAudioFormat::DEFAULT,
        enabled: true,
        volume: 100,
    };
}

/// Pull-based mono PCM source for application-owned I²S streaming.
///
/// Samples are signed 16-bit PCM. Applications can duplicate samples to stereo
/// frames or write mono frames depending on their HAL/I²S configuration.
pub trait AudioSource {
    /// Return the next signed 16-bit mono sample, or `None` when a finite source is exhausted.
    fn next_sample(&mut self) -> Option<i16>;
}

/// Raw signed 16-bit PCM sample source backed by a borrowed slice.
pub struct RawPcm<'a> {
    samples: &'a [i16],
    position: usize,
    repeat: bool,
}

impl<'a> RawPcm<'a> {
    /// Create a finite PCM source that emits each sample exactly once.
    pub const fn once(samples: &'a [i16]) -> Self {
        Self {
            samples,
            position: 0,
            repeat: false,
        }
    }

    /// Create a PCM source that loops over `samples` forever.
    ///
    /// Empty slices still return `None`.
    pub const fn repeating(samples: &'a [i16]) -> Self {
        Self {
            samples,
            position: 0,
            repeat: true,
        }
    }
}

impl AudioSource for RawPcm<'_> {
    fn next_sample(&mut self) -> Option<i16> {
        if self.samples.is_empty() {
            return None;
        }
        if self.position >= self.samples.len() {
            if self.repeat {
                self.position = 0;
            } else {
                return None;
            }
        }
        let sample = self.samples[self.position];
        self.position += 1;
        Some(sample)
    }
}

/// Small sine-wave tone source for beeps and smoke tests.
pub struct Tone {
    phase: u32,
    phase_step: u32,
    remaining_samples: u32,
    amplitude: i16,
}

impl Tone {
    /// Create a finite sine tone.
    ///
    /// `amplitude` is the signed 16-bit peak amplitude. A zero `sample_rate_hz`
    /// creates an already-exhausted source instead of panicking.
    pub const fn new(
        frequency_hz: u16,
        duration_ms: u16,
        sample_rate_hz: u32,
        amplitude: i16,
    ) -> Self {
        let phase_step = if sample_rate_hz == 0 {
            0
        } else {
            ((frequency_hz as u32) << 16) / sample_rate_hz
        };
        let remaining_samples = if sample_rate_hz == 0 {
            0
        } else {
            (sample_rate_hz / 1000) * duration_ms as u32
        };

        Self {
            phase: 0,
            phase_step,
            remaining_samples,
            amplitude,
        }
    }
}

impl AudioSource for Tone {
    fn next_sample(&mut self) -> Option<i16> {
        if self.remaining_samples == 0 {
            return None;
        }
        self.remaining_samples -= 1;
        let index = ((self.phase >> 10) & 0x3F) as usize;
        self.phase = self.phase.wrapping_add(self.phase_step);
        Some(((i32::from(SINE_64_Q15[index]) * i32::from(self.amplitude)) / 32767) as i16)
    }
}

/// Voice profile for the small prompt generator.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Voice {
    /// Mid-range, unobtrusive prompt voice.
    Neutral,
    /// Lower-pitched prompt voice.
    Low,
    /// Higher-pitched prompt voice.
    High,
    /// Wider pitch steps for a more synthetic prompt.
    Robot,
}

impl Voice {
    const fn profile(self) -> VoiceProfile {
        match self {
            Voice::Neutral => VoiceProfile {
                base_hz: 420,
                step_hz: 18,
                amplitude: 6_000,
            },
            Voice::Low => VoiceProfile {
                base_hz: 260,
                step_hz: 12,
                amplitude: 7_000,
            },
            Voice::High => VoiceProfile {
                base_hz: 620,
                step_hz: 24,
                amplitude: 5_000,
            },
            Voice::Robot => VoiceProfile {
                base_hz: 360,
                step_hz: 48,
                amplitude: 8_000,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct VoiceProfile {
    base_hz: u16,
    step_hz: u16,
    amplitude: i16,
}

/// Tiny no-heap text-to-audio prompt source.
///
/// This is intentionally not natural speech synthesis. It maps text into short
/// voice-dependent pitched tones so firmware can provide audible prompts without
/// allocation or a large TTS engine. Full natural TTS should live in an
/// application/service layer and feed this crate through [`RawPcm`].
pub struct TextToAudio<'a> {
    text: &'a [u8],
    position: usize,
    current: Option<Tone>,
    voice: VoiceProfile,
    sample_rate_hz: u32,
}

impl<'a> TextToAudio<'a> {
    /// Create a prompt source from UTF-8 text.
    ///
    /// The generator consumes bytes, so non-ASCII text will still produce a prompt
    /// but not phonetic speech. Use [`RawPcm`] for application-generated natural TTS.
    pub fn new(text: &'a str, voice: Voice, sample_rate_hz: u32) -> Self {
        Self {
            text: text.as_bytes(),
            position: 0,
            current: None,
            voice: voice.profile(),
            sample_rate_hz,
        }
    }
}

impl AudioSource for TextToAudio<'_> {
    fn next_sample(&mut self) -> Option<i16> {
        loop {
            if let Some(tone) = &mut self.current {
                if let Some(sample) = tone.next_sample() {
                    return Some(sample);
                }
            }
            self.current = None;
            let byte = *self.text.get(self.position)?;
            self.position += 1;
            let duration = if byte == b' ' { 90 } else { 70 };
            let frequency = char_frequency(byte, self.voice);
            let amplitude = if byte == b' ' {
                0
            } else {
                self.voice.amplitude
            };
            self.current = Some(Tone::new(
                frequency,
                duration,
                self.sample_rate_hz,
                amplitude,
            ));
        }
    }
}

fn char_frequency(byte: u8, voice: VoiceProfile) -> u16 {
    if byte == b' ' {
        return 1;
    }
    let normalized = byte.to_ascii_lowercase();
    let bucket = if normalized.is_ascii_lowercase() {
        normalized - b'a'
    } else if normalized.is_ascii_digit() {
        normalized - b'0' + 26
    } else {
        36
    };
    voice.base_hz + u16::from(bucket % 16) * voice.step_hz
}

/// ES7210 microphone ADC I²C control driver.
pub struct Es7210<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Es7210<I2C> {
    /// Create an ES7210 driver using the CoreS3 ES7210 I²C address.
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::ES7210_ADC,
        }
    }
    /// Release the underlying I²C bus/device.
    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Es7210<I2C>
where
    I2C: I2c<Error = Error>,
{
    /// Apply a conservative ES7210 initialization sequence for CoreS3 microphones.
    pub fn init(&mut self, config: MicrophoneConfig) -> Result<(), Error> {
        // Conservative ES7210 setup for I²S slave mode. Applications may tune
        // registers further for clock tree and analog gain requirements.
        self.write_register(0x00, 0xFF)?;
        self.write_register(0x00, 0x32)?;
        self.write_register(0x01, 0x30)?;
        self.write_register(0x02, 0x10)?;
        self.write_register(0x03, 0x20)?;
        self.write_register(0x04, sample_width_code(config.format.sample_width))?;
        self.write_register(0x22, config.gain_db.min(37))
    }

    /// Set microphone gain, clamped to the supported range used by this helper.
    pub fn set_gain(&mut self, gain_db: u8) -> Result<(), Error> {
        self.write_register(0x22, gain_db.min(37))
    }

    /// Read one ES7210 register.
    pub fn read_register(&mut self, register: u8) -> Result<u8, Error> {
        let mut value = [0u8];
        self.i2c.write_read(self.address, &[register], &mut value)?;
        Ok(value[0])
    }

    /// Write one ES7210 register.
    pub fn write_register(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.i2c.write(self.address, &[register, value])
    }
}

/// AW88298 speaker amplifier I²C control driver.
pub struct Aw88298<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Aw88298<I2C> {
    /// Create an AW88298 driver using the CoreS3 amplifier I²C address.
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::AW88298_AMPLIFIER,
        }
    }
    /// Release the underlying I²C bus/device.
    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Aw88298<I2C>
where
    I2C: I2c<Error = Error>,
{
    /// Apply the CoreS3 AW88298 speaker amplifier sequence used by M5Unified.
    ///
    /// The CoreS3 speaker path clocks the amplifier from BCK/WS/DOUT. The helper
    /// selects the AW88298 sample-rate bucket from [`SpeakerConfig::format`] and
    /// enables I²S input, unmutes the high-level path, disables boost mode, and
    /// applies a clamped volume value.
    pub fn init(&mut self, config: SpeakerConfig) -> Result<(), Error> {
        if config.enabled {
            self.enable_core_s3(config)
        } else {
            self.set_enabled(false)
        }
    }

    /// Enable or disable the CoreS3 amplifier output path.
    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), Error> {
        if enabled {
            self.enable_core_s3(SpeakerConfig::DEFAULT)
        } else {
            self.write_register16(0x04, 0x4000)
        }
    }

    /// Set the CoreS3 amplifier volume register value.
    pub fn set_volume(&mut self, volume: u8) -> Result<(), Error> {
        self.write_register16(0x0C, u16::from(volume.min(100)))
    }

    fn enable_core_s3(&mut self, config: SpeakerConfig) -> Result<(), Error> {
        let sample_rate_code = aw88298_sample_rate_code(config.format.sample_rate_hz());
        self.write_register16(0x61, 0x0673)?;
        self.write_register16(0x04, 0x4040)?;
        self.write_register16(0x05, 0x0008)?;
        self.write_register16(0x06, 0x14C0 | u16::from(sample_rate_code))?;
        self.set_volume(config.volume)
    }

    /// Read one 16-bit AW88298 register.
    pub fn read_register16(&mut self, register: u8) -> Result<u16, Error> {
        let mut data = [0u8; 2];
        self.i2c.write_read(self.address, &[register], &mut data)?;
        Ok(u16::from_be_bytes(data))
    }

    /// Write one 16-bit AW88298 register.
    pub fn write_register16(&mut self, register: u8, value: u16) -> Result<(), Error> {
        let bytes = value.to_be_bytes();
        self.i2c
            .write(self.address, &[register, bytes[0], bytes[1]])
    }
}

const fn sample_width_code(width: SampleWidth) -> u8 {
    match width {
        SampleWidth::Bits16 => 0x60,
        SampleWidth::Bits24 => 0x00,
        SampleWidth::Bits32 => 0x10,
    }
}

const fn aw88298_sample_rate_code(sample_rate_hz: u32) -> u8 {
    let rate = (sample_rate_hz + 1_102) / 2_205;
    if rate <= 4 {
        0
    } else if rate <= 5 {
        1
    } else if rate <= 6 {
        2
    } else if rate <= 8 {
        3
    } else if rate <= 10 {
        4
    } else if rate <= 11 {
        5
    } else if rate <= 15 {
        6
    } else if rate <= 20 {
        7
    } else if rate <= 22 {
        8
    } else {
        9
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_pcm_once_stops_after_slice() {
        let mut pcm = RawPcm::once(&[10, -20]);

        assert_eq!(pcm.next_sample(), Some(10));
        assert_eq!(pcm.next_sample(), Some(-20));
        assert_eq!(pcm.next_sample(), None);
        assert_eq!(pcm.next_sample(), None);
    }

    #[test]
    fn raw_pcm_repeating_loops() {
        let mut pcm = RawPcm::repeating(&[1, 2, 3]);

        assert_eq!(pcm.next_sample(), Some(1));
        assert_eq!(pcm.next_sample(), Some(2));
        assert_eq!(pcm.next_sample(), Some(3));
        assert_eq!(pcm.next_sample(), Some(1));
    }

    #[test]
    fn empty_repeating_pcm_stops() {
        let mut pcm = RawPcm::repeating(&[]);

        assert_eq!(pcm.next_sample(), None);
    }

    #[test]
    fn tone_yields_finite_nonzero_samples() {
        let mut tone = Tone::new(440, 10, 16_000, 8_000);
        let mut nonzero = false;
        let mut count = 0;

        while let Some(sample) = tone.next_sample() {
            nonzero |= sample != 0;
            count += 1;
        }

        assert!(nonzero);
        assert_eq!(count, 160);
        assert_eq!(tone.next_sample(), None);
    }

    #[test]
    fn tone_with_zero_sample_rate_is_exhausted() {
        let mut tone = Tone::new(440, 100, 0, 8_000);

        assert_eq!(tone.next_sample(), None);
    }

    #[test]
    fn text_to_audio_emits_for_each_voice() {
        for voice in [Voice::Neutral, Voice::Low, Voice::High, Voice::Robot] {
            let mut prompt = TextToAudio::new("ok", voice, 8_000);
            let mut count = 0;
            let mut nonzero = false;

            while let Some(sample) = prompt.next_sample() {
                count += 1;
                nonzero |= sample != 0;
            }

            assert_eq!(count, 1_120);
            assert!(nonzero);
        }
    }

    #[test]
    fn text_to_audio_spaces_are_silent() {
        let mut prompt = TextToAudio::new(" ", Voice::Neutral, 8_000);
        let mut count = 0;

        while let Some(sample) = prompt.next_sample() {
            count += 1;
            assert_eq!(sample, 0);
        }

        assert_eq!(count, 720);
    }

    #[test]
    fn aw88298_sample_rate_code_matches_m5_rate_buckets() {
        assert_eq!(aw88298_sample_rate_code(8_000), 0);
        assert_eq!(aw88298_sample_rate_code(16_000), 3);
        assert_eq!(aw88298_sample_rate_code(44_100), 7);
        assert_eq!(aw88298_sample_rate_code(48_000), 8);
    }
}
