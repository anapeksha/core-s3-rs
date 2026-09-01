//! Audio codec configuration helpers for CoreS3.
//!
//! CoreS3 uses an ES7210 microphone ADC and AW88298 speaker amplifier. This
//! module configures the I²C-controlled devices and documents the expected I²S
//! format. High-throughput I²S clocks/DMA remain in the HAL/application layer.

use embedded_hal::i2c::I2c;

use crate::devices;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleRate {
    Rate8K,
    Rate16K,
    Rate24K,
    Rate32K,
    Rate44K1,
    Rate48K,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleWidth {
    Bits16,
    Bits24,
    Bits32,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum I2sMode {
    Standard,
    LeftJustified,
}

/// Board-level I²S format expected by the ES7210/AW88298 path.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I2sAudioFormat {
    pub sample_rate: SampleRate,
    pub sample_width: SampleWidth,
    pub mode: I2sMode,
    pub microphone_channels: u8,
    pub speaker_channels: u8,
}

impl I2sAudioFormat {
    pub const DEFAULT: Self = Self {
        sample_rate: SampleRate::Rate16K,
        sample_width: SampleWidth::Bits16,
        mode: I2sMode::Standard,
        microphone_channels: 2,
        speaker_channels: 1,
    };
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MicrophoneConfig {
    pub format: I2sAudioFormat,
    pub gain_db: u8,
}

impl MicrophoneConfig {
    pub const DEFAULT: Self = Self {
        format: I2sAudioFormat::DEFAULT,
        gain_db: 24,
    };
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpeakerConfig {
    pub format: I2sAudioFormat,
    pub enabled: bool,
    pub volume: u8,
}

impl SpeakerConfig {
    pub const DEFAULT: Self = Self {
        format: I2sAudioFormat::DEFAULT,
        enabled: true,
        volume: 128,
    };
}

pub struct Es7210<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Es7210<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::ES7210_ADC,
        }
    }
    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Es7210<I2C>
where
    I2C: I2c<Error = Error>,
{
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

    pub fn set_gain(&mut self, gain_db: u8) -> Result<(), Error> {
        self.write_register(0x22, gain_db.min(37))
    }

    pub fn read_register(&mut self, register: u8) -> Result<u8, Error> {
        let mut value = [0u8];
        self.i2c.write_read(self.address, &[register], &mut value)?;
        Ok(value[0])
    }

    pub fn write_register(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.i2c.write(self.address, &[register, value])
    }
}

pub struct Aw88298<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Aw88298<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::AW88298_AMPLIFIER,
        }
    }
    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Aw88298<I2C>
where
    I2C: I2c<Error = Error>,
{
    pub fn init(&mut self, config: SpeakerConfig) -> Result<(), Error> {
        self.write_register16(0x01, if config.enabled { 0x0000 } else { 0x0001 })?;
        self.set_volume(config.volume)
    }

    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), Error> {
        self.write_register16(0x01, if enabled { 0x0000 } else { 0x0001 })
    }

    pub fn set_volume(&mut self, volume: u8) -> Result<(), Error> {
        self.write_register16(0x04, u16::from(volume))
    }

    pub fn read_register16(&mut self, register: u8) -> Result<u16, Error> {
        let mut data = [0u8; 2];
        self.i2c.write_read(self.address, &[register], &mut data)?;
        Ok(u16::from_be_bytes(data))
    }

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
