//! TF/microSD card metadata and helpers for CoreS3.
//!
//! CoreS3 routes the TF card over the same SPI signal group used by the LCD:
//! SCLK GPIO36, MOSI/COPI GPIO37, MISO/CIPO GPIO35, and CS GPIO4. Applications
//! that need both display and SD access should coordinate ownership of this
//! shared bus at the HAL layer. The card-detect switch is exposed through
//! AW9523B port 0 bit 4 and is active-low, matching the official M5Stack demo.

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
