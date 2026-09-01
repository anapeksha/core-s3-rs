//! BMI270 accelerometer/gyroscope and BMM150 magnetometer helpers.
//!
//! Magnetometer readings are easily distorted by nearby magnets, speakers,
//! batteries, metal, and board current. Calibrate in the final enclosure and do
//! not treat heading as absolute without environmental validation.

use embedded_hal::{delay::DelayNs, i2c::I2c};

use crate::devices;

#[path = "motion/bmi270_config.rs"]
mod bmi270_config;

const BMI270_CHIP_ID: u8 = 0x00;
const BMI270_ACC_DATA: u8 = 0x0C;
const BMI270_GYR_DATA: u8 = 0x12;
const BMI270_ACC_CONF: u8 = 0x40;
const BMI270_ACC_RANGE: u8 = 0x41;
const BMI270_GYR_CONF: u8 = 0x42;
const BMI270_GYR_RANGE: u8 = 0x43;
const BMI270_INT_STATUS_1: u8 = 0x1D;
const BMI270_INTERNAL_STATUS: u8 = 0x21;
const BMI270_INIT_CTRL: u8 = 0x59;
const BMI270_INIT_ADDR_0: u8 = 0x5B;
const BMI270_INIT_DATA: u8 = 0x5E;
const BMI270_PWR_CONF: u8 = 0x7C;
const BMI270_PWR_CTRL: u8 = 0x7D;
const BMI270_CMD: u8 = 0x7E;
const BMI270_EXPECTED_CHIP_ID: u8 = 0x24;

const BMM150_CHIP_ID: u8 = 0x40;
const BMM150_DATA_X_LSB: u8 = 0x42;
const BMM150_POWER_CONTROL: u8 = 0x4B;
const BMM150_OP_MODE: u8 = 0x4C;
const BMM150_EXPECTED_CHIP_ID: u8 = 0x32;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Vector3 {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Vector3 {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    pub const fn offset(self, offsets: Self) -> Self {
        Self {
            x: self.x - offsets.x,
            y: self.y - offsets.y,
            z: self.z - offsets.z,
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccelRange {
    G2,
    G4,
    G8,
    G16,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GyroRange {
    Dps125,
    Dps250,
    Dps500,
    Dps1000,
    Dps2000,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleRate {
    Hz25,
    Hz50,
    Hz100,
    Hz200,
    Hz400,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bmi270Config {
    pub accel_range: AccelRange,
    pub gyro_range: GyroRange,
    pub sample_rate: SampleRate,
}

impl Bmi270Config {
    pub const DEFAULT: Self = Self {
        accel_range: AccelRange::G4,
        gyro_range: GyroRange::Dps500,
        sample_rate: SampleRate::Hz100,
    };
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MotionThreshold {
    pub accel_delta_mg: i32,
}

pub struct Bmi270<I2C> {
    i2c: I2C,
    address: u8,
    accel_offset: Vector3,
    gyro_offset: Vector3,
}

impl<I2C> Bmi270<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::BMI270_IMU,
            accel_offset: Vector3::new(0, 0, 0),
            gyro_offset: Vector3::new(0, 0, 0),
        }
    }

    pub fn set_accel_offset(&mut self, offset: Vector3) {
        self.accel_offset = offset;
    }
    pub fn set_gyro_offset(&mut self, offset: Vector3) {
        self.gyro_offset = offset;
    }
    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Bmi270<I2C>
where
    I2C: I2c<Error = Error>,
{
    /// Initializes the BMI270 register settings that do not require a delay provider.
    ///
    /// CoreS3 firmware should prefer [`Self::init_with_delay`] because BMI270
    /// requires delays while resetting and loading its mandatory configuration file.
    pub fn init(&mut self, config: Bmi270Config) -> Result<(), Error> {
        self.write_register(BMI270_PWR_CONF, 0x00)?;
        self.configure(config)?;
        self.write_register(BMI270_PWR_CTRL, 0x0E)
    }

    pub fn init_with_delay(
        &mut self,
        config: Bmi270Config,
        delay: &mut impl DelayNs,
    ) -> Result<(), Error> {
        self.write_register(BMI270_CMD, 0xB6)?;
        delay.delay_ms(2);
        self.write_register(BMI270_PWR_CONF, 0x00)?;
        delay.delay_ms(2);
        self.upload_config(delay)?;
        self.configure(config)?;
        self.write_register(BMI270_PWR_CTRL, 0x0E)?;
        delay.delay_ms(20);
        Ok(())
    }

    pub fn configure(&mut self, config: Bmi270Config) -> Result<(), Error> {
        self.write_register(BMI270_ACC_CONF, accel_conf_code(config.sample_rate))?;
        self.write_register(BMI270_GYR_CONF, gyro_conf_code(config.sample_rate))?;
        self.write_register(BMI270_ACC_RANGE, accel_range_code(config.accel_range))?;
        self.write_register(BMI270_GYR_RANGE, gyro_range_code(config.gyro_range))
    }

    pub fn chip_id(&mut self) -> Result<u8, Error> {
        self.read_register(BMI270_CHIP_ID)
    }
    pub fn is_expected_chip(&mut self) -> Result<bool, Error> {
        Ok(self.chip_id()? == BMI270_EXPECTED_CHIP_ID)
    }

    /// Returns the BMI270 internal status register.
    ///
    /// After a successful config upload the lower nibble should report `0x01`
    /// (`init_ok`). Other values indicate that acceleration/gyro data may stay
    /// invalid or zero.
    pub fn internal_status(&mut self) -> Result<u8, Error> {
        self.read_register(BMI270_INTERNAL_STATUS)
    }

    /// Returns the BMI270 interrupt/status register used to observe data-ready bits.
    pub fn interrupt_status_1(&mut self) -> Result<u8, Error> {
        self.read_register(BMI270_INT_STATUS_1)
    }

    /// Returns the BMI270 power-control register.
    pub fn power_control(&mut self) -> Result<u8, Error> {
        self.read_register(BMI270_PWR_CTRL)
    }

    pub fn acceleration_raw(&mut self) -> Result<Vector3, Error> {
        let raw = self.read_vector(BMI270_ACC_DATA)?;
        Ok(raw.offset(self.accel_offset))
    }

    pub fn gyroscope_raw(&mut self) -> Result<Vector3, Error> {
        let raw = self.read_vector(BMI270_GYR_DATA)?;
        Ok(raw.offset(self.gyro_offset))
    }

    fn upload_config(&mut self, delay: &mut impl DelayNs) -> Result<(), Error> {
        self.write_register(BMI270_INIT_CTRL, 0x00)?;
        delay.delay_ms(2);

        let config_file = bmi270_config::config_file();
        let mut offset = 0usize;
        while offset < config_file.len() {
            let chunk_len = (config_file.len() - offset).min(16);
            let word_addr = (offset / 2) as u16;
            self.i2c.write(
                self.address,
                &[BMI270_INIT_ADDR_0, word_addr as u8, (word_addr >> 8) as u8],
            )?;

            let mut packet = [0u8; 17];
            packet[0] = BMI270_INIT_DATA;
            packet[1..1 + chunk_len].copy_from_slice(&config_file[offset..offset + chunk_len]);
            self.i2c.write(self.address, &packet[..1 + chunk_len])?;
            offset += chunk_len;
        }

        self.write_register(BMI270_INIT_CTRL, 0x01)?;
        delay.delay_ms(20);
        Ok(())
    }

    fn read_vector(&mut self, start: u8) -> Result<Vector3, Error> {
        let mut data = [0u8; 6];
        self.i2c.write_read(self.address, &[start], &mut data)?;
        Ok(Vector3::new(
            i32::from(i16::from_le_bytes([data[0], data[1]])),
            i32::from(i16::from_le_bytes([data[2], data[3]])),
            i32::from(i16::from_le_bytes([data[4], data[5]])),
        ))
    }

    fn read_register(&mut self, register: u8) -> Result<u8, Error> {
        let mut value = [0u8];
        self.i2c.write_read(self.address, &[register], &mut value)?;
        Ok(value[0])
    }

    fn write_register(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.i2c.write(self.address, &[register, value])
    }
}

pub struct Bmm150<I2C> {
    i2c: I2C,
    address: u8,
    hard_iron_offset: Vector3,
}

impl<I2C> Bmm150<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::BMM150_MAGNETOMETER,
            hard_iron_offset: Vector3::new(0, 0, 0),
        }
    }

    pub fn set_hard_iron_offset(&mut self, offset: Vector3) {
        self.hard_iron_offset = offset;
    }
    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Bmm150<I2C>
where
    I2C: I2c<Error = Error>,
{
    pub fn init(&mut self) -> Result<(), Error> {
        self.write_register(BMM150_POWER_CONTROL, 0x01)?;
        self.write_register(BMM150_OP_MODE, 0x00)
    }

    pub fn chip_id(&mut self) -> Result<u8, Error> {
        self.read_register(BMM150_CHIP_ID)
    }
    pub fn is_expected_chip(&mut self) -> Result<bool, Error> {
        Ok(self.chip_id()? == BMM150_EXPECTED_CHIP_ID)
    }

    pub fn magnetic_raw(&mut self) -> Result<Vector3, Error> {
        let mut data = [0u8; 6];
        self.i2c
            .write_read(self.address, &[BMM150_DATA_X_LSB], &mut data)?;
        let raw = Vector3::new(
            i32::from(i16::from_le_bytes([data[0] & 0xF8, data[1]]) >> 3),
            i32::from(i16::from_le_bytes([data[2] & 0xF8, data[3]]) >> 3),
            i32::from(i16::from_le_bytes([data[4] & 0xFE, data[5]]) >> 1),
        );
        Ok(raw.offset(self.hard_iron_offset))
    }

    fn read_register(&mut self, register: u8) -> Result<u8, Error> {
        let mut value = [0u8];
        self.i2c.write_read(self.address, &[register], &mut value)?;
        Ok(value[0])
    }

    fn write_register(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.i2c.write(self.address, &[register, value])
    }
}

pub fn motion_detected(previous: Vector3, current: Vector3, threshold: MotionThreshold) -> bool {
    (current.x - previous.x).abs() >= threshold.accel_delta_mg
        || (current.y - previous.y).abs() >= threshold.accel_delta_mg
        || (current.z - previous.z).abs() >= threshold.accel_delta_mg
}

/// Returns heading in centidegrees using a small integer approximation.
pub fn heading_centidegrees(magnetic: Vector3) -> Option<u16> {
    if magnetic.x == 0 && magnetic.y == 0 {
        return None;
    }
    let angle = atan2_centidegrees(magnetic.y, magnetic.x);
    Some(if angle < 0 {
        (angle + 36_000) as u16
    } else {
        angle as u16
    })
}

fn sample_rate_code(rate: SampleRate) -> u8 {
    match rate {
        SampleRate::Hz25 => 0x06,
        SampleRate::Hz50 => 0x07,
        SampleRate::Hz100 => 0x08,
        SampleRate::Hz200 => 0x09,
        SampleRate::Hz400 => 0x0A,
    }
}

fn accel_conf_code(rate: SampleRate) -> u8 {
    0xA0 | sample_rate_code(rate)
}

fn gyro_conf_code(rate: SampleRate) -> u8 {
    0xA0 | sample_rate_code(rate)
}
fn accel_range_code(range: AccelRange) -> u8 {
    match range {
        AccelRange::G2 => 0x00,
        AccelRange::G4 => 0x01,
        AccelRange::G8 => 0x02,
        AccelRange::G16 => 0x03,
    }
}
fn gyro_range_code(range: GyroRange) -> u8 {
    match range {
        GyroRange::Dps2000 => 0x00,
        GyroRange::Dps1000 => 0x01,
        GyroRange::Dps500 => 0x02,
        GyroRange::Dps250 => 0x03,
        GyroRange::Dps125 => 0x04,
    }
}

fn atan2_centidegrees(y: i32, x: i32) -> i32 {
    // Fast integer approximation adequate for UI compass hints.
    let abs_y = y.abs();
    let angle = if x >= 0 {
        let r = ((x - abs_y) * 1000) / (x + abs_y).max(1);
        4500 - (4500 * r / 1000)
    } else {
        let r = ((x + abs_y) * 1000) / (abs_y - x).max(1);
        13_500 - (4500 * r / 1000)
    };
    if y < 0 { -angle } else { angle }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_motion_threshold() {
        assert!(motion_detected(
            Vector3::new(0, 0, 0),
            Vector3::new(101, 0, 0),
            MotionThreshold {
                accel_delta_mg: 100
            }
        ));
        assert!(!motion_detected(
            Vector3::new(0, 0, 0),
            Vector3::new(10, 0, 0),
            MotionThreshold {
                accel_delta_mg: 100
            }
        ));
    }

    #[test]
    fn heading_handles_cardinal_directions() {
        assert_eq!(heading_centidegrees(Vector3::new(1, 0, 0)), Some(0));
        assert_eq!(heading_centidegrees(Vector3::new(0, 1, 0)), Some(9000));
    }
}
