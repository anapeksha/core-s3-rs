//! TF/microSD card metadata and helpers for CoreS3.
//!
//! CoreS3 routes the TF card over the same SPI signal group used by the LCD:
//! SCLK GPIO36, MOSI/COPI GPIO37, MISO/CIPO GPIO35, and CS GPIO4. Applications
//! that need both display and SD access should coordinate ownership of this
//! shared bus at the HAL layer. The card-detect switch is exposed through
//! AW9523B port 0 bit 4 and is active-low, matching the official M5Stack demo.

use embedded_hal::{delay::DelayNs, spi::SpiDevice};

use crate::pins::SpiSdPins;

/// Default SPI clock used by M5Stack's CoreS3 SD demo.
pub const DEFAULT_SPI_HZ: u32 = 25_000_000;
/// AW9523B port-0 bit used for TF card detect.
pub const CARD_DETECT_P0_BIT: u8 = 4;

/// Static CoreS3 TF-card wiring and bus settings.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdCardSlot {
    /// SPI pins for the TF card socket.
    pub spi: SpiSdPins,
    /// Recommended maximum SPI clock for initialization/use.
    pub spi_hz: u32,
    /// Card-detect signal exposed through AW9523B input port 0.
    pub detect: SdCardDetect,
}

impl SdCardSlot {
    /// CoreS3 onboard TF-card slot.
    pub const CORE_S3: Self = Self {
        spi: SpiSdPins::TF_CARD,
        spi_hz: DEFAULT_SPI_HZ,
        detect: SdCardDetect::AW9523B_P0_4_ACTIVE_LOW,
    };
}

/// Card-detect wiring description.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdCardDetect {
    /// AW9523B input port number.
    pub port: u8,
    /// Bit in the input port.
    pub bit: u8,
    /// Whether a low level means card-present.
    pub active_low: bool,
}

impl SdCardDetect {
    /// CoreS3 TF card detect: AW9523B port 0 bit 4, active-low.
    pub const AW9523B_P0_4_ACTIVE_LOW: Self = Self {
        port: 0,
        bit: CARD_DETECT_P0_BIT,
        active_low: true,
    };

    /// Interpret a raw AW9523B input-port value.
    pub const fn present_from_port_value(self, value: u8) -> bool {
        let high = (value & (1 << self.bit)) != 0;
        if self.active_low { !high } else { high }
    }
}

/// Runtime SD-slot metadata exposed to downstream storage stacks.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreS3SdSlot {
    /// SPI SCLK GPIO number.
    pub sclk_gpio: u8,
    /// SPI MOSI/COPI/CMD GPIO number.
    pub mosi_gpio: u8,
    /// SPI MISO/CIPO/D0 GPIO number. On CoreS3 this is also the LCD D/C pad.
    pub miso_gpio: u8,
    /// TF-card chip-select GPIO number.
    pub cs_gpio: u8,
    /// Maximum SPI frequency used by the official M5Stack SD example.
    pub max_frequency_hz: u32,
    /// Optional direct card-detect GPIO. CoreS3 uses AW9523B instead, so this is `None`.
    pub card_detect_gpio: Option<u8>,
    /// Optional direct power-enable GPIO. CoreS3 SD power is board-managed, so this is `None`.
    pub power_enable_gpio: Option<u8>,
}

impl CoreS3SdSlot {
    /// Onboard CoreS3 TF-card slot metadata.
    pub const CORE_S3: Self = Self {
        sclk_gpio: 36,
        mosi_gpio: 37,
        miso_gpio: 35,
        cs_gpio: 4,
        max_frequency_hz: DEFAULT_SPI_HZ,
        card_detect_gpio: None,
        power_enable_gpio: None,
    };
}

impl From<SdCardSlot> for CoreS3SdSlot {
    fn from(slot: SdCardSlot) -> Self {
        Self {
            sclk_gpio: slot.spi.sclk.0,
            mosi_gpio: slot.spi.mosi.0,
            miso_gpio: slot.spi.miso.0,
            cs_gpio: slot.spi.cs.0,
            max_frequency_hz: slot.spi_hz,
            card_detect_gpio: None,
            power_enable_gpio: None,
        }
    }
}

/// Low-level SD resources returned by ESP-HAL BSP helpers.
///
/// `spi_device` implements [`embedded_hal::spi::SpiDevice`] and can be passed to
/// `embedded_sdmmc::SdCard::new(spi_device, delay)` by downstream firmware. The
/// BSP intentionally does not add Wi-Fi credential, token, or application-secret
/// abstractions; applications should encrypt sensitive bytes before writing them.
pub struct CoreS3SdParts<SPI, DELAY> {
    /// Chip-select scoped SPI device for the TF-card socket.
    pub spi_device: SPI,
    /// Delay provider suitable for SD-card initialization.
    pub delay: DELAY,
    /// Static CoreS3 TF-card slot metadata.
    pub slot: CoreS3SdSlot,
}

impl<SPI, DELAY> CoreS3SdParts<SPI, DELAY>
where
    SPI: SpiDevice,
    DELAY: DelayNs,
{
    /// Convert these parts into an `embedded-sdmmc` SD-card block device.
    #[cfg(feature = "sdmmc")]
    pub fn into_sdmmc(self) -> embedded_sdmmc::SdCard<SPI, DELAY> {
        embedded_sdmmc::SdCard::new(self.spi_device, self.delay)
    }
}

/// Interpret CoreS3's raw AW9523B P0 input byte as TF-card presence.
pub const fn core_s3_card_present_from_aw9523_p0(value: u8) -> bool {
    SdCardDetect::AW9523B_P0_4_ACTIVE_LOW.present_from_port_value(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_detect_is_active_low_on_p0_bit4() {
        assert!(core_s3_card_present_from_aw9523_p0(0b1110_1111));
        assert!(!core_s3_card_present_from_aw9523_p0(0b0001_0000));
    }
}
