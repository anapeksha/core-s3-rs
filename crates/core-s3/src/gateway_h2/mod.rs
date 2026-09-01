pub mod matter;

use crate::pins::{Gpio, GrovePortPins};

/// Feature-gated support metadata for an M5Stack Zigbee Gateway H2 stack.
///
/// Enable with `--features gateway-h2`. The board presents the ESP32-H2 module
/// to CoreS3 firmware as an external co-processor; high-level protocol drivers
/// can build on top of this pin/bus description without making all users pay for
/// Zigbee-related code.
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
