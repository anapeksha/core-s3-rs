//! Matter-over-Thread scaffolding for the Gateway H2 stack.
//!
//! The Gateway H2 provides the IEEE 802.15.4/Thread-capable ESP32-H2 side of the
//! stack. `rs-matter` provides the Matter application/session/data-model layers.
//! This module ties those two pieces together at the BSP boundary by exposing the
//! Gateway H2 transport metadata and commissioning/server configuration a firmware
//! crate needs before it instantiates its concrete Matter server.

pub use rs_matter as stack;

use heapless::String;

use super::GatewayH2;

/// Default Matter secure port.
pub const DEFAULT_MATTER_PORT: u16 = 5540;

/// Default Matter setup discriminator length in decimal digits.
pub const SETUP_DISCRIMINATOR_DIGITS: u8 = 12;

/// Configuration needed to advertise and commission a Matter node over Thread.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatterServerConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub discriminator: u16,
    pub setup_passcode: u32,
    pub device_name: String<32>,
    pub port: u16,
}

impl MatterServerConfig {
    pub fn new(
        vendor_id: u16,
        product_id: u16,
        discriminator: u16,
        setup_passcode: u32,
        device_name: &str,
    ) -> Self {
        let mut name = String::new();
        let _ = name.push_str(device_name);

        Self {
            vendor_id,
            product_id,
            discriminator,
            setup_passcode,
            device_name: name,
            port: DEFAULT_MATTER_PORT,
        }
    }

    pub const fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

/// Thread dataset values a firmware can provision through the Gateway H2
/// co-processor before starting `rs-matter` networking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadDatasetConfig {
    pub network_name: String<16>,
    pub pan_id: u16,
    pub extended_pan_id: [u8; 8],
    pub channel: u8,
    pub network_key: [u8; 16],
}

impl ThreadDatasetConfig {
    pub fn new(
        network_name: &str,
        pan_id: u16,
        extended_pan_id: [u8; 8],
        channel: u8,
        network_key: [u8; 16],
    ) -> Self {
        let mut name = String::new();
        let _ = name.push_str(network_name);

        Self {
            network_name: name,
            pan_id,
            extended_pan_id,
            channel,
            network_key,
        }
    }
}

/// Combined BSP-level Matter-over-Thread setup description.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatterOverThreadConfig {
    pub gateway: GatewayH2,
    pub matter: MatterServerConfig,
    pub thread: ThreadDatasetConfig,
}

impl MatterOverThreadConfig {
    pub const fn new(
        gateway: GatewayH2,
        matter: MatterServerConfig,
        thread: ThreadDatasetConfig,
    ) -> Self {
        Self {
            gateway,
            matter,
            thread,
        }
    }
}
