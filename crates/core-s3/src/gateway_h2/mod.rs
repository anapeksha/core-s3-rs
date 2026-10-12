pub mod matter;
pub mod spinel;
pub mod transport;

use crate::pins::Gpio;

/// Feature-gated metadata for the officially stacked M5Stack Module Gateway H2.
///
/// UART, EN, and BOOT are fixed M-Bus routes. SPI and PTA are optional physical
/// DIP-switch routes and are not detected by software.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GatewayH2 {
    pub host_uart: UartPins,
    pub enable: Gpio,
    pub boot: Gpio,
    pub optional: GatewayH2OptionalPins,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UartPins {
    pub tx: Gpio,
    pub rx: Gpio,
}

/// Optional DIP-routed Gateway H2 signals and their CoreS3 conflicts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GatewayH2OptionalPins {
    pub spi_mosi: Gpio,
    pub spi_miso: Gpio,
    pub spi_clock: Gpio,
    pub spi_cs: Gpio,
    pub wl_active: Gpio,
    pub bt_priority: Gpio,
    pub bt_active: Gpio,
}

impl GatewayH2 {
    /// Hardware-validated CoreS3 + stacked Module Gateway H2 v0.4 mapping.
    pub const STACKED: Self = Self {
        host_uart: UartPins {
            tx: Gpio(17),
            rx: Gpio(10),
        },
        enable: Gpio(7),
        boot: Gpio(18),
        optional: GatewayH2OptionalPins {
            spi_mosi: Gpio(37),
            spi_miso: Gpio(35),
            spi_clock: Gpio(36),
            spi_cs: Gpio(13),
            wl_active: Gpio(0),
            bt_priority: Gpio(5),
            bt_active: Gpio(6),
        },
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_mapping_matches_hardware_validated_route() {
        assert_eq!(GatewayH2::STACKED.host_uart.tx, Gpio(17));
        assert_eq!(GatewayH2::STACKED.host_uart.rx, Gpio(10));
        assert_eq!(GatewayH2::STACKED.enable, Gpio(7));
        assert_eq!(GatewayH2::STACKED.boot, Gpio(18));
        assert_eq!(GatewayH2::STACKED.optional.spi_miso, Gpio(35));
        assert_eq!(GatewayH2::STACKED.optional.wl_active, Gpio(0));
    }
}
