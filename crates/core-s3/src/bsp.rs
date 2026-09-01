//! ESP-HAL board bring-up helpers for M5Stack CoreS3.
//!
//! These helpers own only the resources needed for the requested peripheral.
//! `CoreS3DisplayResources` consumes the LCD SPI pins plus the internal I2C
//! bus used to configure AXP2101/AW9523B display power, reset, and backlight;
//! all other ESP peripherals remain with the application.

use embedded_hal::{delay::DelayNs, i2c::I2c};
use embedded_hal_bus::spi;
use esp_hal::{
    Blocking,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c as EspI2c},
    spi::{
        Mode,
        master::{Config as SpiConfig, Spi},
    },
    time::Rate,
};

use crate::{
    CoreS3, devices,
    display::{BusConfig, Display, DisplayError, DisplayGeometry, PanelConfig},
};

/// Display SPI write frequency used by M5GFX for CoreS3 after autodetection.
pub const DISPLAY_SPI_WRITE_HZ: u32 = 40_000_000;
/// CoreS3 internal I2C frequency.
pub const INTERNAL_I2C_HZ: u32 = 400_000;

const AXP_LDOS_ON_OFF: u8 = 0x90;
const AXP_ALDO3_VOLTAGE: u8 = 0x94;
const AXP_ALDO4_VOLTAGE: u8 = 0x95;
const AXP_DLDO1_VOLTAGE: u8 = 0x99;
const AXP_LDO_3V3_CODE: u8 = 33 - 5;
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
/// SPI device wrapper used by the CoreS3 LCD.
pub type CoreS3LcdSpiDevice = spi::ExclusiveDevice<CoreS3RawSpi, CoreS3Output, Delay>;
/// Concrete display type returned by [`CoreS3::init_display`].
pub type CoreS3Display = Display<CoreS3LcdSpiDevice, CoreS3Output, CoreS3Output>;

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

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardInitError {
    I2c,
    Spi,
    Power,
    Display,
}

impl CoreS3 {
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

    write_register(i2c, devices::i2c::AXP2101_PMU, AXP_LDOS_ON_OFF, 0xBF)?;
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
