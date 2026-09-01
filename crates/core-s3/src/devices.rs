/// Shared internal I²C bus addresses from the CoreS3 schematics/datasheets.
pub mod i2c {
    pub const AXP2101_PMU: u8 = 0x34;
    pub const BM8563_RTC: u8 = 0x51;
    pub const BMI270_IMU: u8 = 0x69;
    /// BMM150 address on the BMI270 auxiliary sensor-hub bus, not directly on CoreS3 internal I2C.
    pub const BMM150_MAGNETOMETER: u8 = 0x10;
    pub const LTR553_ALS_PROX: u8 = 0x23;
    pub const AW9523B_GPIO_EXPANDER: u8 = 0x58;
    pub const FT6336U_TOUCH: u8 = 0x38;
    pub const ES7210_ADC: u8 = 0x40;
    pub const AW88298_AMPLIFIER: u8 = 0x36;
}

/// Display geometry and controller metadata.
pub mod display {
    pub const WIDTH: u16 = 320;
    pub const HEIGHT: u16 = 240;
    pub const CONTROLLER: &str = "ILI9342C-compatible SPI TFT";
}

/// Camera sensor metadata.
pub mod camera {
    pub const SENSOR: &str = "GC0308";
    pub const WIDTH: u16 = 640;
    pub const HEIGHT: u16 = 480;
}

/// Audio codec metadata.
pub mod audio {
    pub const ADC: &str = "ES7210";
    pub const AMPLIFIER: &str = "AW88298";
}
