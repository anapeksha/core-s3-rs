use embedded_graphics::{Pixel, pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use embedded_hal::{
    delay::DelayNs,
    digital::{Error as DigitalError, OutputPin},
    spi::{Error as SpiErrorTrait, SpiDevice},
};

const COLOR_STREAM_PIXELS: usize = 128;
const PIXEL_STREAM_PIXELS: usize = 128;
const DRAW_ITER_PIXELS: usize = 64;
const RGB565_BE_WRITE_CHUNK_BYTES: usize = 4096;

const CMD_SOFTWARE_RESET: u8 = 0x01;
const CMD_SLEEP_OUT: u8 = 0x11;
const CMD_DISPLAY_INVERSION_ON: u8 = 0x21;
const CMD_DISPLAY_ON: u8 = 0x29;
const CMD_IDLE_MODE_OFF: u8 = 0x38;
const CMD_COLUMN_ADDRESS_SET: u8 = 0x2A;
const CMD_ROW_ADDRESS_SET: u8 = 0x2B;
const CMD_MEMORY_WRITE: u8 = 0x2C;
const CMD_MEMORY_ACCESS_CONTROL: u8 = 0x36;
const CMD_PIXEL_FORMAT_SET: u8 = 0x3A;
const PIXEL_FORMAT_RGB565: u8 = 0x55;

const SOFTWARE_RESET_DELAY_NS: u32 = 150_000_000;
const SLEEP_OUT_DELAY_NS: u32 = 120_000_000;
const DISPLAY_ON_DELAY_NS: u32 = 20_000_000;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayError<SpiError, PinError> {
    Spi(SpiError),
    Pin(PinError),
    PixelData(PixelDataError),
    Text,
}

/// Validation error for caller-owned panel-ready pixel data.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelDataError {
    OddByteLength,
    LengthMismatch { expected: usize, actual: usize },
    LengthOverflow,
    OutOfBounds,
    UnsupportedOrientation,
}

/// Transfer counters for one scoped LCD session.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DisplayTransferStats {
    pub logical_sessions: u32,
    pub address_windows: u32,
    pub spi_write_operations: u32,
    pub pixel_bytes: usize,
}

/// Low-level writer available only while a device-owned LCD session is active.
pub trait LcdTransactionWriter {
    type Error;

    fn set_command_mode(&mut self) -> Result<(), Self::Error>;
    fn set_data_mode(&mut self) -> Result<(), Self::Error>;
    fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// Optional capability for devices that can keep LCD CS and bus ownership
/// active across multiple command/data writes.
pub trait LcdTransactionDevice: SpiDevice {
    fn with_lcd_transaction<R, E>(
        &mut self,
        operation: impl FnOnce(&mut dyn LcdTransactionWriter<Error = Self::Error>) -> Result<R, E>,
    ) -> Result<R, LcdTransactionError<E, Self::Error>>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LcdTransactionError<OperationError, DeviceError> {
    Operation(OperationError),
    Device(DeviceError),
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayOrientation {
    Portrait,
    PortraitInverted,
    Landscape,
    LandscapeInverted,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DisplayGeometry {
    pub width: u16,
    pub height: u16,
    pub offset_x: u16,
    pub offset_y: u16,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelConfig {
    pub geometry: DisplayGeometry,
    pub invert_colors: bool,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BusConfig {
    pub write_hz: u32,
}

pub struct Display<SPI, DC, SDCS> {
    spi: SPI,
    dc: DC,
    sd_cs_guard: SDCS,
    bus: BusConfig,
    panel: PanelConfig,
    orientation: DisplayOrientation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PixelRun {
    y: i32,
    x_start: i32,
    x_end: i32,
    color: Rgb565,
}

impl PixelRun {
    const fn new(point: Point, color: Rgb565) -> Self {
        Self {
            y: point.y,
            x_start: point.x,
            x_end: point.x,
            color,
        }
    }

    fn try_extend(&mut self, point: Point, color: Rgb565) -> bool {
        if self.y == point.y && self.x_end.saturating_add(1) == point.x && self.color == color {
            self.x_end = point.x;
            true
        } else {
            false
        }
    }

    fn rectangle(self) -> Rectangle {
        Rectangle::new(
            Point::new(self.x_start, self.y),
            Size::new((self.x_end - self.x_start + 1) as u32, 1),
        )
    }
}

struct PixelRowBuffer {
    start: Point,
    colors: [Rgb565; DRAW_ITER_PIXELS],
    len: usize,
}

impl PixelRowBuffer {
    fn new(point: Point, color: Rgb565) -> Self {
        let mut colors = [Rgb565::BLACK; DRAW_ITER_PIXELS];
        colors[0] = color;
        Self {
            start: point,
            colors,
            len: 1,
        }
    }

    fn try_push(&mut self, point: Point, color: Rgb565) -> bool {
        if self.len == DRAW_ITER_PIXELS
            || point.y != self.start.y
            || point.x != self.start.x.saturating_add(self.len as i32)
        {
            return false;
        }
        self.colors[self.len] = color;
        self.len += 1;
        true
    }

    fn area(&self) -> Rectangle {
        Rectangle::new(self.start, Size::new(self.len as u32, 1))
    }

    fn colors(&self) -> &[Rgb565] {
        &self.colors[..self.len]
    }
}

fn rgb565_be_byte_len(width: usize, height: usize) -> Result<usize, PixelDataError> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(2))
        .ok_or(PixelDataError::LengthOverflow)
}

fn validate_rgb565_be_len(
    width: usize,
    height: usize,
    actual: usize,
) -> Result<usize, PixelDataError> {
    if !actual.is_multiple_of(2) {
        return Err(PixelDataError::OddByteLength);
    }
    let expected = rgb565_be_byte_len(width, height)?;
    if actual != expected {
        return Err(PixelDataError::LengthMismatch { expected, actual });
    }
    Ok(expected)
}

/// Scoped high-level writer used by [`Display::with_lcd_transaction`].
pub struct DisplayTransaction<'a, SpiError> {
    writer: &'a mut dyn LcdTransactionWriter<Error = SpiError>,
    geometry: DisplayGeometry,
    stats: DisplayTransferStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayTransactionError<SpiError> {
    Spi(SpiError),
    PixelData(PixelDataError),
}

impl<SpiError> DisplayTransaction<'_, SpiError> {
    /// Validates and streams one landscape panel region in big-endian RGB565.
    pub fn blit_rgb565_be(
        &mut self,
        area: Rectangle,
        bytes: &[u8],
    ) -> Result<(), DisplayTransactionError<SpiError>> {
        let bounds = Rectangle::new(
            Point::zero(),
            Size::new(
                u32::from(self.geometry.width),
                u32::from(self.geometry.height),
            ),
        );
        if area.is_zero_sized() {
            validate_rgb565_be_len(0, 0, bytes.len())
                .map_err(DisplayTransactionError::PixelData)?;
            return Ok(());
        }
        if area.intersection(&bounds) != area {
            return Err(DisplayTransactionError::PixelData(
                PixelDataError::OutOfBounds,
            ));
        }
        validate_rgb565_be_len(
            area.size.width as usize,
            area.size.height as usize,
            bytes.len(),
        )
        .map_err(DisplayTransactionError::PixelData)?;
        self.set_address_window(area)
            .map_err(DisplayTransactionError::Spi)?;
        self.write_rgb565_be(bytes)
            .map_err(DisplayTransactionError::Spi)
    }

    fn set_address_window(&mut self, area: Rectangle) -> Result<(), SpiError> {
        let x0 = self.geometry.offset_x + area.top_left.x as u16;
        let y0 = self.geometry.offset_y + area.top_left.y as u16;
        let x1 = x0 + area.size.width.saturating_sub(1) as u16;
        let y1 = y0 + area.size.height.saturating_sub(1) as u16;

        self.command(
            CMD_COLUMN_ADDRESS_SET,
            &[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8],
        )?;
        self.command(
            CMD_ROW_ADDRESS_SET,
            &[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8],
        )?;
        self.command(CMD_MEMORY_WRITE, &[])?;
        self.writer.set_data_mode()?;
        self.stats.address_windows += 1;
        Ok(())
    }

    /// Streams panel-ready big-endian RGB565 bytes without conversion or copying.
    fn write_rgb565_be(&mut self, bytes: &[u8]) -> Result<(), SpiError> {
        for chunk in bytes.chunks(RGB565_BE_WRITE_CHUNK_BYTES) {
            self.writer.write(chunk)?;
            self.stats.spi_write_operations += 1;
            self.stats.pixel_bytes += chunk.len();
        }
        Ok(())
    }

    #[must_use]
    pub const fn stats(&self) -> DisplayTransferStats {
        self.stats
    }

    fn command(&mut self, command: u8, data: &[u8]) -> Result<(), SpiError> {
        self.writer.set_command_mode()?;
        self.writer.write(&[command])?;
        self.stats.spi_write_operations += 1;
        if !data.is_empty() {
            self.writer.set_data_mode()?;
            self.writer.write(data)?;
            self.stats.spi_write_operations += 1;
        }
        Ok(())
    }
}

impl<SPI, DC, SDCS> Display<SPI, DC, SDCS> {
    #[must_use]
    pub const fn new(
        spi: SPI,
        dc: DC,
        sd_cs_guard: SDCS,
        bus: BusConfig,
        panel: PanelConfig,
    ) -> Self {
        Self {
            spi,
            dc,
            sd_cs_guard,
            bus,
            panel,
            orientation: DisplayOrientation::Landscape,
        }
    }

    #[must_use]
    pub const fn bus_config(&self) -> BusConfig {
        self.bus
    }

    #[must_use]
    pub const fn panel_config(&self) -> PanelConfig {
        self.panel
    }

    #[must_use]
    pub const fn geometry(&self) -> DisplayGeometry {
        self.panel.geometry
    }

    #[must_use]
    pub const fn orientation(&self) -> DisplayOrientation {
        self.orientation
    }

    pub fn set_orientation(&mut self, orientation: DisplayOrientation) {
        self.orientation = orientation;
    }

    pub fn clip_rectangle(&self, area: Rectangle) -> Rectangle {
        area.intersection(&self.bounding_box())
    }

    pub fn map_logical_rectangle_to_native(&self, area: &Rectangle) -> Rectangle {
        self.map_rectangle_to_native(area)
    }

    pub fn release(self) -> (SPI, DC, SDCS) {
        (self.spi, self.dc, self.sd_cs_guard)
    }

    /// Returns the exact byte length for a tightly packed RGB565 frame.
    pub fn rgb565_be_byte_len(width: usize, height: usize) -> Result<usize, PixelDataError> {
        rgb565_be_byte_len(width, height)
    }

    /// Validates a tightly packed big-endian RGB565 byte slice.
    pub fn validate_rgb565_be_len(
        width: usize,
        height: usize,
        actual: usize,
    ) -> Result<usize, PixelDataError> {
        validate_rgb565_be_len(width, height, actual)
    }

    fn logical_size(&self) -> Size {
        let geometry = self.panel.geometry;
        match self.orientation {
            DisplayOrientation::Portrait | DisplayOrientation::PortraitInverted => {
                Size::new(u32::from(geometry.height), u32::from(geometry.width))
            }
            DisplayOrientation::Landscape | DisplayOrientation::LandscapeInverted => {
                Size::new(u32::from(geometry.width), u32::from(geometry.height))
            }
        }
    }

    fn map_rectangle_to_native(&self, area: &Rectangle) -> Rectangle {
        let geometry = self.panel.geometry;
        let native_width = i32::from(geometry.width);
        let native_height = i32::from(geometry.height);
        let x = area.top_left.x;
        let y = area.top_left.y;
        let width = area.size.width as i32;
        let height = area.size.height as i32;

        match self.orientation {
            DisplayOrientation::Landscape => *area,
            DisplayOrientation::LandscapeInverted => Rectangle::new(
                Point::new(native_width - x - width, native_height - y - height),
                area.size,
            ),
            DisplayOrientation::Portrait => Rectangle::new(
                Point::new(y, native_height - x - width),
                Size::new(area.size.height, area.size.width),
            ),
            DisplayOrientation::PortraitInverted => Rectangle::new(
                Point::new(native_width - y - height, x),
                Size::new(area.size.height, area.size.width),
            ),
        }
    }
}

impl<SPI, DC, SDCS, SpiError, PinError> Display<SPI, DC, SDCS>
where
    SPI: SpiDevice<Error = SpiError>,
    DC: OutputPin<Error = PinError>,
    SDCS: OutputPin<Error = PinError>,
    SpiError: SpiErrorTrait,
    PinError: DigitalError,
{
    pub fn init(
        &mut self,
        delay: &mut impl DelayNs,
    ) -> Result<(), DisplayError<SpiError, PinError>> {
        self.sd_cs_guard.set_high().map_err(DisplayError::Pin)?;
        self.command(CMD_SOFTWARE_RESET, &[])?;
        delay.delay_ns(SOFTWARE_RESET_DELAY_NS);
        self.init_ili9342_registers()?;
        self.command(CMD_SLEEP_OUT, &[])?;
        delay.delay_ns(SLEEP_OUT_DELAY_NS);
        self.command(CMD_PIXEL_FORMAT_SET, &[PIXEL_FORMAT_RGB565])?;
        self.write_orientation()?;
        if self.panel.invert_colors {
            self.command(CMD_DISPLAY_INVERSION_ON, &[])?;
        }
        self.command(CMD_IDLE_MODE_OFF, &[])?;
        self.command(CMD_DISPLAY_ON, &[])?;
        delay.delay_ns(DISPLAY_ON_DELAY_NS);
        Ok(())
    }

    fn init_ili9342_registers(&mut self) -> Result<(), DisplayError<SpiError, PinError>> {
        self.command(0xC8, &[0xFF, 0x93, 0x42])?;
        self.command(0xC0, &[0x12, 0x12])?;
        self.command(0xC1, &[0x03])?;
        self.command(0xC5, &[0xF2])?;
        self.command(0xB0, &[0xE0])?;
        self.command(0xF6, &[0x01, 0x00, 0x00])?;
        self.command(
            0xE0,
            &[
                0x00, 0x0C, 0x11, 0x04, 0x11, 0x08, 0x37, 0x89, 0x4C, 0x06, 0x0C, 0x0A, 0x2E, 0x34,
                0x0F,
            ],
        )?;
        self.command(
            0xE1,
            &[
                0x00, 0x0B, 0x11, 0x05, 0x13, 0x09, 0x33, 0x67, 0x48, 0x07, 0x0E, 0x0B, 0x2E, 0x33,
                0x0F,
            ],
        )?;
        self.command(0xB6, &[0x08, 0x82, 0x1D, 0x04])
    }

    pub fn set_orientation_and_apply(
        &mut self,
        orientation: DisplayOrientation,
    ) -> Result<(), DisplayError<SpiError, PinError>> {
        self.orientation = orientation;
        self.write_orientation()
    }

    fn write_orientation(&mut self) -> Result<(), DisplayError<SpiError, PinError>> {
        self.command(
            CMD_MEMORY_ACCESS_CONTROL,
            &[madctl_for_orientation(self.orientation)],
        )
    }

    pub fn command(
        &mut self,
        command: u8,
        data: &[u8],
    ) -> Result<(), DisplayError<SpiError, PinError>> {
        self.dc.set_low().map_err(DisplayError::Pin)?;
        self.spi.write(&[command]).map_err(DisplayError::Spi)?;
        if !data.is_empty() {
            self.dc.set_high().map_err(DisplayError::Pin)?;
            self.spi.write(data).map_err(DisplayError::Spi)?;
        }
        Ok(())
    }

    pub fn clear(&mut self, color: Rgb565) -> Result<(), DisplayError<SpiError, PinError>> {
        self.fill_solid(&Rectangle::new(Point::zero(), self.logical_size()), color)
    }

    pub fn blit_pixels<I>(
        &mut self,
        area: &Rectangle,
        pixels: I,
    ) -> Result<(), DisplayError<SpiError, PinError>>
    where
        I: IntoIterator<Item = Rgb565>,
    {
        let clipped = area.intersection(&self.bounding_box());
        if clipped.is_zero_sized() {
            return Ok(());
        }
        if self.orientation == DisplayOrientation::Landscape && clipped == *area {
            self.set_address_window(&clipped)?;
            self.write_color_stream(
                pixels,
                clipped.size.width.saturating_mul(clipped.size.height) as usize,
            )
        } else {
            self.fill_contiguous(area, pixels)
        }
    }

    fn flush_run(&mut self, run: PixelRun) -> Result<(), DisplayError<SpiError, PinError>> {
        self.fill_solid(&run.rectangle(), run.color)
    }

    fn flush_pixel_row(
        &mut self,
        row: &PixelRowBuffer,
    ) -> Result<(), DisplayError<SpiError, PinError>> {
        self.blit_pixels(&row.area(), row.colors().iter().copied())
    }

    fn set_address_window(
        &mut self,
        area: &Rectangle,
    ) -> Result<(), DisplayError<SpiError, PinError>> {
        let geometry = self.panel.geometry;
        let x0 = geometry.offset_x + area.top_left.x.max(0) as u16;
        let y0 = geometry.offset_y + area.top_left.y.max(0) as u16;
        let x1 = x0 + area.size.width.saturating_sub(1) as u16;
        let y1 = y0 + area.size.height.saturating_sub(1) as u16;
        self.command(
            CMD_COLUMN_ADDRESS_SET,
            &[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8],
        )?;
        self.command(
            CMD_ROW_ADDRESS_SET,
            &[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8],
        )?;
        self.dc.set_low().map_err(DisplayError::Pin)?;
        self.spi
            .write(&[CMD_MEMORY_WRITE])
            .map_err(DisplayError::Spi)?;
        self.dc.set_high().map_err(DisplayError::Pin)
    }

    fn write_repeated_color(
        &mut self,
        color: Rgb565,
        pixels: usize,
    ) -> Result<(), DisplayError<SpiError, PinError>> {
        let raw = color.into_storage().to_be_bytes();
        let mut chunk = [0u8; COLOR_STREAM_PIXELS * 2];
        let mut offset = 0;
        while offset < chunk.len() {
            chunk[offset] = raw[0];
            chunk[offset + 1] = raw[1];
            offset += 2;
        }
        let mut remaining = pixels;
        while remaining > 0 {
            let write_pixels = remaining.min(COLOR_STREAM_PIXELS);
            let write_len = write_pixels * 2;
            self.spi
                .write(&chunk[..write_len])
                .map_err(DisplayError::Spi)?;
            remaining -= write_pixels;
        }
        Ok(())
    }

    fn write_color_stream<I>(
        &mut self,
        pixels: I,
        max_pixels: usize,
    ) -> Result<(), DisplayError<SpiError, PinError>>
    where
        I: IntoIterator<Item = Rgb565>,
    {
        let mut chunk = [0u8; PIXEL_STREAM_PIXELS * 2];
        let mut buffered_pixels = 0usize;

        for (written_pixels, color) in pixels.into_iter().enumerate() {
            if written_pixels >= max_pixels {
                break;
            }
            let raw = color.into_storage().to_be_bytes();
            let offset = buffered_pixels * 2;
            chunk[offset] = raw[0];
            chunk[offset + 1] = raw[1];
            buffered_pixels += 1;

            if buffered_pixels == PIXEL_STREAM_PIXELS {
                self.spi.write(&chunk).map_err(DisplayError::Spi)?;
                buffered_pixels = 0;
            }
        }

        if buffered_pixels > 0 {
            self.spi
                .write(&chunk[..buffered_pixels * 2])
                .map_err(DisplayError::Spi)?;
        }
        Ok(())
    }
}

impl<SPI, DC, SDCS, SpiError, PinError> Display<SPI, DC, SDCS>
where
    SPI: LcdTransactionDevice<Error = SpiError>,
    DC: OutputPin<Error = PinError>,
    SDCS: OutputPin<Error = PinError>,
    SpiError: SpiErrorTrait,
    PinError: DigitalError,
{
    /// Runs several LCD command/data writes in one device-owned bus session.
    ///
    /// CoreS3's shared LCD device keeps LCD CS asserted and GPIO35 in D/C mode
    /// for this closure, then restores both chip selects high, GPIO35 to SD MISO,
    /// and SPI2 to SD-safe full-duplex mode before returning.
    pub fn with_lcd_transaction<R>(
        &mut self,
        operation: impl FnOnce(
            &mut DisplayTransaction<'_, SpiError>,
        ) -> Result<R, DisplayError<SpiError, PinError>>,
    ) -> Result<R, DisplayError<SpiError, PinError>> {
        let geometry = self.panel.geometry;
        self.spi
            .with_lcd_transaction(|writer| {
                let mut transaction = DisplayTransaction {
                    writer,
                    geometry,
                    stats: DisplayTransferStats {
                        logical_sessions: 1,
                        ..DisplayTransferStats::default()
                    },
                };
                operation(&mut transaction)
            })
            .map_err(|error| match error {
                LcdTransactionError::Operation(error) => error,
                LcdTransactionError::Device(error) => DisplayError::Spi(error),
            })
    }

    /// Zero-copy blit of tightly packed, big-endian RGB565 bytes.
    ///
    /// `area` must be fully inside the landscape display bounds. The byte slice
    /// must contain exactly two bytes per pixel in row-major, big-endian RGB565
    /// order. Validation completes before the shared LCD transaction begins.
    pub fn blit_rgb565_be(
        &mut self,
        area: &Rectangle,
        bytes: &[u8],
    ) -> Result<(), DisplayError<SpiError, PinError>> {
        if self.orientation != DisplayOrientation::Landscape {
            return Err(DisplayError::PixelData(
                PixelDataError::UnsupportedOrientation,
            ));
        }
        if area.is_zero_sized() {
            Self::validate_rgb565_be_len(0, 0, bytes.len()).map_err(DisplayError::PixelData)?;
            return Ok(());
        }
        if area.intersection(&self.bounding_box()) != *area {
            return Err(DisplayError::PixelData(PixelDataError::OutOfBounds));
        }
        Self::validate_rgb565_be_len(
            area.size.width as usize,
            area.size.height as usize,
            bytes.len(),
        )
        .map_err(DisplayError::PixelData)?;

        self.with_lcd_transaction(|transaction| {
            transaction
                .blit_rgb565_be(*area, bytes)
                .map_err(|error| match error {
                    DisplayTransactionError::Spi(error) => DisplayError::Spi(error),
                    DisplayTransactionError::PixelData(error) => DisplayError::PixelData(error),
                })
        })
    }
}

impl<SPI, DC, SDCS, SpiError, PinError> DrawTarget for Display<SPI, DC, SDCS>
where
    SPI: SpiDevice<Error = SpiError>,
    DC: OutputPin<Error = PinError>,
    SDCS: OutputPin<Error = PinError>,
    SpiError: SpiErrorTrait,
    PinError: DigitalError,
{
    type Color = Rgb565;
    type Error = DisplayError<SpiError, PinError>;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let mut row = None;
        for Pixel(point, color) in pixels {
            if !self.bounding_box().contains(point) {
                if let Some(current) = row.take() {
                    self.flush_pixel_row(&current)?;
                }
                continue;
            }
            if let Some(mut current) = row.take() {
                if current.try_push(point, color) {
                    row = Some(current);
                } else {
                    self.flush_pixel_row(&current)?;
                    row = Some(PixelRowBuffer::new(point, color));
                }
            } else {
                row = Some(PixelRowBuffer::new(point, color));
            }
        }
        if let Some(current) = row {
            self.flush_pixel_row(&current)?;
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let clipped = area.intersection(&self.bounding_box());
        if clipped.is_zero_sized() {
            return Ok(());
        }
        let native = self.map_rectangle_to_native(&clipped);
        self.set_address_window(&native)?;
        self.write_repeated_color(
            color,
            clipped.size.width.saturating_mul(clipped.size.height) as usize,
        )
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let clipped = area.intersection(&self.bounding_box());
        if clipped.is_zero_sized() {
            return Ok(());
        }
        if self.orientation == DisplayOrientation::Landscape && clipped == *area {
            self.set_address_window(&clipped)?;
            return self.write_color_stream(
                colors,
                clipped.size.width.saturating_mul(clipped.size.height) as usize,
            );
        }

        let area_width = area.size.width as i32;
        let mut run = None;
        for (index, color) in colors.into_iter().enumerate() {
            let index = index as i32;
            let point = Point::new(
                area.top_left.x + index % area_width,
                area.top_left.y + index / area_width,
            );
            if !clipped.contains(point) {
                if let Some(current) = run.take() {
                    self.flush_run(current)?;
                }
                continue;
            }
            if let Some(mut current) = run.take() {
                if current.try_extend(point, color) {
                    run = Some(current);
                } else {
                    self.flush_run(current)?;
                    run = Some(PixelRun::new(point, color));
                }
            } else {
                run = Some(PixelRun::new(point, color));
            }
        }
        if let Some(current) = run {
            self.flush_run(current)?;
        }
        Ok(())
    }
}

const fn madctl_for_orientation(orientation: DisplayOrientation) -> u8 {
    match orientation {
        DisplayOrientation::Landscape => 0x08,
        DisplayOrientation::LandscapeInverted => 0xC8,
        DisplayOrientation::Portrait => 0x68,
        DisplayOrientation::PortraitInverted => 0xA8,
    }
}

impl<SPI, DC, SDCS> OriginDimensions for Display<SPI, DC, SDCS> {
    fn size(&self) -> Size {
        self.logical_size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::{
        digital::{ErrorType, OutputPin},
        spi::{ErrorType as SpiErrorType, Operation},
    };

    #[derive(Default)]
    struct RecordingSpi {
        writes: std::vec::Vec<std::vec::Vec<u8>>,
        command_mode: bool,
        sessions: u32,
    }

    impl SpiErrorType for RecordingSpi {
        type Error = Infallible;
    }

    impl SpiDevice for RecordingSpi {
        fn transaction(&mut self, operations: &mut [Operation<'_, u8>]) -> Result<(), Self::Error> {
            for operation in operations {
                if let Operation::Write(bytes) = operation {
                    self.writes.push(bytes.to_vec());
                }
            }
            Ok(())
        }
    }

    impl LcdTransactionWriter for RecordingSpi {
        type Error = Infallible;

        fn set_command_mode(&mut self) -> Result<(), Self::Error> {
            self.command_mode = true;
            Ok(())
        }

        fn set_data_mode(&mut self) -> Result<(), Self::Error> {
            self.command_mode = false;
            Ok(())
        }

        fn write(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
            self.writes.push(bytes.to_vec());
            Ok(())
        }
    }

    impl LcdTransactionDevice for RecordingSpi {
        fn with_lcd_transaction<R, E>(
            &mut self,
            operation: impl FnOnce(&mut dyn LcdTransactionWriter<Error = Self::Error>) -> Result<R, E>,
        ) -> Result<R, LcdTransactionError<E, Self::Error>> {
            self.sessions += 1;
            operation(self).map_err(LcdTransactionError::Operation)
        }
    }

    #[derive(Default)]
    struct DummyPin;

    impl ErrorType for DummyPin {
        type Error = Infallible;
    }

    impl OutputPin for DummyPin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn display(width: u16, height: u16) -> Display<RecordingSpi, DummyPin, DummyPin> {
        Display::new(
            RecordingSpi::default(),
            DummyPin,
            DummyPin,
            BusConfig {
                write_hz: 40_000_000,
            },
            PanelConfig {
                geometry: DisplayGeometry {
                    width,
                    height,
                    offset_x: 0,
                    offset_y: 0,
                },
                invert_colors: true,
            },
        )
    }

    #[test]
    fn rgb565_length_validation_rejects_malformed_data() {
        type TestDisplay = Display<RecordingSpi, DummyPin, DummyPin>;
        assert_eq!(TestDisplay::validate_rgb565_be_len(2, 2, 8), Ok(8));
        assert_eq!(
            TestDisplay::validate_rgb565_be_len(2, 2, 7),
            Err(PixelDataError::OddByteLength)
        );
        assert_eq!(
            TestDisplay::validate_rgb565_be_len(2, 2, 6),
            Err(PixelDataError::LengthMismatch {
                expected: 8,
                actual: 6
            })
        );
        assert_eq!(
            TestDisplay::validate_rgb565_be_len(2, 2, 10),
            Err(PixelDataError::LengthMismatch {
                expected: 8,
                actual: 10
            })
        );
        assert_eq!(
            TestDisplay::rgb565_be_byte_len(usize::MAX, 2),
            Err(PixelDataError::LengthOverflow)
        );
        assert_eq!(TestDisplay::validate_rgb565_be_len(0, 0, 0), Ok(0));
    }

    #[test]
    fn invalid_rgb565_blits_do_not_start_a_session() {
        let mut display = display(4, 4);
        let area = Rectangle::new(Point::new(3, 3), Size::new(2, 2));
        assert_eq!(
            display.blit_rgb565_be(&area, &[0; 8]),
            Err(DisplayError::PixelData(PixelDataError::OutOfBounds))
        );
        let (spi, _, _) = display.release();
        assert_eq!(spi.sessions, 0);
    }

    #[test]
    fn rgb565_blit_uses_one_session_and_panel_ready_bytes() {
        let mut display = display(4, 4);
        let area = Rectangle::new(Point::new(1, 2), Size::new(2, 1));
        display
            .blit_rgb565_be(&area, &[0xF8, 0x00, 0x07, 0xE0])
            .unwrap();

        let (spi, _, _) = display.release();
        assert_eq!(spi.sessions, 1);
        assert_eq!(
            spi.writes,
            std::vec![
                std::vec![CMD_COLUMN_ADDRESS_SET],
                std::vec![0, 1, 0, 2],
                std::vec![CMD_ROW_ADDRESS_SET],
                std::vec![0, 2, 0, 2],
                std::vec![CMD_MEMORY_WRITE],
                std::vec![0xF8, 0x00, 0x07, 0xE0],
            ]
        );
    }

    #[test]
    fn rgb565_blit_chunks_without_copying_or_extra_sessions() {
        let bytes = std::vec![0x5A; RGB565_BE_WRITE_CHUNK_BYTES + 2];
        let mut display = display(2049, 1);
        let area = Rectangle::new(Point::zero(), Size::new(2049, 1));
        display.blit_rgb565_be(&area, &bytes).unwrap();

        let (spi, _, _) = display.release();
        assert_eq!(spi.sessions, 1);
        assert_eq!(spi.writes[5].len(), RGB565_BE_WRITE_CHUNK_BYTES);
        assert_eq!(spi.writes[6], std::vec![0x5A; 2]);
    }

    #[test]
    fn transaction_reports_reusable_transfer_statistics() {
        let mut display = display(2, 1);
        let stats = display
            .with_lcd_transaction(|transaction| {
                transaction
                    .blit_rgb565_be(
                        Rectangle::new(Point::zero(), Size::new(2, 1)),
                        &[0, 1, 2, 3],
                    )
                    .unwrap();
                Ok(transaction.stats())
            })
            .unwrap();

        assert_eq!(stats.logical_sessions, 1);
        assert_eq!(stats.address_windows, 1);
        assert_eq!(stats.spi_write_operations, 6);
        assert_eq!(stats.pixel_bytes, 4);
    }
}
