//! Protocol-neutral ownership of the CoreS3 Wi-Fi and BLE controllers.
//!
//! Initialize the HAL and an upstream-compatible allocator first, then start
//! [`CoreS3Runtime`] through the BSP, and finally call
//! [`CoreS3::init_radio_coexistence`] with interrupts enabled. The caller must
//! provision sufficient internal RAM for the upstream scheduler, radio tasks,
//! controller buffers and allocations; PSRAM is not a substitute for allocations
//! that require internal RAM. This module neither installs nor sizes a heap.
//!
//! The `radio-coexistence` feature enables upstream `esp-radio` Wi-Fi, BLE and
//! `coex` support. This is controller initialization, not hardware coexistence
//! proof: simultaneous traffic, throughput, latency and RF behavior still need
//! validation on real CoreS3 hardware.
//!
//! Applications own Wi-Fi credentials, connection/start policy, IP stacks and
//! BLE host/protocol runtimes. BLE host connection/attribute/L2CAP pools and IP
//! stack/socket pools are separate from these controller configurations and must
//! be provisioned by the application. No host stack or pool is created here.
//! This wrapper logs no credentials or payloads; applications must likewise
//! avoid logging them and review upstream logging settings.

use esp_hal::peripherals::{BT, WIFI};
use esp_radio::{
    ble::{
        Config as BleConfig,
        controller::{BleConnector, BleInitError},
    },
    wifi::{Config as WifiConfig, ControllerConfig, Interface, WifiController, WifiError},
};

use crate::{CoreS3, rtos::CoreS3Runtime};

/// Move the unique HAL radio tokens into BSP initialization.
///
/// No tokens are cloned, stolen or returned on failure. Do not initialize the
/// same radios independently through upstream APIs.
pub struct CoreS3RadioPeripherals {
    pub wifi: WIFI<'static>,
    pub bt: BT<'static>,
}

/// Wi-Fi interface ownership and initial controller mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WifiInterfaceMode {
    /// Own only the station interface.
    #[default]
    Station,
    /// Own only the access-point interface.
    AccessPoint,
    /// Own both station and access-point interfaces.
    AccessPointStation,
}

/// Exact `esp-radio` 1.0.0-beta.1 controller configurations plus interface selection.
#[derive(Default)]
pub struct RadioCoexistenceConfig {
    /// Requested interfaces and initial mode; defaults to station only.
    pub interface_mode: WifiInterfaceMode,
    /// Controller buffers and tuning. `initial_config` is replaced with defaults
    /// for `interface_mode`; credentials and further configuration remain caller
    /// policy through the returned controller, before starting Wi-Fi.
    pub wifi: ControllerConfig,
    /// BLE controller settings, not BLE host-layer pool sizing.
    pub ble: BleConfig,
}

/// Independently movable upstream handles, without a BSP protocol runtime.
///
/// Only requested interfaces are `Some`. Keep the Wi-Fi controller alive while
/// using either interface. Dropping an interface releases its upstream singleton;
/// dropping the controller
/// deinitializes Wi-Fi unless upstream ESP-NOW/sniffer owners retain it.
/// Dropping the BLE connector deinitializes BLE and clears its queued HCI state.
/// Shared radio resources are reference-counted upstream. Stop application
/// users before dropping handles. No BSP global claim or custom drop is needed.
/// The runtime token is only startup evidence: dropping it does not stop RTOS.
pub struct CoreS3RadioParts {
    pub station: Option<Interface>,
    pub access_point: Option<Interface>,
    pub wifi_controller: WifiController<'static>,
    pub ble_connector: BleConnector<'static>,
}

/// Initialization failures, preserving the exact upstream error values.
#[derive(Debug)]
pub enum RadioInitError {
    /// Another owner already holds the upstream station interface singleton.
    StationInterfaceTaken,
    /// Another owner already holds the upstream access-point interface singleton.
    AccessPointInterfaceTaken,
    Wifi(WifiError),
    Ble(BleInitError),
}

impl CoreS3 {
    /// Claim requested interfaces, initialize Wi-Fi in that mode, then BLE.
    ///
    /// In simultaneous mode the station is claimed first. If claiming the access
    /// point fails, the station is released by RAII. Unrequested interfaces are
    /// neither claimed nor returned. Initial station/AP settings are upstream
    /// defaults, not application credentials or production AP policy.
    ///
    /// `runtime` proves that BSP startup has called `esp_rtos::start`; it is not
    /// consumed or retained, so the caller can subsequently run its executor.
    /// Wi-Fi is not started or connected, and no BLE host is started. Upstream
    /// coexistence is enabled by the crate feature, not a separate BSP service.
    ///
    /// On a returned error, successfully constructed local handles are dropped
    /// through upstream RAII. Peripheral tokens are consumed even on failure;
    /// this API does not promise retry or recover tokens. Upstream configuration
    /// validation or allocation can panic; call with interrupts enabled and
    /// satisfy upstream allocator/configuration requirements before entering.
    pub fn init_radio_coexistence(
        _runtime: &CoreS3Runtime,
        peripherals: CoreS3RadioPeripherals,
        config: RadioCoexistenceConfig,
    ) -> Result<CoreS3RadioParts, RadioInitError> {
        let station = match config.interface_mode {
            WifiInterfaceMode::Station | WifiInterfaceMode::AccessPointStation => {
                Some(Interface::try_station().ok_or(RadioInitError::StationInterfaceTaken)?)
            }
            WifiInterfaceMode::AccessPoint => None,
        };
        let access_point = match config.interface_mode {
            WifiInterfaceMode::AccessPoint | WifiInterfaceMode::AccessPointStation => Some(
                Interface::try_access_point().ok_or(RadioInitError::AccessPointInterfaceTaken)?,
            ),
            WifiInterfaceMode::Station => None,
        };
        let initial_config = match config.interface_mode {
            WifiInterfaceMode::Station => WifiConfig::Station(Default::default()),
            WifiInterfaceMode::AccessPoint => WifiConfig::AccessPoint(Default::default()),
            WifiInterfaceMode::AccessPointStation => {
                WifiConfig::AccessPointStation(Default::default(), Default::default())
            }
        };
        let wifi_config = config.wifi.with_initial_config(initial_config);
        let wifi_controller =
            WifiController::new(peripherals.wifi, wifi_config).map_err(RadioInitError::Wifi)?;
        let ble_connector =
            BleConnector::new(peripherals.bt, config.ble).map_err(RadioInitError::Ble)?;
        Ok(CoreS3RadioParts {
            station,
            access_point,
            wifi_controller,
            ble_connector,
        })
    }
}
