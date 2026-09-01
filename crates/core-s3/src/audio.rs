//! Audio codec configuration helpers for CoreS3.
//!
//! CoreS3 uses an ES7210 microphone ADC and AW88298 speaker amplifier. This
//! module configures the I²C-controlled devices, documents the expected I²S
//! format, and provides small `no_std` sample-source helpers for raw PCM, WAV
//! assets, and tones. High-throughput I²S clocks/DMA remain in the
//! HAL/application layer.

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

/// Error returned while parsing a PCM WAV asset.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WavError {
    /// The byte slice is too short for a WAV header or chunk.
    Truncated,
    /// The file is not a RIFF/WAVE file.
    NotWave,
    /// The file does not contain a supported `fmt ` chunk.
    UnsupportedFormat,
    /// The file has no PCM data chunk.
    MissingData,
}

/// Borrowed 16-bit PCM WAV source for app-owned I²S streaming.
///
/// This parser intentionally supports the simple format useful for embedded
/// prompts: RIFF/WAVE, PCM format tag 1, 16-bit little-endian samples, mono or
/// stereo. Stereo input is downmixed by averaging left and right samples. It does
/// not allocate and can wrap `include_bytes!()` assets stored in flash.
pub struct WavPcm16<'a> {
    data: &'a [u8],
    cursor: usize,
    end: usize,
    channels: u16,
    sample_rate_hz: u32,
}

impl<'a> WavPcm16<'a> {
    /// Parse a borrowed WAV file.
    pub fn new(wav: &'a [u8]) -> Result<Self, WavError> {
        if wav.len() < 12 {
            return Err(WavError::Truncated);
        }
        if &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
            return Err(WavError::NotWave);
        }

        let mut offset = 12;
        let mut channels = 0u16;
        let mut sample_rate_hz = 0u32;
        let mut bits_per_sample = 0u16;
        let mut format_tag = 0u16;
        let mut data_range = None;

        while offset + 8 <= wav.len() {
            let id = &wav[offset..offset + 4];
            let size = read_le_u32(wav, offset + 4).ok_or(WavError::Truncated)? as usize;
            let payload = offset + 8;
            let next = payload.checked_add(size).ok_or(WavError::Truncated)?;
            if next > wav.len() {
                return Err(WavError::Truncated);
            }

            if id == b"fmt " {
                if size < 16 {
                    return Err(WavError::UnsupportedFormat);
                }
                format_tag = read_le_u16(wav, payload).ok_or(WavError::Truncated)?;
                channels = read_le_u16(wav, payload + 2).ok_or(WavError::Truncated)?;
                sample_rate_hz = read_le_u32(wav, payload + 4).ok_or(WavError::Truncated)?;
                bits_per_sample = read_le_u16(wav, payload + 14).ok_or(WavError::Truncated)?;
            } else if id == b"data" {
                data_range = Some((payload, next));
            }

            offset = next + (size & 1);
        }

        if format_tag != 1 || bits_per_sample != 16 || !(channels == 1 || channels == 2) {
            return Err(WavError::UnsupportedFormat);
        }
        let (cursor, end) = data_range.ok_or(WavError::MissingData)?;
        Ok(Self {
            data: wav,
            cursor,
            end,
            channels,
            sample_rate_hz,
        })
    }

    /// Return the sample rate declared by the WAV file.
    pub const fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Return the number of source channels: 1 or 2.
    pub const fn channels(&self) -> u16 {
        self.channels
    }
}

impl AudioSource for WavPcm16<'_> {
    fn next_sample(&mut self) -> Option<i16> {
        match self.channels {
            1 => {
                if self.cursor + 2 > self.end {
                    return None;
                }
                let sample = read_le_i16(self.data, self.cursor)?;
                self.cursor += 2;
                Some(sample)
            }
            2 => {
                if self.cursor + 4 > self.end {
                    return None;
                }
                let left = i32::from(read_le_i16(self.data, self.cursor)?);
                let right = i32::from(read_le_i16(self.data, self.cursor + 2)?);
                self.cursor += 4;
                Some(((left + right) / 2) as i16)
            }
            _ => None,
        }
    }
}

fn read_le_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le_i16(data: &[u8], offset: usize) -> Option<i16> {
    let bytes = data.get(offset..offset + 2)?;
    Some(i16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_le_u32(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
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
    fn wav_pcm16_parses_mono_prompt() {
        let mut wav = WavPcm16::new(MONO_WAV).expect("valid mono wav");

        assert_eq!(wav.sample_rate_hz(), 16_000);
        assert_eq!(wav.channels(), 1);
        assert_eq!(wav.next_sample(), Some(1_000));
        assert_eq!(wav.next_sample(), Some(-1_000));
        assert_eq!(wav.next_sample(), None);
    }

    #[test]
    fn wav_pcm16_downmixes_stereo() {
        let mut wav = WavPcm16::new(STEREO_WAV).expect("valid stereo wav");

        assert_eq!(wav.sample_rate_hz(), 16_000);
        assert_eq!(wav.channels(), 2);
        assert_eq!(wav.next_sample(), Some(0));
        assert_eq!(wav.next_sample(), Some(0));
        assert_eq!(wav.next_sample(), None);
    }

    #[test]
    fn wav_pcm16_rejects_non_wave() {
        assert!(matches!(
            WavPcm16::new(b"not wave"),
            Err(WavError::Truncated)
        ));
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
    fn aw88298_sample_rate_code_matches_m5_rate_buckets() {
        assert_eq!(aw88298_sample_rate_code(8_000), 0);
        assert_eq!(aw88298_sample_rate_code(16_000), 3);
        assert_eq!(aw88298_sample_rate_code(44_100), 7);
        assert_eq!(aw88298_sample_rate_code(48_000), 8);
    }

    const MONO_WAV: &[u8] = &[
        b'R', b'I', b'F', b'F', 40, 0, 0, 0, b'W', b'A', b'V', b'E', b'f', b'm', b't', b' ', 16, 0,
        0, 0, 1, 0, 1, 0, 0x80, 0x3E, 0, 0, 0x00, 0x7D, 0, 0, 2, 0, 16, 0, b'd', b'a', b't', b'a',
        4, 0, 0, 0, 0xE8, 0x03, 0x18, 0xFC,
    ];

    const STEREO_WAV: &[u8] = &[
        b'R', b'I', b'F', b'F', 44, 0, 0, 0, b'W', b'A', b'V', b'E', b'f', b'm', b't', b' ', 16, 0,
        0, 0, 1, 0, 2, 0, 0x80, 0x3E, 0, 0, 0x00, 0xFA, 0, 0, 4, 0, 16, 0, b'd', b'a', b't', b'a',
        8, 0, 0, 0, 0xE8, 0x03, 0x18, 0xFC, 0x18, 0xFC, 0xE8, 0x03,
    ];
}
