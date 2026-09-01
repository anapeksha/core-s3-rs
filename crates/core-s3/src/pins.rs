/// ESP32-S3 GPIO number.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Gpio(pub u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I2cBusPins {
    pub sda: Gpio,
    pub scl: Gpio,
}

impl I2cBusPins {
    /// Internal CoreS3 peripheral bus: PMU, RTC, IMU, magnetometer, ALS/prox,
    /// and AW9523B GPIO expander.
    pub const INTERNAL: Self = Self {
        sda: Gpio(12),
        scl: Gpio(11),
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiDisplayPins {
    pub sclk: Gpio,
    pub mosi: Gpio,
    pub dc: Gpio,
    pub cs: Gpio,
    pub reset: Option<Gpio>,
    pub backlight: Option<Gpio>,
}

impl SpiDisplayPins {
    /// LCD SPI bus and control lines. Backlight/reset are controlled through the
    /// board's power/GPIO-expander path on CoreS3, so direct GPIOs are absent.
    pub const LCD: Self = Self {
        sclk: Gpio(36),
        mosi: Gpio(37),
        dc: Gpio(35),
        cs: Gpio(3),
        reset: None,
        backlight: None,
    };
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpiSdPins {
    pub sclk: Gpio,
    pub mosi: Gpio,
    pub miso: Gpio,
    pub cs: Gpio,
}

impl SpiSdPins {
    /// TF/microSD socket on the shared LCD SPI signal group.
    pub const TF_CARD: Self = Self {
        sclk: Gpio(36),
        mosi: Gpio(37),
        miso: Gpio(35),
        cs: Gpio(4),
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct I2sAudioPins {
    pub mclk: Gpio,
    pub bclk: Gpio,
    pub lrclk: Gpio,
    pub dout: Gpio,
    pub din: Gpio,
}

impl I2sAudioPins {
    /// Shared I²S wiring for ES7210 microphone ADC and AW88298 speaker amp.
    pub const CODECS: Self = Self {
        mclk: Gpio(0),
        bclk: Gpio(34),
        lrclk: Gpio(33),
        dout: Gpio(13),
        din: Gpio(14),
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CameraPins {
    pub xclk: Gpio,
    pub pclk: Gpio,
    pub vsync: Gpio,
    pub href: Gpio,
    pub reset: Option<Gpio>,
    pub data: [Gpio; 8],
}

impl CameraPins {
    /// GC0308 DVP camera signal pins.
    pub const GC0308: Self = Self {
        xclk: Gpio(15),
        pclk: Gpio(39),
        vsync: Gpio(38),
        href: Gpio(47),
        reset: None,
        data: [
            Gpio(48),
            Gpio(17),
            Gpio(18),
            Gpio(16),
            Gpio(8),
            Gpio(9),
            Gpio(10),
            Gpio(46),
        ],
    };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrovePortPins {
    pub pin1: Gpio,
    pub pin2: Gpio,
}

impl GrovePortPins {
    /// HY2.0-4P Grove port A exposed on CoreS3.
    pub const PORT_A: Self = Self {
        pin1: Gpio(1),
        pin2: Gpio(2),
    };
}
