pub mod matter;

use crate::pins::{Gpio, GrovePortPins};

/// Feature-gated support metadata for an M5Stack Gateway H2 stack.
///
/// Enable with `--features gateway-h2`. Gateway H2 is an ESP32-H2
/// IEEE 802.15.4 co-processor/device. Depending on the firmware flashed to the
/// H2 it may expose an OpenThread CLI, an OpenThread RCP/Spinel transport, or a
/// standalone application; it is not assumed to be an AT-command modem.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GatewayH2 {
    pub host_uart: UartPins,
    pub reset: Option<Gpio>,
    pub boot: Option<Gpio>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UartPins {
    pub tx: Gpio,
    pub rx: Gpio,
}

impl GatewayH2 {
    /// Default Grove-port UART wiring. Validate against your exact base/stack
    /// revision before enabling bootloader-reset automation.
    pub const GROVE_UART: Self = Self {
        host_uart: UartPins {
            tx: GrovePortPins::PORT_A.pin1,
            rx: GrovePortPins::PORT_A.pin2,
        },
        reset: None,
        boot: None,
    };
}
