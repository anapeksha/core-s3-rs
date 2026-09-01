//! BM8563 RTC driver and `no_std` date/time types.

use embedded_hal::i2c::I2c;

use crate::devices;

const REG_CONTROL_STATUS1: u8 = 0x00;
const REG_SECONDS: u8 = 0x02;
const REG_MINUTE_ALARM: u8 = 0x09;
const REG_TIMER_CONTROL: u8 = 0x0E;
const REG_TIMER: u8 = 0x0F;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub weekday: u8,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Time {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DateTime {
    pub date: Date,
    pub time: Time,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Alarm {
    pub minute: Option<u8>,
    pub hour: Option<u8>,
    pub day: Option<u8>,
    pub weekday: Option<u8>,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerClock {
    Hz4096,
    Hz64,
    Hz1,
    OneOver60Hz,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerConfig {
    pub clock: TimerClock,
    pub count: u8,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimedWakeMetadata {
    pub alarm: Option<Alarm>,
    pub timer: Option<TimerConfig>,
}

pub struct Bm8563<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Bm8563<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::BM8563_RTC,
        }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }
}

impl<I2C, Error> Bm8563<I2C>
where
    I2C: I2c<Error = Error>,
{
    pub fn init(&mut self) -> Result<(), Error> {
        self.write_register(REG_CONTROL_STATUS1, 0x00)
    }

    pub fn datetime(&mut self) -> Result<DateTime, Error> {
        let data = self.datetime_registers()?;
        Ok(DateTime {
            time: Time {
                second: bcd_to_bin(data[0] & 0x7F),
                minute: bcd_to_bin(data[1] & 0x7F),
                hour: bcd_to_bin(data[2] & 0x3F),
            },
            date: Date {
                day: bcd_to_bin(data[3] & 0x3F),
                weekday: data[4] & 0x07,
                month: bcd_to_bin(data[5] & 0x1F),
                year: 2000 + u16::from(bcd_to_bin(data[6])),
            },
        })
    }

    /// Returns `true` when BM8563 reports low voltage or clock-integrity loss.
    ///
    /// BM8563 stores this in bit 7 of the seconds register. When set, the date
    /// and time should be treated as potentially invalid until application code
    /// sets a known-good time.
    pub fn clock_integrity_lost(&mut self) -> Result<bool, Error> {
        Ok(self.datetime_registers()?[0] & 0x80 != 0)
    }

    pub fn set_datetime(&mut self, datetime: DateTime) -> Result<(), Error> {
        let data = [
            REG_SECONDS,
            bin_to_bcd(datetime.time.second),
            bin_to_bcd(datetime.time.minute),
            bin_to_bcd(datetime.time.hour),
            bin_to_bcd(datetime.date.day),
            datetime.date.weekday & 0x07,
            bin_to_bcd(datetime.date.month),
            bin_to_bcd((datetime.date.year % 100) as u8),
        ];
        self.i2c.write(self.address, &data)
    }

    pub fn set_alarm(&mut self, alarm: Alarm) -> Result<(), Error> {
        let disabled = 0x80;
        let data = [
            REG_MINUTE_ALARM,
            alarm.minute.map(bin_to_bcd).unwrap_or(disabled),
            alarm.hour.map(bin_to_bcd).unwrap_or(disabled),
            alarm.day.map(bin_to_bcd).unwrap_or(disabled),
            alarm.weekday.map(bin_to_bcd).unwrap_or(disabled),
        ];
        self.i2c.write(self.address, &data)
    }

    pub fn set_timer(&mut self, timer: TimerConfig) -> Result<(), Error> {
        self.write_register(REG_TIMER, timer.count)?;
        self.write_register(REG_TIMER_CONTROL, 0x80 | timer_clock_code(timer.clock))
    }

    fn datetime_registers(&mut self) -> Result<[u8; 7], Error> {
        let mut data = [0u8; 7];
        self.i2c
            .write_read(self.address, &[REG_SECONDS], &mut data)?;
        Ok(data)
    }

    fn write_register(&mut self, register: u8, value: u8) -> Result<(), Error> {
        self.i2c.write(self.address, &[register, value])
    }
}

pub const fn bcd_to_bin(value: u8) -> u8 {
    ((value >> 4) * 10) + (value & 0x0F)
}

pub const fn bin_to_bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}

const fn timer_clock_code(clock: TimerClock) -> u8 {
    match clock {
        TimerClock::Hz4096 => 0,
        TimerClock::Hz64 => 1,
        TimerClock::Hz1 => 2,
        TimerClock::OneOver60Hz => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bcd_round_trips() {
        assert_eq!(bcd_to_bin(0x59), 59);
        assert_eq!(bin_to_bcd(42), 0x42);
    }
}
