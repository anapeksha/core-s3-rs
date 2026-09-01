//! AW9523B I/O expander support used by CoreS3 board power/reset paths.

use embedded_hal::i2c::I2c;

use crate::devices;

const REG_INPUT_P0: u8 = 0x00;
const REG_INPUT_P1: u8 = 0x01;
const REG_OUTPUT_P0: u8 = 0x02;
const REG_OUTPUT_P1: u8 = 0x03;
const REG_CONFIG_P0: u8 = 0x04;
const REG_CONFIG_P1: u8 = 0x05;
const REG_GLOBAL_CONTROL: u8 = 0x11;
const REG_LED_MODE_P0: u8 = 0x12;
const REG_LED_MODE_P1: u8 = 0x13;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Port {
    P0,
    P1,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpanderPin {
    pub port: Port,
    pub index: u8,
}

impl ExpanderPin {
    pub const LCD_RESET: Self = Self {
        port: Port::P1,
        index: 1,
    };
}

pub struct Aw9523b<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Aw9523b<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::AW9523B_GPIO_EXPANDER,
        }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Aw9523b<I2C>
where
    I2C: I2c<Error = Error>,
{
    pub fn init_core_s3_defaults(&mut self) -> Result<(), Error> {
        self.write_bit(REG_OUTPUT_P0, 0, true)?;
        self.write_bit(REG_OUTPUT_P0, 2, true)?;
        self.write_bit(REG_OUTPUT_P1, 0, true)?;
        self.write_bit(REG_OUTPUT_P1, 1, true)?;
        self.write_register(REG_CONFIG_P0, 0b0001_1000)?;
        self.write_register(REG_CONFIG_P1, 0b0000_1100)?;
        self.write_register(REG_GLOBAL_CONTROL, 0b0001_0000)?;
        self.write_register(REG_LED_MODE_P0, 0xFF)?;
        self.write_register(REG_LED_MODE_P1, 0xFF)
    }

    pub fn set_output(&mut self, pin: ExpanderPin, high: bool) -> Result<(), Error> {
        let reg = output_register(pin.port);
        self.write_bit(reg, pin.index, high)
    }

    pub fn read_input_port(&mut self, port: Port) -> Result<u8, Error> {
        self.read_register(match port {
            Port::P0 => REG_INPUT_P0,
            Port::P1 => REG_INPUT_P1,
        })
    }

    pub fn read_register(&mut self, register: u8) -> Result<u8, Error> {
        let mut value = [0u8];
        self.i2c.write_read(self.address, &[register], &mut value)?;
        Ok(value[0])
    }

    pub fn write_register(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.i2c.write(self.address, &[register, value])
    }

    pub fn write_bit(&mut self, register: u8, bit: u8, value: bool) -> Result<(), Error> {
        let current = self.read_register(register)?;
        let mask = 1u8 << bit;
        let next = if value {
            current | mask
        } else {
            current & !mask
        };
        self.write_register(register, next)
    }
}

const fn output_register(port: Port) -> u8 {
    match port {
        Port::P0 => REG_OUTPUT_P0,
        Port::P1 => REG_OUTPUT_P1,
    }
}
