#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::{CoreS3CameraResources, CoreS3DisplayResources},
    camera::{CameraConfig, DigitalZoom, QQVGA_RGB565_FRAME_BUFFER_BYTES},
    touch::{Ft6336u, TouchPhase},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::{Rgb565, RgbColor},
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use esp_hal::{delay::Delay, dma_rx_buffer};
use esp_println::println;
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

const PREVIEW_WIDTH: u32 = 160;
const INITIAL_SENSOR_ZOOM: DigitalZoom = DigitalZoom::X2;
const PREVIEW_ORIGIN: Point = Point::new(8, 54);
const STATUS_AREA: Rectangle = Rectangle::new(Point::new(0, 184), Size::new(320, 56));
const ZOOM_SLIDER_AREA: Rectangle = Rectangle::new(Point::new(184, 54), Size::new(128, 124));
const ZOOM_BUTTON_X: i32 = 248;
const ZOOM_BUTTON_WIDTH: u32 = 84;
const ZOOM_BUTTON_HEIGHT: u32 = 28;
const ZOOM_STOP_Y1: i32 = 78;
const ZOOM_STOP_Y2: i32 = 116;
const ZOOM_STOP_Y4: i32 = 154;
const FRAME_DELAY_MS: u32 = 10;

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    println!("CoreS3 live camera preview boot");
    println!(
        "Camera pin map: xclk=GPIO2, sccb=GPIO12/GPIO11, pclk=GPIO45, vsync=GPIO46, href=GPIO38, d0..d7=GPIO39/40/41/42/15/16/48/47"
    );

    let mut display_parts = CoreS3::init_display(CoreS3DisplayResources {
        i2c0: peripherals.I2C0,
        i2c_sda: peripherals.GPIO12,
        i2c_scl: peripherals.GPIO11,
        spi2: peripherals.SPI2,
        lcd_sclk: peripherals.GPIO36,
        lcd_mosi: peripherals.GPIO37,
        lcd_dc: peripherals.GPIO35,
        lcd_cs: peripherals.GPIO3,
        tf_card_cs: peripherals.GPIO4,
    })
    .expect("display");

    draw_static_ui(&mut display_parts.display);

    let mut camera = CoreS3::init_camera(
        CoreS3CameraResources {
            lcd_cam: peripherals.LCD_CAM,
            dma_ch0: peripherals.DMA_CH0,
            internal_i2c: display_parts.internal_i2c,
            xclk: peripherals.GPIO2,
            pclk: peripherals.GPIO45,
            vsync: peripherals.GPIO46,
            href: peripherals.GPIO38,
            d0: peripherals.GPIO39,
            d1: peripherals.GPIO40,
            d2: peripherals.GPIO41,
            d3: peripherals.GPIO42,
            d4: peripherals.GPIO15,
            d5: peripherals.GPIO16,
            d6: peripherals.GPIO48,
            d7: peripherals.GPIO47,
        },
        CameraConfig::qqvga_rgb565().with_zoom(INITIAL_SENSOR_ZOOM),
    )
    .expect("camera init");

    println!("CoreS3 camera sensor: {:?}", camera.sensor());
    println!("CoreS3 camera config: {:?}", camera.config());

    draw_status(&mut display_parts.display, "Camera initialized", 0, 0, 0);
    draw_zoom_slider(&mut display_parts.display, camera.config().zoom);

    let touch_ready = {
        let mut touch = Ft6336u::new(camera.internal_i2c_mut());
        touch.init().is_ok()
    };
    println!(
        "CoreS3 touch zoom slider: {}",
        if touch_ready { "OK" } else { "unavailable" }
    );

    let mut dma_buffer =
        dma_rx_buffer!(QQVGA_RGB565_FRAME_BUFFER_BYTES).expect("camera DMA buffer");

    match camera.capture_dma_frame(dma_buffer) {
        Ok((_, returned)) => {
            println!("ERROR: capture before start unexpectedly succeeded");
            dma_buffer = returned;
        }
        Err(error) => {
            println!("capture before start returned {:?}", error);
            dma_buffer =
                dma_rx_buffer!(QQVGA_RGB565_FRAME_BUFFER_BYTES).expect("replacement DMA buffer");
        }
    }

    camera.start().expect("camera start");
    println!("CoreS3 camera live preview started");

    let mut frames = 0u32;
    let mut errors = 0u32;
    loop {
        match camera.capture_dma_frame(dma_buffer) {
            Ok((info, returned)) => {
                dma_buffer = returned;
                frames = frames.wrapping_add(1);
                let frame_len = info.len.min(dma_buffer.as_slice().len());
                let checksum = checksum32(&dma_buffer.as_slice()[..frame_len]);
                let scale = (PREVIEW_WIDTH / info.width as u32).max(1);
                let pixels = ScaledRgb565Pixels::new(
                    &dma_buffer.as_slice()[..frame_len],
                    info.width as usize,
                    info.height as usize,
                    scale as usize,
                );
                let preview = Rectangle::new(
                    PREVIEW_ORIGIN,
                    Size::new(info.width as u32 * scale, info.height as u32 * scale),
                );
                if display_parts.display.blit_pixels(&preview, pixels).is_err() {
                    errors = errors.wrapping_add(1);
                    println!("LCD preview blit failed: frame={frames}, errors={errors}");
                    draw_status(
                        &mut display_parts.display,
                        "LCD blit failed",
                        frames,
                        errors,
                        checksum,
                    );
                } else if frames == 1 || frames.is_multiple_of(10) {
                    println!(
                        "live frame {frames}: {}x{}, zoom={:?}, scale={}x, len={}, received={}, checksum=0x{:08x}",
                        info.width,
                        info.height,
                        camera.config().zoom,
                        scale,
                        info.len,
                        dma_buffer.number_of_received_bytes(),
                        checksum
                    );
                    draw_status(
                        &mut display_parts.display,
                        "Live preview",
                        frames,
                        errors,
                        checksum,
                    );
                }
            }
            Err(error) => {
                errors = errors.wrapping_add(1);
                println!("camera capture failed: frame={frames}, errors={errors}, err={error:?}");
                draw_status(
                    &mut display_parts.display,
                    "Capture failed",
                    frames,
                    errors,
                    0,
                );
                dma_buffer = dma_rx_buffer!(QQVGA_RGB565_FRAME_BUFFER_BYTES)
                    .expect("replacement DMA buffer");
                camera.stop();
                camera.start().expect("camera restart");
            }
        }

        if touch_ready
            && let Some(next_zoom) = read_zoom_slider_touch(camera.internal_i2c_mut())
            && next_zoom != camera.config().zoom
        {
            println!(
                "CoreS3 camera zoom change: {:?} -> {:?}",
                camera.config().zoom,
                next_zoom
            );
            match camera.set_zoom(next_zoom) {
                Ok(()) => {
                    draw_status(
                        &mut display_parts.display,
                        "Zoom changed",
                        frames,
                        errors,
                        0,
                    );
                    draw_zoom_slider(&mut display_parts.display, next_zoom);
                }
                Err(error) => {
                    errors = errors.wrapping_add(1);
                    println!("camera zoom change failed: zoom={next_zoom:?}, err={error:?}");
                    draw_status(
                        &mut display_parts.display,
                        "Zoom change failed",
                        frames,
                        errors,
                        0,
                    );
                }
            }
        }

        delay.delay_millis(FRAME_DELAY_MS);
    }
}

fn draw_static_ui<T>(display: &mut T)
where
    T: DrawTarget<Color = Rgb565>,
{
    display.clear(Rgb565::BLACK).ok();
    let title = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let label = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Text::new("CoreS3 CAMERA LIVE", Point::new(82, 20), title)
        .draw(display)
        .ok();
    Text::new("QQVGA RGB565 touch zoom", Point::new(80, 38), label)
        .draw(display)
        .ok();
    Rectangle::new(PREVIEW_ORIGIN - Point::new(2, 2), Size::new(164, 124))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .ok();
    Rectangle::new(STATUS_AREA.top_left, STATUS_AREA.size)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(8, 24, 8), 1))
        .draw(display)
        .ok();
}

fn draw_status<T>(display: &mut T, message: &str, frames: u32, errors: u32, checksum: u32)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(4, 188), Size::new(312, 46))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);

    Text::new(message, Point::new(12, 202), accent)
        .draw(display)
        .ok();

    let mut line: String<96> = String::new();
    write!(
        &mut line,
        "frames={frames} errors={errors} sum={checksum:08x}"
    )
    .ok();
    Text::new(&line, Point::new(12, 222), style)
        .draw(display)
        .ok();
}

fn draw_zoom_slider<T>(display: &mut T, zoom: DigitalZoom)
where
    T: DrawTarget<Color = Rgb565>,
{
    ZOOM_SLIDER_AREA
        .into_styled(PrimitiveStyle::with_fill(Rgb565::new(0, 8, 12)))
        .draw(display)
        .ok();
    ZOOM_SLIDER_AREA
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(display)
        .ok();

    let label = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let active = MonoTextStyle::new(&FONT_6X10, Rgb565::BLACK);
    Text::new("ZOOM", Point::new(236, 68), label)
        .draw(display)
        .ok();

    draw_zoom_stop(
        display,
        DigitalZoom::X1,
        zoom,
        "1x",
        ZOOM_STOP_Y1,
        label,
        active,
    );
    draw_zoom_stop(
        display,
        DigitalZoom::X2,
        zoom,
        "2x",
        ZOOM_STOP_Y2,
        label,
        active,
    );
    draw_zoom_stop(
        display,
        DigitalZoom::X4,
        zoom,
        "4x",
        ZOOM_STOP_Y4,
        label,
        active,
    );
}

fn draw_zoom_stop<T>(
    display: &mut T,
    stop: DigitalZoom,
    zoom: DigitalZoom,
    text: &str,
    y: i32,
    label: MonoTextStyle<'_, Rgb565>,
    active: MonoTextStyle<'_, Rgb565>,
) where
    T: DrawTarget<Color = Rgb565>,
{
    let selected = stop == zoom;
    let fill = if selected {
        Rgb565::YELLOW
    } else {
        Rgb565::new(0, 18, 24)
    };
    let stroke = if selected {
        Rgb565::WHITE
    } else {
        Rgb565::CYAN
    };
    let button = zoom_button_rect(y);
    button
        .into_styled(PrimitiveStyle::with_fill(fill))
        .draw(display)
        .ok();
    button
        .into_styled(PrimitiveStyle::with_stroke(stroke, 2))
        .draw(display)
        .ok();
    Text::new(
        text,
        Point::new(ZOOM_BUTTON_X - 7, y + 4),
        if selected { active } else { label },
    )
    .draw(display)
    .ok();
}

fn zoom_button_rect(center_y: i32) -> Rectangle {
    Rectangle::new(
        Point::new(
            ZOOM_BUTTON_X - (ZOOM_BUTTON_WIDTH as i32 / 2),
            center_y - (ZOOM_BUTTON_HEIGHT as i32 / 2),
        ),
        Size::new(ZOOM_BUTTON_WIDTH, ZOOM_BUTTON_HEIGHT),
    )
}

fn read_zoom_slider_touch(i2c: &mut core_s3::bsp::CoreS3I2c) -> Option<DigitalZoom> {
    let mut touch = Ft6336u::new(i2c);
    let report = touch.read_report().ok()?;
    let event = report.events.into_iter().flatten().next()?;
    if !matches!(event.phase, TouchPhase::Down | TouchPhase::Move) || !event.hits(ZOOM_SLIDER_AREA)
    {
        return None;
    }

    let y = event.point.y;
    let d1 = (y - ZOOM_STOP_Y1).abs();
    let d2 = (y - ZOOM_STOP_Y2).abs();
    let d4 = (y - ZOOM_STOP_Y4).abs();
    if d1 <= d2 && d1 <= d4 {
        Some(DigitalZoom::X1)
    } else if d2 <= d4 {
        Some(DigitalZoom::X2)
    } else {
        Some(DigitalZoom::X4)
    }
}

fn checksum32(data: &[u8]) -> u32 {
    data.iter().fold(0u32, |acc, byte| {
        acc.rotate_left(5).wrapping_add(*byte as u32)
    })
}

struct ScaledRgb565Pixels<'a> {
    bytes: &'a [u8],
    source_width: usize,
    source_height: usize,
    scale: usize,
    out_index: usize,
}

impl<'a> ScaledRgb565Pixels<'a> {
    fn new(bytes: &'a [u8], source_width: usize, source_height: usize, scale: usize) -> Self {
        Self {
            bytes,
            source_width,
            source_height,
            scale,
            out_index: 0,
        }
    }
}

impl Iterator for ScaledRgb565Pixels<'_> {
    type Item = Rgb565;

    fn next(&mut self) -> Option<Self::Item> {
        let out_width = self.source_width * self.scale;
        let out_height = self.source_height * self.scale;
        if self.out_index >= out_width * out_height {
            return None;
        }
        let out_x = self.out_index % out_width;
        let out_y = self.out_index / out_width;
        self.out_index += 1;
        let source_x = out_x / self.scale;
        let source_y = out_y / self.scale;
        let source_index = (source_y * self.source_width + source_x) * 2;
        let pixel = self.bytes.get(source_index..source_index + 2)?;
        let raw = u16::from_be_bytes([pixel[0], pixel[1]]);
        let r = ((raw >> 11) & 0x1f) as u8;
        let g = ((raw >> 5) & 0x3f) as u8;
        let b = (raw & 0x1f) as u8;
        Some(Rgb565::new(r, g, b))
    }
}
