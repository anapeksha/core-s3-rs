//! TF/microSD card metadata and helpers for CoreS3.
//!
//! CoreS3 routes the TF card over the same SPI signal group used by the LCD:
//! SCLK GPIO36, MOSI/COPI GPIO37, MISO/CIPO GPIO35, and CS GPIO4. Applications
//! that need both display and SD access should coordinate ownership of this
//! shared bus at the HAL layer. The card-detect switch is exposed through
//! AW9523B port 0 bit 4 and is active-low, matching the official M5Stack demo.

use embedded_hal::{delay::DelayNs, spi::SpiDevice};

use crate::pins::SpiSdPins;

/// GPIO35's logical role while CoreS3 LCD and TF-card share SPI2.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedGpio35Role {
    /// GPIO35 is released as the TF-card MISO input / safe idle role.
    SdMisoInput,
    /// GPIO35 is driven as LCD D/C while LCD CS is active.
    LcdDcOutput,
}

/// Pure CoreS3 shared-SPI invariant model used by host tests and docs.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedSpiLogicState {
    pub lcd_cs_low: bool,
    pub sd_cs_low: bool,
    pub gpio35: SharedGpio35Role,
}

impl SharedSpiLogicState {
    /// Safe CoreS3 shared-SPI idle state: both CS lines high and GPIO35 MISO-safe.
    pub const SAFE_IDLE: Self = Self {
        lcd_cs_low: false,
        sd_cs_low: false,
        gpio35: SharedGpio35Role::SdMisoInput,
    };

    /// Begin an LCD transaction: SD CS high, GPIO35 D/C output, LCD CS low.
    pub const fn begin_lcd(self) -> Self {
        Self {
            lcd_cs_low: true,
            sd_cs_low: false,
            gpio35: SharedGpio35Role::LcdDcOutput,
        }
    }

    /// Begin an SD transaction: LCD CS high, GPIO35 MISO-safe, SD CS low.
    pub const fn begin_sd(self) -> Self {
        Self {
            lcd_cs_low: false,
            sd_cs_low: true,
            gpio35: SharedGpio35Role::SdMisoInput,
        }
    }

    /// End or clean up either transaction into the BSP's safe idle state.
    pub const fn safe_idle(self) -> Self {
        Self::SAFE_IDLE
    }

    /// Whether the model satisfies CoreS3's LCD/SD shared-bus invariants.
    pub const fn invariants_hold(self) -> bool {
        !(self.lcd_cs_low && self.sd_cs_low)
            && if self.lcd_cs_low {
                matches!(self.gpio35, SharedGpio35Role::LcdDcOutput)
            } else {
                matches!(self.gpio35, SharedGpio35Role::SdMisoInput)
            }
    }
}

/// Pure state for CoreS3 SD command CS-framing over `embedded-sdmmc` calls.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SdCommandFramingState {
    pub selected_command: Option<u8>,
    pub trailing_single_response_bytes: u8,
    pub data_token_seen: bool,
    pub data_payload_seen: bool,
}

impl SdCommandFramingState {
    pub const IDLE: Self = Self {
        selected_command: None,
        trailing_single_response_bytes: 0,
        data_token_seen: false,
        data_payload_seen: false,
    };

    pub const fn command_write(self, command: u8) -> Self {
        Self {
            selected_command: Some(command),
            trailing_single_response_bytes: 0,
            data_token_seen: false,
            data_payload_seen: false,
        }
    }

    pub const fn one_byte_poll(self, byte: u8) -> Self {
        match self.selected_command {
            Some(_) if self.trailing_single_response_bytes > 0 => {
                if self.trailing_single_response_bytes == 1 {
                    Self::IDLE
                } else {
                    Self {
                        trailing_single_response_bytes: self.trailing_single_response_bytes - 1,
                        ..self
                    }
                }
            }
            Some(command) if command_has_data_block_const(command) && byte == 0xFE => Self {
                data_token_seen: true,
                ..self
            },
            Some(command) if (byte & 0x80) == 0 => {
                if command_has_single_byte_after_r1_const(command) {
                    Self {
                        trailing_single_response_bytes: 1,
                        ..self
                    }
                } else if !command_has_trailing_response_const(command)
                    && !command_has_data_block_const(command)
                {
                    Self::IDLE
                } else {
                    self
                }
            }
            _ => self,
        }
    }

    pub const fn transfer_in_place(self, len: usize) -> Self {
        match self.selected_command {
            Some(command) if command_has_trailing_response_const(command) => Self::IDLE,
            Some(command) if command_has_data_block_const(command) && self.data_token_seen => {
                if self.data_payload_seen && len == 2 {
                    Self::IDLE
                } else {
                    Self {
                        data_payload_seen: true,
                        ..self
                    }
                }
            }
            _ => self,
        }
    }
}

const fn command_has_trailing_response_const(command: u8) -> bool {
    matches!(command, 8 | 58)
}

const fn command_has_single_byte_after_r1_const(command: u8) -> bool {
    matches!(command, 13)
}

const fn command_has_data_block_const(command: u8) -> bool {
    matches!(command, 9 | 10 | 17 | 18 | 24 | 25)
}

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

    #[test]
    fn shared_spi_logic_starts_sd_miso_safe() {
        let state = SharedSpiLogicState::SAFE_IDLE;
        assert!(!state.lcd_cs_low);
        assert!(!state.sd_cs_low);
        assert_eq!(state.gpio35, SharedGpio35Role::SdMisoInput);
        assert!(state.invariants_hold());
    }

    #[test]
    fn lcd_transaction_forces_sd_deselected_and_restores_idle() {
        let active = SharedSpiLogicState::SAFE_IDLE.begin_lcd();
        assert_eq!(
            active,
            SharedSpiLogicState {
                lcd_cs_low: true,
                sd_cs_low: false,
                gpio35: SharedGpio35Role::LcdDcOutput,
            }
        );
        assert!(active.invariants_hold());
        assert_eq!(active.safe_idle(), SharedSpiLogicState::SAFE_IDLE);
    }

    #[test]
    fn sd_transaction_forces_lcd_deselected_and_restores_idle() {
        let active = SharedSpiLogicState::SAFE_IDLE.begin_sd();
        assert_eq!(
            active,
            SharedSpiLogicState {
                lcd_cs_low: false,
                sd_cs_low: true,
                gpio35: SharedGpio35Role::SdMisoInput,
            }
        );
        assert!(active.invariants_hold());
        assert_eq!(active.safe_idle(), SharedSpiLogicState::SAFE_IDLE);
    }

    #[test]
    fn repeated_lcd_sd_lcd_transitions_preserve_invariants() {
        let mut state = SharedSpiLogicState::SAFE_IDLE;
        for _ in 0..8 {
            state = state.begin_lcd();
            assert!(state.invariants_hold());
            state = state.safe_idle().begin_sd();
            assert!(state.invariants_hold());
            state = state.safe_idle();
            assert_eq!(state, SharedSpiLogicState::SAFE_IDLE);
        }
    }

    #[test]
    fn invalid_simultaneous_cs_state_is_rejected_by_model() {
        let invalid = SharedSpiLogicState {
            lcd_cs_low: true,
            sd_cs_low: true,
            gpio35: SharedGpio35Role::LcdDcOutput,
        };
        assert!(!invalid.invariants_hold());
    }

    #[test]
    fn cmd0_stays_selected_until_r1_response() {
        let state = SdCommandFramingState::IDLE.command_write(0);
        assert_eq!(state.selected_command, Some(0));
        let state = state.one_byte_poll(0xFF);
        assert_eq!(state.selected_command, Some(0));
        let state = state.one_byte_poll(0x01);
        assert_eq!(state, SdCommandFramingState::IDLE);
    }

    #[test]
    fn cmd17_closes_after_data_payload_and_crc() {
        let state = SdCommandFramingState::IDLE
            .command_write(17)
            .one_byte_poll(0x00)
            .one_byte_poll(0xFF)
            .one_byte_poll(0xFE);
        assert_eq!(state.selected_command, Some(17));
        assert!(state.data_token_seen);
        let state = state.transfer_in_place(512);
        assert_eq!(state.selected_command, Some(17));
        let state = state.transfer_in_place(2);
        assert_eq!(state, SdCommandFramingState::IDLE);
    }

    #[test]
    fn cmd8_and_cmd58_close_after_trailing_response() {
        for command in [8, 58] {
            let state = SdCommandFramingState::IDLE
                .command_write(command)
                .one_byte_poll(0x01)
                .transfer_in_place(4);
            assert_eq!(state, SdCommandFramingState::IDLE);
        }
    }

    #[test]
    fn cmd13_keeps_cs_for_second_status_byte() {
        let state = SdCommandFramingState::IDLE
            .command_write(13)
            .one_byte_poll(0x00);
        assert_eq!(state.selected_command, Some(13));
        assert_eq!(state.trailing_single_response_bytes, 1);
        let state = state.one_byte_poll(0x00);
        assert_eq!(state, SdCommandFramingState::IDLE);
    }

    #[test]
    fn cmd24_and_cmd25_remain_selected_for_data_phase() {
        for command in [24, 25] {
            let state = SdCommandFramingState::IDLE
                .command_write(command)
                .one_byte_poll(0x00);
            assert_eq!(state.selected_command, Some(command));
        }
    }
}
