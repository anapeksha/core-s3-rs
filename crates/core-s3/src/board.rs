use crate::{devices, pins};

/// Static description of the M5Stack CoreS3 board variant supported by this BSP.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Board {
    pub name: &'static str,
    pub chip: &'static str,
    pub psram: MemorySize,
    pub flash: MemorySize,
    pub display: DisplaySpec,
    pub i2c: pins::I2cBusPins,
    pub spi_display: pins::SpiDisplayPins,
    pub sd: crate::sd::SdCardSlot,
    pub camera: pins::CameraPins,
    pub i2s: pins::I2sAudioPins,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemorySize {
    pub bytes: usize,
}

impl MemorySize {
    pub const fn mib(mib: usize) -> Self {
        Self {
            bytes: mib * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplaySpec {
    pub width: u16,
    pub height: u16,
    pub controller: &'static str,
    pub color_order: ColorOrder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorOrder {
    Rgb,
    Bgr,
}

/// M5Stack CoreS3 K128 board constants.
pub struct CoreS3;

impl CoreS3 {
    pub const BOARD: Board = Board {
        name: "M5Stack CoreS3 K128",
        chip: "ESP32-S3R8",
        psram: MemorySize::mib(8),
        flash: MemorySize::mib(16),
        display: DisplaySpec {
            width: devices::display::WIDTH,
            height: devices::display::HEIGHT,
            controller: devices::display::CONTROLLER,
            color_order: ColorOrder::Bgr,
        },
        i2c: pins::I2cBusPins::INTERNAL,
        spi_display: pins::SpiDisplayPins::LCD,
        sd: crate::sd::SdCardSlot::CORE_S3,
        camera: pins::CameraPins::GC0308,
        i2s: pins::I2sAudioPins::CODECS,
    };

    pub const fn board() -> Board {
        Self::BOARD
    }
}
