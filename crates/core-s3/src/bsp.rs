//! ESP-HAL board bring-up helpers for M5Stack CoreS3.
//!
//! These helpers own only the resources needed for the requested peripheral.
//! `CoreS3DisplayResources` consumes the LCD SPI pins plus the internal I2C
//! bus used to configure AXP2101/AW9523B display power, reset, and backlight;
//! all other ESP peripherals remain with the application.

use core::{cell::RefCell, convert::Infallible};

use critical_section::Mutex;
use embedded_hal::{
    delay::DelayNs,
    digital::{ErrorType as DigitalErrorType, OutputPin},
    i2c::I2c,
    spi::{Error as SpiErrorTrait, ErrorKind as SpiErrorKind, Operation, SpiBus, SpiDevice},
};
use embedded_hal_bus::spi;
use esp_hal::{
    Blocking,
    delay::Delay,
    gpio::{AnyPin, Flex, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config as I2cConfig, I2c as EspI2c},
    spi::{
        Mode,
        master::{Config as SpiConfig, ConfigError as SpiConfigError, Spi},
    },
    time::Rate,
    uart::{Config as UartConfig, Uart},
};

#[cfg(feature = "gateway-h2")]
use crate::gateway_h2::transport::GatewayH2OpenThreadConfig;
use crate::{
    CoreS3, devices,
    display::{BusConfig, Display, DisplayError, DisplayGeometry, PanelConfig},
    sd::{CoreS3SdParts, CoreS3SdSlot},
};

/// Display SPI write frequency used by M5GFX for CoreS3 after autodetection.
pub const DISPLAY_SPI_WRITE_HZ: u32 = 40_000_000;
/// CoreS3 internal I2C frequency.
pub const INTERNAL_I2C_HZ: u32 = 400_000;
/// Default CoreS3-to-Gateway-H2 UART baud rate.
pub const GATEWAY_H2_UART_BAUD: u32 = 115_200;

const AXP_LDOS_ON_OFF: u8 = 0x90;
const AXP_ALDO3_VOLTAGE: u8 = 0x94;
const AXP_ALDO4_VOLTAGE: u8 = 0x95;
const AXP_DLDO1_VOLTAGE: u8 = 0x99;
const AXP_LDO_3V3_CODE: u8 = 33 - 5;
const AXP_CORES3_LDO_ENABLE_MASK: u8 = 0xBF;
const AXP_ALDO4_ENABLE_BIT: u8 = 1 << 3;
const SD_SPI_INIT_HZ: u32 = 400_000;
const SD_INIT_CLOCKS: [u8; 10] = [0xFF; 10];
const AW_OUTPUT_P0: u8 = 0x02;
const AW_OUTPUT_P1: u8 = 0x03;
const AW_CONFIG_P0: u8 = 0x04;
const AW_CONFIG_P1: u8 = 0x05;
const AW_GLOBAL_CONTROL: u8 = 0x11;
const AW_LED_MODE_P0: u8 = 0x12;
const AW_LED_MODE_P1: u8 = 0x13;
const AW_LCD_RESET_BIT: u8 = 1 << 1;

/// Concrete blocking I2C bus used by CoreS3 internal devices.
pub type CoreS3I2c = EspI2c<'static, Blocking>;
/// Concrete blocking SPI bus used by the CoreS3 LCD.
pub type CoreS3RawSpi = Spi<'static, Blocking>;
/// Concrete ESP-HAL output pin type used by CoreS3 helpers.
pub type CoreS3Output = Output<'static>;
/// SPI device wrapper used by the legacy exclusive CoreS3 LCD initializer.
pub type CoreS3LcdSpiDevice = spi::ExclusiveDevice<CoreS3RawSpi, CoreS3Output, Delay>;
/// Concrete display type returned by [`CoreS3::init_display`].
pub type CoreS3Display = Display<CoreS3LcdSpiDevice, CoreS3Output, CoreS3Output>;
/// Shared SPI device wrapper for the CoreS3 LCD chip select.
pub type CoreS3SharedLcdSpiDevice =
    spi::CriticalSectionDevice<'static, CoreS3RawSpi, CoreS3Output, Delay>;
/// Shared SPI device wrapper for the CoreS3 TF-card chip select.
pub type CoreS3SharedSdSpiDevice = CoreS3SharedSdDevice;
/// Display type returned by [`CoreS3::init_display_on_shared_spi`].
pub type CoreS3SharedDisplay = Display<CoreS3SharedLcdSpiDevice, CoreS3SharedDc, NoSdCsGuard>;
/// SD parts returned by [`CoreS3::init_sd_on_shared_spi`].
pub type CoreS3EspHalSdParts = CoreS3SdParts<CoreS3SharedSdSpiDevice, Delay>;
/// Concrete blocking UART used for the Gateway H2 host link.
pub type CoreS3GatewayH2Uart = Uart<'static, Blocking>;

/// ESP-HAL resources required to initialize the shared LCD/TF SPI bus.
///
/// CoreS3 routes LCD writes and TF-card access through the same SPI signal group.
/// GPIO35 is physically shared between LCD D/C during display writes and SD MISO
/// during card reads. The shared initializer consumes GPIO35 as SPI MISO, then
/// keeps a BSP-private D/C controller that safely disables the output driver
/// around SD transactions and restores it before future LCD writes.
pub struct CoreS3SharedSpiResources {
    pub spi2: esp_hal::peripherals::SPI2<'static>,
    pub sclk: esp_hal::peripherals::GPIO36<'static>,
    pub mosi: esp_hal::peripherals::GPIO37<'static>,
    pub miso: esp_hal::peripherals::GPIO35<'static>,
}

/// Shared SPI bus holder used to create chip-select scoped LCD/TF devices.
///
/// This type owns the SPI bus configured with SCLK GPIO36, MOSI GPIO37, and
/// MISO GPIO35. It also owns the BSP-private GPIO35 D/C controller used by
/// [`CoreS3SharedDc`] and [`CoreS3SharedSdDevice`] so downstream applications do
/// not need to alias or manually reconfigure the shared pad.
pub struct CoreS3SharedSpiParts {
    bus: Mutex<RefCell<CoreS3RawSpi>>,
    lcd_dc: Mutex<RefCell<Flex<'static>>>,
    /// CoreS3 TF-card slot metadata, including the physical MISO/DC GPIO.
    pub sd_slot: CoreS3SdSlot,
}

/// ESP-HAL resources required to initialize CoreS3's internal I2C bus.
pub struct CoreS3InternalI2cResources {
    pub i2c0: esp_hal::peripherals::I2C0<'static>,
    pub i2c_sda: esp_hal::peripherals::GPIO12<'static>,
    pub i2c_scl: esp_hal::peripherals::GPIO11<'static>,
}

/// ESP-HAL resources required to initialize the CoreS3 LCD.
pub struct CoreS3DisplayResources {
    pub i2c0: esp_hal::peripherals::I2C0<'static>,
    pub i2c_sda: esp_hal::peripherals::GPIO12<'static>,
    pub i2c_scl: esp_hal::peripherals::GPIO11<'static>,
    pub spi2: esp_hal::peripherals::SPI2<'static>,
    pub lcd_sclk: esp_hal::peripherals::GPIO36<'static>,
    pub lcd_mosi: esp_hal::peripherals::GPIO37<'static>,
    /// Shared LCD D/C and SPI MISO pad. The display write path drives it as D/C.
    pub lcd_dc: esp_hal::peripherals::GPIO35<'static>,
    pub lcd_cs: esp_hal::peripherals::GPIO3<'static>,
    /// TF-card CS on the same physical SPI bus. It is held high while using LCD.
    pub tf_card_cs: esp_hal::peripherals::GPIO4<'static>,
}

/// Initialized display plus the internal I2C bus used during bring-up.
pub struct CoreS3DisplayParts {
    pub display: CoreS3Display,
    pub internal_i2c: CoreS3I2c,
}

/// ESP-HAL resources required to initialize the display on an already-created shared SPI bus.
pub struct CoreS3DisplayOnSharedSpiResources {
    pub shared_spi: &'static CoreS3SharedSpiParts,
    pub i2c0: esp_hal::peripherals::I2C0<'static>,
    pub i2c_sda: esp_hal::peripherals::GPIO12<'static>,
    pub i2c_scl: esp_hal::peripherals::GPIO11<'static>,
    pub lcd_cs: esp_hal::peripherals::GPIO3<'static>,
}

/// ESP-HAL resources required to initialize the display after CoreS3 power setup.
pub struct CoreS3DisplayOnPoweredSharedSpiResources {
    pub shared_spi: &'static CoreS3SharedSpiParts,
    pub internal_i2c: CoreS3I2c,
    pub lcd_cs: esp_hal::peripherals::GPIO3<'static>,
}

/// Initialized shared-SPI display plus the internal I2C bus used during bring-up.
pub struct CoreS3SharedDisplayParts {
    pub display: CoreS3SharedDisplay,
    pub internal_i2c: CoreS3I2c,
}

/// ESP-HAL resources required to create an SD-card SPI device on the shared bus.
pub struct CoreS3SdOnSharedSpiResources {
    pub shared_spi: &'static CoreS3SharedSpiParts,
    pub tf_card_cs: esp_hal::peripherals::GPIO4<'static>,
}

/// ESP-HAL resources required to initialize the Gateway H2 UART link.
pub struct CoreS3GatewayH2Resources {
    pub uart1: esp_hal::peripherals::UART1<'static>,
    /// CoreS3 Grove Port A pin 1, used as host UART TX.
    pub tx: esp_hal::peripherals::GPIO1<'static>,
    /// CoreS3 Grove Port A pin 2, used as host UART RX.
    pub rx: esp_hal::peripherals::GPIO2<'static>,
}

/// Initialized Gateway H2 host UART link.
pub struct CoreS3GatewayH2Parts {
    pub uart: CoreS3GatewayH2Uart,
    pub baud: u32,
}

/// Gateway H2 OpenThread/Spinel-oriented UART parts.
#[cfg(feature = "gateway-h2")]
pub struct CoreS3GatewayH2OpenThreadParts<T = CoreS3GatewayH2Uart> {
    pub transport: T,
    pub max_frame_size: usize,
    pub baud: u32,
    pub config: GatewayH2OpenThreadConfig,
}

/// LCD D/C output facade for CoreS3 shared-SPI display writes.
///
/// Setting D/C high or low also restores GPIO35 as a driven output. SD access
/// should go through [`CoreS3SharedSdDevice`], which releases the output driver
/// while TF-card CS is active.
pub struct CoreS3SharedDc {
    pin: &'static Mutex<RefCell<Flex<'static>>>,
}

impl CoreS3SharedDc {
    fn new(shared_spi: &'static CoreS3SharedSpiParts) -> Self {
        Self {
            pin: &shared_spi.lcd_dc,
        }
    }
}

impl DigitalErrorType for CoreS3SharedDc {
    type Error = Infallible;
}

impl OutputPin for CoreS3SharedDc {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        critical_section::with(|cs| {
            let mut pin = self.pin.borrow_ref_mut(cs);
            pin.set_low();
            pin.apply_output_config(&OutputConfig::default());
            pin.set_output_enable(true);
        });
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        critical_section::with(|cs| {
            let mut pin = self.pin.borrow_ref_mut(cs);
            pin.set_high();
            pin.apply_output_config(&OutputConfig::default());
            pin.set_output_enable(true);
        });
        Ok(())
    }
}

/// SPI error for the CoreS3 shared TF-card device wrapper.
#[derive(Debug)]
pub enum CoreS3SharedSdSpiError {
    Spi(esp_hal::spi::Error),
    Config(SpiConfigError),
    ChipSelect,
}

impl SpiErrorTrait for CoreS3SharedSdSpiError {
    fn kind(&self) -> SpiErrorKind {
        match self {
            Self::Spi(error) => SpiErrorTrait::kind(error),
            Self::Config(_) => SpiErrorKind::Other,
            Self::ChipSelect => SpiErrorKind::ChipSelectFault,
        }
    }
}

/// CoreS3-specific SD `SpiDevice` for the shared LCD/TF SPI bus.
///
/// Each SD transaction switches GPIO35 from LCD D/C output to SD MISO input
/// before asserting TF-card CS and leaves it as an input afterward. The LCD D/C
/// facade restores output mode only when the display actually writes. SD
/// command packets are handled specially for `embedded-sdmmc` so CS remains
/// asserted across command writes, R1 polling, and immediate response/data
/// bytes used during card acquisition and capacity probing.
pub struct CoreS3SharedSdDevice {
    bus: &'static Mutex<RefCell<CoreS3RawSpi>>,
    lcd_dc: &'static Mutex<RefCell<Flex<'static>>>,
    cs: CoreS3Output,
    delay: Delay,
    selected_command: Option<u8>,
    data_token_seen: bool,
    data_payload_seen: bool,
}

impl CoreS3SharedSdDevice {
    fn new(shared_spi: &'static CoreS3SharedSpiParts, cs: CoreS3Output) -> Self {
        Self {
            bus: &shared_spi.bus,
            lcd_dc: &shared_spi.lcd_dc,
            cs,
            delay: Delay::new(),
            selected_command: None,
            data_token_seen: false,
            data_payload_seen: false,
        }
    }

    /// Prepare the shared CoreS3 TF-card SPI path for card acquisition.
    ///
    /// This leaves TF-card CS high, releases GPIO35 as a pulled-up MISO input,
    /// applies SPI mode 0 at 400 kHz, and sends at least 80 idle clocks with
    /// MOSI high before `embedded-sdmmc` starts issuing commands.
    pub fn prepare_for_card_acquire(&mut self) -> Result<(), CoreS3SharedSdSpiError> {
        critical_section::with(|cs| {
            release_gpio35_for_sd(self.lcd_dc, cs);

            let mut bus = self.bus.borrow_ref_mut(cs);
            bus.apply_config(&sd_spi_config())
                .map_err(CoreS3SharedSdSpiError::Config)?;
            OutputPin::set_high(&mut self.cs).map_err(|_| CoreS3SharedSdSpiError::ChipSelect)?;
            self.selected_command = None;
            self.data_token_seen = false;
            self.data_payload_seen = false;
            self.delay.delay_ms(10);
            SpiBus::write(&mut *bus, &SD_INIT_CLOCKS).map_err(CoreS3SharedSdSpiError::Spi)?;
            SpiBus::flush(&mut *bus).map_err(CoreS3SharedSdSpiError::Spi)
        })
    }

    fn finish_selected_command(
        &mut self,
        bus: &mut CoreS3RawSpi,
    ) -> Result<(), CoreS3SharedSdSpiError> {
        OutputPin::set_high(&mut self.cs).map_err(|_| CoreS3SharedSdSpiError::ChipSelect)?;
        SpiBus::write(bus, &[0xFF]).map_err(CoreS3SharedSdSpiError::Spi)?;
        SpiBus::flush(bus).map_err(CoreS3SharedSdSpiError::Spi)?;
        self.selected_command = None;
        self.data_token_seen = false;
        self.data_payload_seen = false;
        Ok(())
    }
}

impl embedded_hal::spi::ErrorType for CoreS3SharedSdDevice {
    type Error = CoreS3SharedSdSpiError;
}

impl SpiDevice for CoreS3SharedSdDevice {
    fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
        critical_section::with(|cs| {
            release_gpio35_for_sd(self.lcd_dc, cs);

            let mut bus = self.bus.borrow_ref_mut(cs);
            bus.apply_config(&sd_spi_config())
                .map_err(CoreS3SharedSdSpiError::Config)?;
            if self.selected_command.is_some() {
                self.finish_selected_command(&mut bus)?;
            }
            OutputPin::set_low(&mut self.cs).map_err(|_| CoreS3SharedSdSpiError::ChipSelect)?;

            let mut result = Ok(());
            for operation in operations {
                result = match operation {
                    Operation::Read(buffer) => SpiBus::read(&mut *bus, buffer),
                    Operation::Write(buffer) => SpiBus::write(&mut *bus, buffer),
                    Operation::Transfer(read, write) => SpiBus::transfer(&mut *bus, read, write),
                    Operation::TransferInPlace(buffer) => {
                        SpiBus::transfer_in_place(&mut *bus, buffer)
                    }
                    Operation::DelayNs(ns) => {
                        self.delay.delay_ns(*ns);
                        Ok(())
                    }
                };
                if result.is_err() {
                    break;
                }
            }

            let flush_result = if result.is_ok() {
                SpiBus::flush(&mut *bus)
            } else {
                Ok(())
            };
            let cs_result = OutputPin::set_high(&mut self.cs);
            let trailing_clock_result =
                if result.is_ok() && flush_result.is_ok() && cs_result.is_ok() {
                    SpiBus::write(&mut *bus, &[0xFF]).and_then(|()| SpiBus::flush(&mut *bus))
                } else {
                    Ok(())
                };

            self.selected_command = None;
            self.data_token_seen = false;
            self.data_payload_seen = false;
            result
                .and(flush_result)
                .and(trailing_clock_result)
                .map_err(CoreS3SharedSdSpiError::Spi)?;
            cs_result.map_err(|_| CoreS3SharedSdSpiError::ChipSelect)
        })
    }

    fn write(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        if let Some(command) = sd_command(buf) {
            return critical_section::with(|cs| {
                release_gpio35_for_sd(self.lcd_dc, cs);

                let mut bus = self.bus.borrow_ref_mut(cs);
                bus.apply_config(&sd_spi_config())
                    .map_err(CoreS3SharedSdSpiError::Config)?;
                if self.selected_command.is_some() {
                    self.finish_selected_command(&mut bus)?;
                }

                OutputPin::set_low(&mut self.cs).map_err(|_| CoreS3SharedSdSpiError::ChipSelect)?;
                self.selected_command = Some(command);
                self.data_token_seen = false;
                self.data_payload_seen = false;
                SpiBus::write(&mut *bus, buf).map_err(CoreS3SharedSdSpiError::Spi)?;
                SpiBus::flush(&mut *bus).map_err(CoreS3SharedSdSpiError::Spi)
            });
        }

        if self.selected_command.is_some() {
            return critical_section::with(|cs| {
                release_gpio35_for_sd(self.lcd_dc, cs);
                let mut bus = self.bus.borrow_ref_mut(cs);
                bus.apply_config(&sd_spi_config())
                    .map_err(CoreS3SharedSdSpiError::Config)?;
                SpiBus::write(&mut *bus, buf).map_err(CoreS3SharedSdSpiError::Spi)?;
                SpiBus::flush(&mut *bus).map_err(CoreS3SharedSdSpiError::Spi)
            });
        }

        self.transaction(&mut [Operation::Write(buf)])
    }

    fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
        if let Some(command) = self.selected_command {
            return critical_section::with(|cs| {
                release_gpio35_for_sd(self.lcd_dc, cs);

                let mut bus = self.bus.borrow_ref_mut(cs);
                bus.apply_config(&sd_spi_config())
                    .map_err(CoreS3SharedSdSpiError::Config)?;
                SpiBus::transfer(&mut *bus, read, write).map_err(CoreS3SharedSdSpiError::Spi)?;
                SpiBus::flush(&mut *bus).map_err(CoreS3SharedSdSpiError::Spi)?;

                if read.len() == 1 && write == [0xFF] {
                    if command_has_data_block(command) && read[0] == 0xFE {
                        self.data_token_seen = true;
                    } else if (read[0] & 0x80) == 0
                        && !command_has_trailing_response(command)
                        && !command_has_data_block(command)
                    {
                        self.finish_selected_command(&mut bus)?;
                    }
                }

                Ok(())
            });
        }

        self.transaction(&mut [Operation::Transfer(read, write)])
    }

    fn transfer_in_place(&mut self, buf: &mut [u8]) -> Result<(), Self::Error> {
        if let Some(command) = self.selected_command {
            return critical_section::with(|cs| {
                release_gpio35_for_sd(self.lcd_dc, cs);

                let mut bus = self.bus.borrow_ref_mut(cs);
                bus.apply_config(&sd_spi_config())
                    .map_err(CoreS3SharedSdSpiError::Config)?;
                SpiBus::transfer_in_place(&mut *bus, buf).map_err(CoreS3SharedSdSpiError::Spi)?;
                SpiBus::flush(&mut *bus).map_err(CoreS3SharedSdSpiError::Spi)?;

                if command_has_trailing_response(command) {
                    self.finish_selected_command(&mut bus)?;
                } else if command_has_data_block(command) && self.data_token_seen {
                    if self.data_payload_seen && buf.len() == 2 {
                        self.finish_selected_command(&mut bus)?;
                    } else {
                        self.data_payload_seen = true;
                    }
                }

                Ok(())
            });
        }

        self.transaction(&mut [Operation::TransferInPlace(buf)])
    }
}

pub struct NoSdCsGuard;

impl DigitalErrorType for NoSdCsGuard {
    type Error = Infallible;
}

impl OutputPin for NoSdCsGuard {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardInitError {
    I2c,
    Spi,
    Power,
    Display,
    Uart,
    Sd,
    SharedPin,
}

impl CoreS3 {
    /// Initializes the shared SPI bus used by the CoreS3 LCD and TF-card slot.
    ///
    /// The returned parts should usually be stored in a `static_cell::StaticCell`
    /// by downstream firmware, then borrowed as `&'static CoreS3SharedSpiParts`
    /// when creating display and SD devices. The SPI peripheral is configured
    /// with GPIO35 as MISO, matching M5Stack's CoreS3 PinMap. The BSP also keeps
    /// an internal GPIO35 D/C controller for the LCD path, but leaves GPIO35 as a
    /// pulled-up SD MISO input until the display actually drives D/C.
    pub fn init_shared_spi(
        resources: CoreS3SharedSpiResources,
    ) -> Result<CoreS3SharedSpiParts, BoardInitError> {
        let spi = configure_shared_spi(
            resources.spi2,
            resources.sclk,
            resources.mosi,
            resources.miso,
        )?;

        // SAFETY: CoreS3 intentionally wires GPIO35 to two mutually-exclusive roles:
        // LCD D/C output while LCD CS is active, and TF-card MISO input while TF CS
        // is active. `configure_shared_spi` consumes the safe GPIO35 token to attach
        // it to the SPI MISO input matrix. This BSP-private alias is used only to
        // control the GPIO output-enable/level for LCD D/C. `CoreS3SharedSdDevice`
        // disables the GPIO output driver before every SD transaction and leaves it
        // released afterward, so downstream firmware never receives aliased pin
        // tokens or has to perform unsafe mode switching.
        #[allow(unsafe_code)]
        let mut lcd_dc = Flex::new(unsafe { AnyPin::steal(35) });
        lcd_dc.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
        lcd_dc.set_input_enable(true);
        lcd_dc.set_output_enable(false);

        Ok(CoreS3SharedSpiParts {
            bus: Mutex::new(RefCell::new(spi)),
            lcd_dc: Mutex::new(RefCell::new(lcd_dc)),
            sd_slot: CoreS3SdSlot::CORE_S3,
        })
    }

    /// Initializes CoreS3's internal I2C bus for PMU/AW9523B/touch/sensor use.
    pub fn init_internal_i2c(
        resources: CoreS3InternalI2cResources,
    ) -> Result<CoreS3I2c, BoardInitError> {
        configure_i2c(resources.i2c0, resources.i2c_sda, resources.i2c_scl)
    }

    /// Initializes CoreS3 board power rails and display reset/backlight defaults.
    pub fn init_core_s3_power(i2c: &mut CoreS3I2c) -> Result<(), BoardInitError> {
        let mut delay = Delay::new();
        init_display_power(i2c, &mut delay).map_err(|_| BoardInitError::Power)
    }

    /// Power-cycles the AXP2101 ALDO4 rail that feeds the CoreS3 TF-card slot.
    ///
    /// This helper uses read-modify-write on AXP2101 register `0x90` so it does
    /// not disturb unrelated LDO rails. The off/on delays match the timings
    /// validated on CoreS3 hardware for cards inserted before boot/reset/flash.
    pub fn power_cycle_tf_card_rail(i2c: &mut CoreS3I2c) -> Result<(), BoardInitError> {
        let mut delay = Delay::new();
        let ldo_enable = read_register(i2c, devices::i2c::AXP2101_PMU, AXP_LDOS_ON_OFF)
            .map_err(|_| BoardInitError::Power)?;
        write_register(
            i2c,
            devices::i2c::AXP2101_PMU,
            AXP_LDOS_ON_OFF,
            ldo_enable & !AXP_ALDO4_ENABLE_BIT,
        )
        .map_err(|_| BoardInitError::Power)?;
        delay.delay_ms(500);
        write_register(
            i2c,
            devices::i2c::AXP2101_PMU,
            AXP_ALDO4_VOLTAGE,
            AXP_LDO_3V3_CODE,
        )
        .map_err(|_| BoardInitError::Power)?;
        write_register(
            i2c,
            devices::i2c::AXP2101_PMU,
            AXP_LDOS_ON_OFF,
            ldo_enable | AXP_ALDO4_ENABLE_BIT,
        )
        .map_err(|_| BoardInitError::Power)?;
        delay.delay_ms(750);
        Ok(())
    }

    /// Initializes only the CoreS3 display path and returns the initialized LCD
    /// plus the internal I2C bus for later PMU/touch/sensor use.
    pub fn init_display(
        resources: CoreS3DisplayResources,
    ) -> Result<CoreS3DisplayParts, BoardInitError> {
        let mut delay = Delay::new();
        let mut i2c = configure_i2c(resources.i2c0, resources.i2c_sda, resources.i2c_scl)?;
        init_display_power(&mut i2c, &mut delay).map_err(|_| BoardInitError::Power)?;

        let spi = configure_lcd_spi(resources.spi2, resources.lcd_sclk, resources.lcd_mosi)?;
        let cs = Output::new(resources.lcd_cs, Level::High, OutputConfig::default());
        let dc = Output::new(resources.lcd_dc, Level::Low, OutputConfig::default());
        let tf_card_cs = Output::new(resources.tf_card_cs, Level::High, OutputConfig::default());
        let spi_device =
            spi::ExclusiveDevice::new(spi, cs, Delay::new()).map_err(|_| BoardInitError::Spi)?;

        let mut display = Display::new(
            spi_device,
            dc,
            tf_card_cs,
            BusConfig {
                write_hz: DISPLAY_SPI_WRITE_HZ,
            },
            PanelConfig {
                invert_colors: true,
                geometry: DisplayGeometry {
                    width: devices::display::WIDTH,
                    height: devices::display::HEIGHT,
                    offset_x: 0,
                    offset_y: 0,
                },
            },
        );
        display
            .init(&mut delay)
            .map_err(|_: DisplayError<_, _>| BoardInitError::Display)?;

        Ok(CoreS3DisplayParts {
            display,
            internal_i2c: i2c,
        })
    }

    /// Initializes the CoreS3 display using a previously-created shared SPI bus.
    ///
    /// This initializer does not consume TF-card CS, so applications can also call
    /// [`Self::init_sd_on_shared_spi`] with the same shared bus. It preserves the
    /// internal I2C bring-up behavior needed for display power/reset/backlight.
    pub fn init_display_on_shared_spi(
        resources: CoreS3DisplayOnSharedSpiResources,
    ) -> Result<CoreS3SharedDisplayParts, BoardInitError> {
        let mut i2c = configure_i2c(resources.i2c0, resources.i2c_sda, resources.i2c_scl)?;
        Self::init_core_s3_power(&mut i2c)?;
        Self::init_display_on_powered_shared_spi(CoreS3DisplayOnPoweredSharedSpiResources {
            shared_spi: resources.shared_spi,
            internal_i2c: i2c,
            lcd_cs: resources.lcd_cs,
        })
    }

    /// Initializes the CoreS3 display after the internal power rails are ready.
    ///
    /// Use this helper when firmware needs to initialize and probe the TF card
    /// before any LCD SPI traffic. The caller supplies the already-initialized
    /// internal I2C bus so it remains available after display bring-up.
    pub fn init_display_on_powered_shared_spi(
        resources: CoreS3DisplayOnPoweredSharedSpiResources,
    ) -> Result<CoreS3SharedDisplayParts, BoardInitError> {
        let mut delay = Delay::new();
        let cs = Output::new(resources.lcd_cs, Level::High, OutputConfig::default());
        let dc = CoreS3SharedDc::new(resources.shared_spi);
        let spi_device =
            spi::CriticalSectionDevice::new(&resources.shared_spi.bus, cs, Delay::new())
                .map_err(|_| BoardInitError::Spi)?;

        let mut display = Display::new(
            spi_device,
            dc,
            NoSdCsGuard,
            BusConfig {
                write_hz: DISPLAY_SPI_WRITE_HZ,
            },
            PanelConfig {
                invert_colors: true,
                geometry: DisplayGeometry {
                    width: devices::display::WIDTH,
                    height: devices::display::HEIGHT,
                    offset_x: 0,
                    offset_y: 0,
                },
            },
        );
        display
            .init(&mut delay)
            .map_err(|_: DisplayError<_, _>| BoardInitError::Display)?;

        Ok(CoreS3SharedDisplayParts {
            display,
            internal_i2c: resources.internal_i2c,
        })
    }

    /// Creates a chip-select scoped SD SPI device using a previously-created shared SPI bus.
    ///
    /// The returned `spi_device` implements `embedded_hal::spi::SpiDevice` and can
    /// be passed to `embedded_sdmmc::SdCard::new`. The BSP does not encrypt or name
    /// application secrets.
    pub fn init_sd_on_shared_spi(
        resources: CoreS3SdOnSharedSpiResources,
    ) -> Result<CoreS3EspHalSdParts, BoardInitError> {
        let cs = Output::new(resources.tf_card_cs, Level::High, OutputConfig::default());
        let spi_device = CoreS3SharedSdDevice::new(resources.shared_spi, cs);
        Ok(CoreS3SdParts {
            spi_device,
            delay: Delay::new(),
            slot: resources.shared_spi.sd_slot,
        })
    }

    /// Initializes Gateway H2 UART for OpenThread RCP/Spinel-oriented downstream firmware.
    ///
    /// This helper does not validate the attached H2 firmware mode. Applications
    /// must flash/select a real OpenThread RCP/NCP firmware and implement Spinel
    /// HDLC-lite framing or a custom Thread controller protocol as appropriate.
    #[cfg(feature = "gateway-h2")]
    pub fn init_gateway_h2_openthread(
        resources: CoreS3GatewayH2Resources,
    ) -> Result<CoreS3GatewayH2OpenThreadParts, BoardInitError> {
        let parts = Self::init_gateway_h2(resources)?;
        let config = GatewayH2OpenThreadConfig::OPENTHREAD_RCP;
        Ok(CoreS3GatewayH2OpenThreadParts {
            transport: parts.uart,
            max_frame_size: config.max_frame_size,
            baud: parts.baud,
            config,
        })
    }

    /// Initializes only the Gateway H2 host UART link on Grove Port A and leaves
    /// all unrelated ESP peripherals untouched.
    pub fn init_gateway_h2(
        resources: CoreS3GatewayH2Resources,
    ) -> Result<CoreS3GatewayH2Parts, BoardInitError> {
        let uart = Uart::new(
            resources.uart1,
            UartConfig::default().with_baudrate(GATEWAY_H2_UART_BAUD),
        )
        .map_err(|_| BoardInitError::Uart)?
        .with_tx(resources.tx)
        .with_rx(resources.rx);

        Ok(CoreS3GatewayH2Parts {
            uart,
            baud: GATEWAY_H2_UART_BAUD,
        })
    }
}

fn configure_i2c(
    i2c0: esp_hal::peripherals::I2C0<'static>,
    sda: esp_hal::peripherals::GPIO12<'static>,
    scl: esp_hal::peripherals::GPIO11<'static>,
) -> Result<CoreS3I2c, BoardInitError> {
    EspI2c::new(
        i2c0,
        I2cConfig::default().with_frequency(Rate::from_hz(INTERNAL_I2C_HZ)),
    )
    .map(|i2c| i2c.with_sda(sda).with_scl(scl))
    .map_err(|_| BoardInitError::I2c)
}

fn configure_lcd_spi(
    spi2: esp_hal::peripherals::SPI2<'static>,
    sclk: esp_hal::peripherals::GPIO36<'static>,
    mosi: esp_hal::peripherals::GPIO37<'static>,
) -> Result<CoreS3RawSpi, BoardInitError> {
    Spi::new(
        spi2,
        SpiConfig::default()
            .with_frequency(Rate::from_hz(DISPLAY_SPI_WRITE_HZ))
            .with_mode(Mode::_0),
    )
    .map(|spi| spi.with_sck(sclk).with_mosi(mosi))
    .map_err(|_| BoardInitError::Spi)
}

fn configure_shared_spi(
    spi2: esp_hal::peripherals::SPI2<'static>,
    sclk: esp_hal::peripherals::GPIO36<'static>,
    mosi: esp_hal::peripherals::GPIO37<'static>,
    miso: esp_hal::peripherals::GPIO35<'static>,
) -> Result<CoreS3RawSpi, BoardInitError> {
    Spi::new(
        spi2,
        SpiConfig::default()
            .with_frequency(Rate::from_hz(DISPLAY_SPI_WRITE_HZ))
            .with_mode(Mode::_0),
    )
    .map(|spi| spi.with_sck(sclk).with_mosi(mosi).with_miso(miso))
    .map_err(|_| BoardInitError::Spi)
}

fn sd_spi_config() -> SpiConfig {
    SpiConfig::default()
        .with_frequency(Rate::from_hz(SD_SPI_INIT_HZ))
        .with_mode(Mode::_0)
}

fn sd_command(buf: &[u8]) -> Option<u8> {
    match buf {
        [command, _, _, _, _, _] if (command & 0xC0) == 0x40 => Some(command & 0x3F),
        _ => None,
    }
}

fn command_has_trailing_response(command: u8) -> bool {
    matches!(command, 8 | 58)
}

fn command_has_data_block(command: u8) -> bool {
    matches!(command, 9 | 10 | 17 | 18)
}

fn release_gpio35_for_sd(
    pin: &Mutex<RefCell<Flex<'static>>>,
    cs: critical_section::CriticalSection<'_>,
) {
    let mut dc = pin.borrow_ref_mut(cs);
    dc.set_output_enable(false);
    dc.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
    dc.set_input_enable(true);
}

fn init_display_power<I2C, Error>(i2c: &mut I2C, delay: &mut impl DelayNs) -> Result<(), Error>
where
    I2C: I2c<Error = Error>,
{
    // M5GFX CoreS3 sequence: configure AW9523B output state/config, enable
    // AXP2101 LDO rails, then reset the ILI9342 panel through AW9523B P1_1.
    write_bit(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_OUTPUT_P0,
        0,
        true,
    )?;
    write_bit(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_OUTPUT_P0,
        2,
        true,
    )?;
    write_bit(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_OUTPUT_P1,
        0,
        true,
    )?;
    write_bit(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_OUTPUT_P1,
        1,
        true,
    )?;
    write_register(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_CONFIG_P0,
        0b0001_1000,
    )?;
    write_register(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_CONFIG_P1,
        0b0000_1100,
    )?;
    write_register(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_GLOBAL_CONTROL,
        0b0001_0000,
    )?;
    write_register(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_LED_MODE_P0,
        0xFF,
    )?;
    write_register(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_LED_MODE_P1,
        0xFF,
    )?;

    let ldo_enable = read_register(i2c, devices::i2c::AXP2101_PMU, AXP_LDOS_ON_OFF)?;
    write_register(
        i2c,
        devices::i2c::AXP2101_PMU,
        AXP_LDOS_ON_OFF,
        ldo_enable | AXP_CORES3_LDO_ENABLE_MASK,
    )?;
    write_register(
        i2c,
        devices::i2c::AXP2101_PMU,
        AXP_ALDO3_VOLTAGE,
        AXP_LDO_3V3_CODE,
    )?;
    write_register(
        i2c,
        devices::i2c::AXP2101_PMU,
        AXP_ALDO4_VOLTAGE,
        AXP_LDO_3V3_CODE,
    )?;
    set_backlight(i2c, 255)?;

    let output_p1 = read_register(i2c, devices::i2c::AW9523B_GPIO_EXPANDER, AW_OUTPUT_P1)?;
    write_register(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_OUTPUT_P1,
        output_p1 & !AW_LCD_RESET_BIT,
    )?;
    delay.delay_ms(10);
    write_register(
        i2c,
        devices::i2c::AW9523B_GPIO_EXPANDER,
        AW_OUTPUT_P1,
        output_p1 | AW_LCD_RESET_BIT,
    )?;
    delay.delay_ms(20);
    Ok(())
}

fn set_backlight<I2C, Error>(i2c: &mut I2C, brightness: u8) -> Result<(), Error>
where
    I2C: I2c<Error = Error>,
{
    if brightness == 0 {
        write_bit(i2c, devices::i2c::AXP2101_PMU, AXP_LDOS_ON_OFF, 7, false)
    } else {
        let voltage = ((u16::from(brightness) + 641) >> 5) as u8;
        write_bit(i2c, devices::i2c::AXP2101_PMU, AXP_LDOS_ON_OFF, 7, true)?;
        write_register(i2c, devices::i2c::AXP2101_PMU, AXP_DLDO1_VOLTAGE, voltage)
    }
}

fn read_register<I2C, Error>(i2c: &mut I2C, address: u8, register: u8) -> Result<u8, Error>
where
    I2C: I2c<Error = Error>,
{
    let mut value = [0u8];
    i2c.write_read(address, &[register], &mut value)?;
    Ok(value[0])
}

fn write_register<I2C, Error>(
    i2c: &mut I2C,
    address: u8,
    register: u8,
    value: u8,
) -> Result<(), Error>
where
    I2C: I2c<Error = Error>,
{
    i2c.write(address, &[register, value])
}

fn write_bit<I2C, Error>(
    i2c: &mut I2C,
    address: u8,
    register: u8,
    bit: u8,
    value: bool,
) -> Result<(), Error>
where
    I2C: I2c<Error = Error>,
{
    let current = read_register(i2c, address, register)?;
    let mask = 1u8 << bit;
    let next = if value {
        current | mask
    } else {
        current & !mask
    };
    write_register(i2c, address, register, next)
}
