//! Power and battery helpers for the CoreS3 AXP2101 PMIC.
//!
//! Battery percentage is estimated from voltage only. This is useful for UI hints
//! but is not a precise state-of-charge measurement because LiPo voltage depends
//! on load, age, temperature, and charge/discharge history.

use embedded_hal::i2c::I2c;

use crate::devices;

const REG_STATUS1: u8 = 0x00;
const REG_STATUS2: u8 = 0x01;
const REG_DATA_BUFFER0: u8 = 0x04;
const REG_POWER_OFF: u8 = 0x10;
const REG_LDOS_ON_OFF: u8 = 0x90;
const REG_ALDO3_VOLTAGE: u8 = 0x94;
const REG_ALDO4_VOLTAGE: u8 = 0x95;
const REG_DLDO1_VOLTAGE: u8 = 0x99;
const REG_BATTERY_VOLTAGE_H: u8 = 0x34;
const REG_BATTERY_VOLTAGE_L: u8 = 0x35;
const LDO_3V3_CODE: u8 = 33 - 5;

/// Battery charging state reported or inferred from the PMIC.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargeState {
    Unknown,
    Discharging,
    Charging,
    Full,
}

/// External input power state.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalPower {
    Unknown,
    Disconnected,
    Connected,
}

/// High-level battery/power status for UI and application policy.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryStatus {
    pub millivolts: u16,
    pub percentage: u8,
    pub charge_state: ChargeState,
    pub external_power: ExternalPower,
    pub low_battery: bool,
}

impl BatteryStatus {
    pub const fn new(millivolts: u16, charge_state: ChargeState) -> Self {
        Self {
            millivolts,
            percentage: estimate_lipo_percentage(millivolts),
            charge_state,
            external_power: ExternalPower::Unknown,
            low_battery: millivolts <= LowBatteryThreshold::DEFAULT.millivolts,
        }
    }

    pub const fn with_power(
        millivolts: u16,
        charge_state: ChargeState,
        external_power: ExternalPower,
        threshold: LowBatteryThreshold,
    ) -> Self {
        Self {
            millivolts,
            percentage: estimate_lipo_percentage(millivolts),
            charge_state,
            external_power,
            low_battery: millivolts <= threshold.millivolts,
        }
    }
}

/// Low-battery threshold in millivolts.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LowBatteryThreshold {
    pub millivolts: u16,
}

impl LowBatteryThreshold {
    pub const DEFAULT: Self = Self { millivolts: 3_500 };
}

/// Integer exponential moving average for battery voltage readings.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VoltageSmoother {
    value_mv: Option<u16>,
    weight_new: u8,
}

impl VoltageSmoother {
    pub const fn new(weight_new: u8) -> Self {
        Self {
            value_mv: None,
            weight_new,
        }
    }

    pub fn update(&mut self, sample_mv: u16) -> u16 {
        let weight = self.weight_new.clamp(1, 100) as u32;
        let next = match self.value_mv {
            None => sample_mv,
            Some(current) => {
                let current = u32::from(current);
                let sample = u32::from(sample_mv);
                (((current * (100 - weight)) + (sample * weight)) / 100) as u16
            }
        };
        self.value_mv = Some(next);
        next
    }

    pub const fn value(&self) -> Option<u16> {
        self.value_mv
    }
}

/// Voltage-only LiPo percentage estimate.
pub const fn estimate_lipo_percentage(millivolts: u16) -> u8 {
    match millivolts {
        4200..=u16::MAX => 100,
        4100..=4199 => 90,
        4000..=4099 => 80,
        3920..=3999 => 70,
        3850..=3919 => 60,
        3790..=3849 => 50,
        3740..=3789 => 40,
        3700..=3739 => 30,
        3610..=3699 => 20,
        3500..=3609 => 10,
        3300..=3499 => 5,
        _ => 0,
    }
}

/// Generic blocking AXP2101 driver.
pub struct Axp2101<I2C> {
    i2c: I2C,
    address: u8,
    low_threshold: LowBatteryThreshold,
}

impl<I2C> Axp2101<I2C> {
    pub const fn new(i2c: I2C) -> Self {
        Self {
            i2c,
            address: devices::i2c::AXP2101_PMU,
            low_threshold: LowBatteryThreshold::DEFAULT,
        }
    }

    pub fn release(self) -> I2C {
        self.i2c
    }

    pub fn set_low_battery_threshold(&mut self, threshold: LowBatteryThreshold) {
        self.low_threshold = threshold;
    }
}

impl<I2C, Error> Axp2101<I2C>
where
    I2C: I2c<Error = Error>,
{
    pub fn init_core_s3_defaults(&mut self) -> Result<(), Error> {
        self.write_register(REG_LDOS_ON_OFF, 0xBF)?;
        self.write_register(REG_ALDO3_VOLTAGE, LDO_3V3_CODE)?;
        self.write_register(REG_ALDO4_VOLTAGE, LDO_3V3_CODE)
    }

    pub fn set_display_backlight(&mut self, brightness: u8) -> Result<(), Error> {
        if brightness == 0 {
            self.write_bit(REG_LDOS_ON_OFF, 7, false)
        } else {
            let voltage = ((u16::from(brightness) + 641) >> 5) as u8;
            self.write_bit(REG_LDOS_ON_OFF, 7, true)?;
            self.write_register(REG_DLDO1_VOLTAGE, voltage)
        }
    }

    pub fn battery_voltage_mv(&mut self) -> Result<u16, Error> {
        let high = u16::from(self.read_register(REG_BATTERY_VOLTAGE_H)?);
        let low = u16::from(self.read_register(REG_BATTERY_VOLTAGE_L)?);
        Ok(((high & 0x3F) << 8) | low)
    }

    pub fn status(&mut self) -> Result<BatteryStatus, Error> {
        let status1 = self.read_register(REG_STATUS1)?;
        let status2 = self.read_register(REG_STATUS2)?;
        let voltage = self.battery_voltage_mv().unwrap_or(0);
        let external = if status1 & 0x20 != 0 {
            ExternalPower::Connected
        } else {
            ExternalPower::Disconnected
        };
        let charge_state = if status2 & 0x04 != 0 {
            ChargeState::Charging
        } else if external == ExternalPower::Connected {
            ChargeState::Full
        } else {
            ChargeState::Discharging
        };
        Ok(BatteryStatus::with_power(
            voltage,
            charge_state,
            external,
            self.low_threshold,
        ))
    }

    pub fn prepare_sleep(&mut self, wake_marker: u8) -> Result<(), Error> {
        self.write_register(REG_DATA_BUFFER0, wake_marker)
    }

    pub fn shutdown(&mut self) -> Result<(), Error> {
        self.write_register(REG_POWER_OFF, 0x01)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_voltage_percentage() {
        assert_eq!(estimate_lipo_percentage(4200), 100);
        assert_eq!(estimate_lipo_percentage(3750), 40);
        assert_eq!(estimate_lipo_percentage(3400), 5);
    }

    #[test]
    fn smooths_voltage() {
        let mut smoother = VoltageSmoother::new(25);
        assert_eq!(smoother.update(4000), 4000);
        assert_eq!(smoother.update(3800), 3950);
    }
}
