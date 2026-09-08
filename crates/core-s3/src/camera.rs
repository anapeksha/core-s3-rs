//! M5Stack CoreS3 camera metadata, configuration, and capture helpers.
//!
//! CoreS3 uses a GalaxyCore GC0308 DVP camera on the ESP32-S3 `LCD_CAM`
//! peripheral. M5Stack's CoreS3 user demo configures the sensor over the
//! internal SCCB/I²C bus on GPIO12/GPIO11 and captures QQVGA RGB565 frames.
//! This module keeps those board-facing pieces typed and bounded for `no_std`
//! firmware while avoiding any application-specific QR/provisioning/storage
//! policy.
//!
//! With the `camera` feature on Xtensa ESP-HAL targets,
//! [`crate::CoreS3::init_camera`] owns the CoreS3 camera pin mapping, configures
//! ESP-HAL `LCD_CAM`, probes the GC0308 sensor over SCCB/I²C, applies the
//! GC0308 initialization sequence, and returns a [`crate::bsp::CoreS3Camera`].
//! ESP-HAL 1.1.x camera DMA requires a DMA descriptor-backed buffer, so runtime
//! capture uses `capture_dma_frame(...)` with `esp_hal::dma::DmaRxBuf` instead
//! of a bare byte slice.
//!
//! ## Supported modes in v0.5.0
//!
//! The first camera release intentionally validates only the low-memory CoreS3
//! modes needed by the M5Stack demo and simple machine vision:
//!
//! - QQVGA RGB565 (`160x120`, 38_400 bytes)
//! - QQVGA grayscale/luminance (`160x120`, 19_200 bytes)
//! - centered GC0308 crop windows through [`DigitalZoom`]
//!
//! `Qvga`, `Vga`, JPEG, and custom windows remain modeled as public metadata so
//! callers can calculate buffer sizes and so future releases can extend support,
//! but `validate_config` rejects them until they are hardware-validated.
//!
//! The GC0308 default register table below is derived from Espressif's
//! `esp32-camera` `sensors/private_include/gc0308_settings.h` (Apache-2.0), then
//! reduced to a `no_std` static register/value list. Frame-size and pixel-format
//! programming mirrors the corresponding `gc0308.c` helpers for the supported
//! modes.

use embedded_hal::{delay::DelayNs, i2c::I2c};

/// GC0308 SCCB/I²C address used by ESP32 camera drivers.
pub const GC0308_SCCB_ADDRESS: u8 = 0x21;
/// GC0308 product-ID register.
pub const GC0308_PRODUCT_ID_REGISTER: u8 = 0x00;
/// Expected GC0308 product ID.
pub const GC0308_PRODUCT_ID: u8 = 0x9B;
/// Recommended low-memory QR/machine-vision frame-buffer length.
pub const QR_GRAYSCALE_FRAME_BUFFER_BYTES: usize = 160 * 120;
/// Recommended low-memory RGB565 preview frame-buffer length.
pub const QQVGA_RGB565_FRAME_BUFFER_BYTES: usize = 160 * 120 * 2;

const GC0308_PAGE_SELECT: u8 = 0xFE;
const GC0308_RESET_RELATED: u8 = 0xFE;
const GC0308_OUTPUT_FORMAT: u8 = 0x24;
const GC0308_ROW_START_H: u8 = 0x05;
const GC0308_ROW_START_L: u8 = 0x06;
const GC0308_COL_START_H: u8 = 0x07;
const GC0308_COL_START_L: u8 = 0x08;
const GC0308_WIN_HEIGHT_H: u8 = 0x09;
const GC0308_WIN_HEIGHT_L: u8 = 0x0A;
const GC0308_WIN_WIDTH_H: u8 = 0x0B;
const GC0308_WIN_WIDTH_L: u8 = 0x0C;
const GC0308_SUB_COL_N: u8 = 0xF7;
const GC0308_SUB_ROW_N: u8 = 0xF8;
const GC0308_SUB_COL_N1: u8 = 0xF9;
const GC0308_SUB_ROW_N1: u8 = 0xFA;
const GC0308_VGA_WIDTH: u16 = 640;
const GC0308_VGA_HEIGHT: u16 = 480;

/// GC0308 register write or delay operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gc0308RegOp {
    /// Write `value` to `register`.
    Write { register: u8, value: u8 },
    /// Delay for the given number of milliseconds.
    DelayMs(u32),
}

macro_rules! gc0308_regs {
    ($(($register:expr, $value:expr)),* $(,)?) => {
        &[$(Gc0308RegOp::Write { register: $register, value: $value }),*]
    };
}

/// Espressif GC0308 default tuning table, converted to a compact no_std slice.
pub const GC0308_DEFAULT_REGS: &[Gc0308RegOp] = gc0308_regs![
    (0xfe, 0x00),
    (0xec, 0x20),
    (0x05, 0x00),
    (0x06, 0x00),
    (0x07, 0x00),
    (0x08, 0x00),
    (0x09, 0x01),
    (0x0a, 0xe8),
    (0x0b, 0x02),
    (0x0c, 0x88),
    (0x0d, 0x02),
    (0x0e, 0x02),
    (0x10, 0x26),
    (0x11, 0x0d),
    (0x12, 0x2a),
    (0x13, 0x00),
    (0x14, 0x10),
    (0x15, 0x0a),
    (0x16, 0x05),
    (0x17, 0x01),
    (0x18, 0x44),
    (0x19, 0x44),
    (0x1a, 0x2a),
    (0x1b, 0x00),
    (0x1c, 0x49),
    (0x1d, 0x9a),
    (0x1e, 0x61),
    (0x1f, 0x00),
    (0x20, 0x7f),
    (0x21, 0xfa),
    (0x22, 0x57),
    (0x24, 0xa2),
    (0x25, 0x0f),
    (0x26, 0x03),
    (0x28, 0x00),
    (0x2d, 0x0a),
    (0x2f, 0x01),
    (0x30, 0xf7),
    (0x31, 0x50),
    (0x32, 0x00),
    (0x33, 0x28),
    (0x34, 0x2a),
    (0x35, 0x28),
    (0x39, 0x04),
    (0x3a, 0x20),
    (0x3b, 0x20),
    (0x3c, 0x00),
    (0x3d, 0x00),
    (0x3e, 0x00),
    (0x3f, 0x00),
    (0x50, 0x14),
    (0x52, 0x41),
    (0x53, 0x80),
    (0x54, 0x80),
    (0x55, 0x80),
    (0x56, 0x80),
    (0x5a, 0x56),
    (0x5b, 0x40),
    (0x5c, 0x4a),
    (0x8b, 0x20),
    (0x8c, 0x20),
    (0x8d, 0x20),
    (0x8e, 0x14),
    (0x8f, 0x10),
    (0x90, 0x14),
    (0x91, 0x3c),
    (0x92, 0x50),
    (0x5d, 0x12),
    (0x5e, 0x1a),
    (0x5f, 0x24),
    (0x60, 0x07),
    (0x61, 0x15),
    (0x62, 0x08),
    (0x64, 0x03),
    (0x66, 0xe8),
    (0x67, 0x86),
    (0x68, 0x82),
    (0x69, 0x18),
    (0x6a, 0x0f),
    (0x6b, 0x00),
    (0x6c, 0x5f),
    (0x6d, 0x8f),
    (0x6e, 0x55),
    (0x6f, 0x38),
    (0x70, 0x15),
    (0x71, 0x33),
    (0x72, 0xdc),
    (0x73, 0x00),
    (0x74, 0x02),
    (0x75, 0x3f),
    (0x76, 0x02),
    (0x77, 0x38),
    (0x78, 0x88),
    (0x79, 0x81),
    (0x7a, 0x81),
    (0x7b, 0x22),
    (0x7c, 0xff),
    (0x93, 0x48),
    (0x94, 0x02),
    (0x95, 0x07),
    (0x96, 0xe0),
    (0x97, 0x40),
    (0x98, 0xf0),
    (0xb1, 0x40),
    (0xb2, 0x40),
    (0xb3, 0x40),
    (0xb6, 0xe0),
    (0xbd, 0x38),
    (0xbe, 0x36),
    (0xd0, 0xcb),
    (0xd1, 0x10),
    (0xd2, 0x90),
    (0xd3, 0x48),
    (0xd5, 0xf2),
    (0xd6, 0x16),
    (0xdb, 0x92),
    (0xdc, 0xa5),
    (0xdf, 0x23),
    (0xd9, 0x00),
    (0xda, 0x00),
    (0xe0, 0x09),
    (0xed, 0x04),
    (0xee, 0xa0),
    (0xef, 0x40),
    (0x80, 0x03),
    (0x9f, 0x10),
    (0xa0, 0x20),
    (0xa1, 0x38),
    (0xa2, 0x4e),
    (0xa3, 0x63),
    (0xa4, 0x76),
    (0xa5, 0x87),
    (0xa6, 0xa2),
    (0xa7, 0xb8),
    (0xa8, 0xca),
    (0xa9, 0xd8),
    (0xaa, 0xe3),
    (0xab, 0xeb),
    (0xac, 0xf0),
    (0xad, 0xf8),
    (0xae, 0xfd),
    (0xaf, 0xff),
    (0xc0, 0x00),
    (0xc1, 0x10),
    (0xc2, 0x1c),
    (0xc3, 0x30),
    (0xc4, 0x43),
    (0xc5, 0x54),
    (0xc6, 0x65),
    (0xc7, 0x75),
    (0xc8, 0x93),
    (0xc9, 0xb0),
    (0xca, 0xcb),
    (0xcb, 0xe6),
    (0xcc, 0xff),
    (0xf0, 0x02),
    (0xf1, 0x01),
    (0xf2, 0x02),
    (0xf3, 0x30),
    (0xf7, 0x04),
    (0xf8, 0x02),
    (0xf9, 0x9f),
    (0xfa, 0x78),
    (0xfe, 0x01),
    (0x00, 0xf5),
    (0x02, 0x20),
    (0x04, 0x10),
    (0x05, 0x08),
    (0x06, 0x20),
    (0x08, 0x0a),
    (0x0a, 0xa0),
    (0x0b, 0x60),
    (0x0c, 0x08),
    (0x0e, 0x44),
    (0x0f, 0x32),
    (0x10, 0x41),
    (0x11, 0x37),
    (0x12, 0x22),
    (0x13, 0x19),
    (0x14, 0x44),
    (0x15, 0x44),
    (0x16, 0xc2),
    (0x17, 0xa8),
    (0x18, 0x18),
    (0x19, 0x50),
    (0x1a, 0xd8),
    (0x1b, 0xf5),
    (0x70, 0x40),
    (0x71, 0x58),
    (0x72, 0x30),
    (0x73, 0x48),
    (0x74, 0x20),
    (0x75, 0x60),
    (0x77, 0x20),
    (0x78, 0x32),
    (0x30, 0x03),
    (0x31, 0x40),
    (0x32, 0x10),
    (0x33, 0xe0),
    (0x34, 0xe0),
    (0x35, 0x00),
    (0x36, 0x80),
    (0x37, 0x00),
    (0x38, 0x04),
    (0x39, 0x09),
    (0x3a, 0x12),
    (0x3b, 0x1c),
    (0x3c, 0x28),
    (0x3d, 0x31),
    (0x3e, 0x44),
    (0x3f, 0x57),
    (0x40, 0x6c),
    (0x41, 0x81),
    (0x42, 0x94),
    (0x43, 0xa7),
    (0x44, 0xb8),
    (0x45, 0xd6),
    (0x46, 0xee),
    (0x47, 0x0d),
    (0x62, 0xf7),
    (0x63, 0x68),
    (0x64, 0xd3),
    (0x65, 0xd3),
    (0x66, 0x60),
    (0xfe, 0x00),
    (0x01, 0x32),
    (0x02, 0x0c),
    (0x0f, 0x01),
    (0xe2, 0x00),
    (0xe3, 0x78),
    (0xe4, 0x00),
    (0xe5, 0xfe),
    (0xe6, 0x01),
    (0xe7, 0xe0),
    (0xe8, 0x01),
    (0xe9, 0xe0),
    (0xea, 0x01),
    (0xeb, 0xe0),
    (0xfe, 0x00),
];

/// Camera sensor identified on CoreS3.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraSensor {
    /// GalaxyCore GC0308, the sensor used by the M5Stack CoreS3 user demo.
    Gc0308 { product_id: u8 },
}

/// Requested frame size.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameSize {
    /// 160x120. Supported for CoreS3 GC0308 in v0.5.0.
    Qqvga,
    /// 320x240. Modeled but not configured by v0.5.0.
    Qvga,
    /// 640x480. Modeled but not configured by v0.5.0.
    Vga,
    /// 800x600. Not supported by GC0308/CoreS3.
    Svga,
    /// 1024x768. Not supported by GC0308/CoreS3.
    Xga,
    /// Explicit size. Modeled for buffer sizing; not configured by v0.5.0.
    Custom { width: u16, height: u16 },
}

impl FrameSize {
    /// Return `(width, height)` for this frame size.
    pub const fn dimensions(self) -> Option<(u16, u16)> {
        match self {
            Self::Qqvga => Some((160, 120)),
            Self::Qvga => Some((320, 240)),
            Self::Vga => Some((640, 480)),
            Self::Svga | Self::Xga => None,
            Self::Custom { width, height }
                if width > 0 && height > 0 && width <= 640 && height <= 480 =>
            {
                Some((width, height))
            }
            Self::Custom { .. } => None,
        }
    }
}

/// Centered digital crop applied at the GC0308 sensor window.
///
/// This is not optical zoom. `X2` and `X4` shrink the active centered sensor
/// window and therefore also reduce the captured frame dimensions. Preview code
/// can scale the returned frame back up on the LCD.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigitalZoom {
    /// Full configured frame window.
    X1,
    /// Center crop to half width/height.
    X2,
    /// Center crop to quarter width/height.
    X4,
}

impl DigitalZoom {
    /// Integer crop divisor for width and height.
    pub const fn divisor(self) -> u16 {
        match self {
            Self::X1 => 1,
            Self::X2 => 2,
            Self::X4 => 4,
        }
    }
}

/// Camera output pixel format.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// One luminance byte per pixel.
    Grayscale8,
    /// 16-bit RGB565, two bytes per pixel.
    Rgb565,
    /// Packed YUV422, two bytes per pixel. Modeled but not validated in v0.5.0.
    Yuv422,
    /// JPEG bitstream. GC0308 does not provide JPEG on CoreS3.
    Jpeg,
}

impl PixelFormat {
    /// Bytes per pixel for fixed-width raw formats.
    pub const fn bytes_per_pixel(self) -> Option<usize> {
        match self {
            Self::Grayscale8 => Some(1),
            Self::Rgb565 | Self::Yuv422 => Some(2),
            Self::Jpeg => None,
        }
    }
}

/// Camera configuration requested by firmware.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraConfig {
    pub frame_size: FrameSize,
    pub pixel_format: PixelFormat,
    /// Sensor master clock frequency. M5Stack's CoreS3 demo uses 20 MHz.
    pub xclk_hz: u32,
    /// Centered GC0308 sensor crop. Preview code may scale the cropped frame.
    pub zoom: DigitalZoom,
}

impl CameraConfig {
    /// M5Stack-demo-compatible QQVGA RGB565 preview configuration.
    pub const fn qqvga_rgb565() -> Self {
        Self {
            frame_size: FrameSize::Qqvga,
            pixel_format: PixelFormat::Rgb565,
            xclk_hz: 20_000_000,
            zoom: DigitalZoom::X1,
        }
    }

    /// Low-memory QR/machine-vision oriented grayscale configuration.
    ///
    /// GC0308 can emit grayscale/luminance bytes. This avoids requiring a large
    /// RGB/JPEG decode buffer in downstream QR or vision code.
    pub const fn qr_grayscale() -> Self {
        Self {
            frame_size: FrameSize::Qqvga,
            pixel_format: PixelFormat::Grayscale8,
            xclk_hz: 20_000_000,
            zoom: DigitalZoom::X1,
        }
    }

    /// Return base dimensions before digital crop.
    pub const fn dimensions(self) -> Option<(u16, u16)> {
        self.frame_size.dimensions()
    }

    /// Return output dimensions after the centered digital crop.
    pub const fn output_dimensions(self) -> Option<(u16, u16)> {
        let (width, height) = match self.frame_size.dimensions() {
            Some(dimensions) => dimensions,
            None => return None,
        };
        let divisor = self.zoom.divisor();
        Some((width / divisor, height / divisor))
    }

    /// Return this configuration with a centered sensor crop applied.
    pub const fn with_zoom(mut self, zoom: DigitalZoom) -> Self {
        self.zoom = zoom;
        self
    }
}

impl Default for CameraConfig {
    fn default() -> Self {
        Self::qqvga_rgb565()
    }
}

/// Metadata for a captured frame.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraFrame<'a> {
    pub width: u16,
    pub height: u16,
    pub format: PixelFormat,
    pub stride: u16,
    pub data: &'a [u8],
}

/// Owned frame metadata returned with ESP-HAL DMA buffers.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraFrameInfo {
    pub width: u16,
    pub height: u16,
    pub format: PixelFormat,
    pub stride: u16,
    pub len: usize,
}

/// Camera initialization/configuration errors.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraInitError {
    UnsupportedHardware,
    SensorNotFound,
    SensorUnsupported,
    Power,
    Clock,
    Pins,
    Sccb,
    Dma,
    InvalidConfig,
    FrameBufferTooSmall,
}

/// Camera runtime/capture errors.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraCaptureError {
    NotStarted,
    Timeout,
    Dma,
    Sensor,
    BufferTooSmall,
    InvalidState,
}

/// Return the required raw frame-buffer length for a fixed-size raw mode.
pub const fn frame_buffer_len(frame_size: FrameSize, format: PixelFormat) -> Option<usize> {
    let (width, height) = match frame_size.dimensions() {
        Some(dimensions) => dimensions,
        None => return None,
    };
    let bytes_per_pixel = match format.bytes_per_pixel() {
        Some(bytes) => bytes,
        None => return None,
    };
    Some(width as usize * height as usize * bytes_per_pixel)
}

/// Return the expected metadata for a captured frame in this configuration.
pub const fn frame_info(config: CameraConfig) -> Option<CameraFrameInfo> {
    let (width, height) = match config.output_dimensions() {
        Some(dimensions) => dimensions,
        None => return None,
    };
    let bytes_per_pixel = match config.pixel_format.bytes_per_pixel() {
        Some(bytes) => bytes,
        None => return None,
    };
    let len = width as usize * height as usize * bytes_per_pixel;
    Some(CameraFrameInfo {
        width,
        height,
        format: config.pixel_format,
        stride: width * bytes_per_pixel as u16,
        len,
    })
}

/// Validate a CoreS3 camera configuration against the hardware-validated GC0308 path.
pub const fn validate_config(config: CameraConfig) -> Result<(), CameraInitError> {
    if config.xclk_hz == 0 {
        return Err(CameraInitError::Clock);
    }
    match config.pixel_format {
        PixelFormat::Jpeg => return Err(CameraInitError::SensorUnsupported),
        PixelFormat::Yuv422 => return Err(CameraInitError::SensorUnsupported),
        PixelFormat::Grayscale8 | PixelFormat::Rgb565 => {}
    }
    if !matches!(config.frame_size, FrameSize::Qqvga) {
        return Err(CameraInitError::SensorUnsupported);
    }
    if frame_info(config).is_none() {
        return Err(CameraInitError::InvalidConfig);
    }
    Ok(())
}

/// Probe a GC0308 sensor over SCCB/I²C.
pub fn probe_gc0308<I2C>(i2c: &mut I2C) -> Result<CameraSensor, CameraInitError>
where
    I2C: I2c,
{
    write_register(i2c, GC0308_PAGE_SELECT, 0x00).map_err(|_| CameraInitError::Sccb)?;
    let product_id = read_register(i2c, GC0308_PRODUCT_ID_REGISTER)
        .map_err(|_| CameraInitError::SensorNotFound)?;
    if product_id == GC0308_PRODUCT_ID {
        Ok(CameraSensor::Gc0308 { product_id })
    } else {
        Err(CameraInitError::SensorUnsupported)
    }
}

/// Apply the GC0308 default table and supported CoreS3 output/window settings.
pub fn configure_gc0308<I2C, DELAY>(
    i2c: &mut I2C,
    delay: &mut DELAY,
    config: CameraConfig,
) -> Result<(), CameraInitError>
where
    I2C: I2c,
    DELAY: DelayNs,
{
    validate_config(config)?;
    write_register(i2c, GC0308_RESET_RELATED, 0xF0).map_err(|_| CameraInitError::Sccb)?;
    delay.delay_ms(80);
    apply_gc0308_registers(i2c, delay, GC0308_DEFAULT_REGS)?;
    delay.delay_ms(80);
    configure_gc0308_frame_size(i2c, config)?;
    configure_gc0308_pixel_format(i2c, config.pixel_format)?;
    delay.delay_ms(80);
    Ok(())
}

/// Apply a GC0308 register-operation slice.
pub fn apply_gc0308_registers<I2C, DELAY>(
    i2c: &mut I2C,
    delay: &mut DELAY,
    operations: &[Gc0308RegOp],
) -> Result<(), CameraInitError>
where
    I2C: I2c,
    DELAY: DelayNs,
{
    for operation in operations {
        match *operation {
            Gc0308RegOp::Write { register, value } => {
                write_register(i2c, register, value).map_err(|_| CameraInitError::Sccb)?;
            }
            Gc0308RegOp::DelayMs(ms) => delay.delay_ms(ms),
        }
    }
    Ok(())
}

fn configure_gc0308_pixel_format<I2C>(
    i2c: &mut I2C,
    format: PixelFormat,
) -> Result<(), CameraInitError>
where
    I2C: I2c,
{
    write_register(i2c, GC0308_PAGE_SELECT, 0x00).map_err(|_| CameraInitError::Sccb)?;
    let output = match format {
        PixelFormat::Rgb565 => 0x06,
        PixelFormat::Grayscale8 => 0xB1,
        PixelFormat::Yuv422 | PixelFormat::Jpeg => return Err(CameraInitError::SensorUnsupported),
    };
    write_register(i2c, GC0308_OUTPUT_FORMAT, output).map_err(|_| CameraInitError::Sccb)
}

fn configure_gc0308_frame_size<I2C>(
    i2c: &mut I2C,
    config: CameraConfig,
) -> Result<(), CameraInitError>
where
    I2C: I2c,
{
    if !matches!(config.frame_size, FrameSize::Qqvga) {
        return Err(CameraInitError::SensorUnsupported);
    }
    let (width, height) = config
        .output_dimensions()
        .ok_or(CameraInitError::InvalidConfig)?;
    let row_start = (GC0308_VGA_HEIGHT - height) / 2;
    let col_start = (GC0308_VGA_WIDTH - width) / 2;
    write_register(i2c, GC0308_PAGE_SELECT, 0x00).map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_SUB_COL_N, (col_start / 4) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_SUB_ROW_N, (row_start / 4) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_SUB_COL_N1, ((col_start + width) / 4) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_SUB_ROW_N1, ((row_start + height) / 4) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_ROW_START_H, (row_start >> 8) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_ROW_START_L, row_start as u8).map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_COL_START_H, (col_start >> 8) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_COL_START_L, col_start as u8).map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_WIN_HEIGHT_H, ((height + 8) >> 8) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_WIN_HEIGHT_L, (height + 8) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_WIN_WIDTH_H, ((width + 8) >> 8) as u8)
        .map_err(|_| CameraInitError::Sccb)?;
    write_register(i2c, GC0308_WIN_WIDTH_L, (width + 8) as u8).map_err(|_| CameraInitError::Sccb)
}

fn read_register<I2C>(i2c: &mut I2C, register: u8) -> Result<u8, I2C::Error>
where
    I2C: I2c,
{
    let mut value = [0u8];
    i2c.write_read(GC0308_SCCB_ADDRESS, &[register], &mut value)?;
    Ok(value[0])
}

fn write_register<I2C>(i2c: &mut I2C, register: u8, value: u8) -> Result<(), I2C::Error>
where
    I2C: I2c,
{
    i2c.write(GC0308_SCCB_ADDRESS, &[register, value])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_dimensions_match_named_sizes() {
        assert_eq!(FrameSize::Qqvga.dimensions(), Some((160, 120)));
        assert_eq!(FrameSize::Qvga.dimensions(), Some((320, 240)));
        assert_eq!(FrameSize::Vga.dimensions(), Some((640, 480)));
        assert_eq!(FrameSize::Svga.dimensions(), None);
        assert_eq!(FrameSize::Xga.dimensions(), None);
    }

    #[test]
    fn frame_buffer_lengths_are_bounded() {
        assert_eq!(
            frame_buffer_len(FrameSize::Qqvga, PixelFormat::Grayscale8),
            Some(QR_GRAYSCALE_FRAME_BUFFER_BYTES)
        );
        assert_eq!(
            frame_buffer_len(FrameSize::Qqvga, PixelFormat::Rgb565),
            Some(QQVGA_RGB565_FRAME_BUFFER_BYTES)
        );
        assert_eq!(frame_buffer_len(FrameSize::Qqvga, PixelFormat::Jpeg), None);
    }

    #[test]
    fn qr_config_is_low_memory_grayscale() {
        let config = CameraConfig::qr_grayscale();
        assert_eq!(config.frame_size, FrameSize::Qqvga);
        assert_eq!(config.pixel_format, PixelFormat::Grayscale8);
        assert_eq!(
            frame_info(config).unwrap().len,
            QR_GRAYSCALE_FRAME_BUFFER_BYTES
        );
    }

    #[test]
    fn validate_accepts_v050_supported_modes() {
        assert_eq!(validate_config(CameraConfig::qqvga_rgb565()), Ok(()));
        assert_eq!(validate_config(CameraConfig::qr_grayscale()), Ok(()));
    }

    #[test]
    fn validate_rejects_unsupported_or_invalid_modes() {
        assert_eq!(
            validate_config(CameraConfig {
                frame_size: FrameSize::Qqvga,
                pixel_format: PixelFormat::Jpeg,
                xclk_hz: 20_000_000,
                zoom: DigitalZoom::X1,
            }),
            Err(CameraInitError::SensorUnsupported)
        );
        assert_eq!(
            validate_config(CameraConfig {
                frame_size: FrameSize::Qqvga,
                pixel_format: PixelFormat::Yuv422,
                xclk_hz: 20_000_000,
                zoom: DigitalZoom::X1,
            }),
            Err(CameraInitError::SensorUnsupported)
        );
        assert_eq!(
            validate_config(CameraConfig {
                frame_size: FrameSize::Qvga,
                pixel_format: PixelFormat::Rgb565,
                xclk_hz: 20_000_000,
                zoom: DigitalZoom::X1,
            }),
            Err(CameraInitError::SensorUnsupported)
        );
        assert_eq!(
            validate_config(CameraConfig {
                frame_size: FrameSize::Custom {
                    width: 0,
                    height: 120
                },
                pixel_format: PixelFormat::Rgb565,
                xclk_hz: 20_000_000,
                zoom: DigitalZoom::X1,
            }),
            Err(CameraInitError::SensorUnsupported)
        );
    }

    #[test]
    fn digital_zoom_reduces_output_dimensions() {
        let x2 = CameraConfig::qqvga_rgb565().with_zoom(DigitalZoom::X2);
        let x4 = CameraConfig::qqvga_rgb565().with_zoom(DigitalZoom::X4);
        assert_eq!(x2.output_dimensions(), Some((80, 60)));
        assert_eq!(x4.output_dimensions(), Some((40, 30)));
        assert_eq!(frame_info(x2).unwrap().len, 80 * 60 * 2);
        assert_eq!(frame_info(x4).unwrap().len, 40 * 30 * 2);
        assert_eq!(validate_config(x2), Ok(()));
        assert_eq!(validate_config(x4), Ok(()));
    }

    #[test]
    fn gc0308_defaults_start_and_end_on_page_zero() {
        assert_eq!(
            GC0308_DEFAULT_REGS.first().copied(),
            Some(Gc0308RegOp::Write {
                register: GC0308_PAGE_SELECT,
                value: 0x00,
            })
        );
        assert_eq!(
            GC0308_DEFAULT_REGS.last().copied(),
            Some(Gc0308RegOp::Write {
                register: GC0308_PAGE_SELECT,
                value: 0x00,
            })
        );
        assert!(GC0308_DEFAULT_REGS.len() > 200);
    }
}
