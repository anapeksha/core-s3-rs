#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    display::DirtySprite,
    motion::{Bmi270, Bmi270Config, Vector3, heading_centidegrees},
    ui::{Label, StatusBar, Theme},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use esp_backtrace as _;
use esp_hal::time::Duration;
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

const TEXT_W: u16 = 176;
const TEXT_H: u16 = 78;
const DIAL_W: u16 = 82;
const DIAL_H: u16 = 82;
const CALIBRATION_SAMPLES: u16 = 100;
const TEXT_ORIGIN: Point = Point::new(24, 120);
const DIAL_ORIGIN: Point = Point::new(205, 116);

type TextSprite = DirtySprite<Rgb565, TEXT_W, TEXT_H, { 176 * 78 }, 8>;
type DialSprite = DirtySprite<Rgb565, DIAL_W, DIAL_H, { 82 * 82 }, 8>;

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut text_sprite = TextSprite::new(Rgb565::BLACK).expect("valid text sprite");
    let mut dial_sprite = DialSprite::new(Rgb565::BLACK).expect("valid dial sprite");

    let mut parts = CoreS3::init_display(CoreS3DisplayResources {
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

    let mut imu = Bmi270::new(parts.internal_i2c);
    let mut delay = esp_hal::delay::Delay::new();
    let bmi_chip = imu.chip_id().ok();
    let imu_ok = imu
        .init_with_delay(Bmi270Config::DEFAULT, &mut delay)
        .is_ok();
    let bmm_chip = imu.init_bmm150_aux(&mut delay).ok();

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 BMM150 compass demo",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 34), Size::new(296, 194))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "COMPASS LIVE READ",
        top_left: Point::new(24, 48),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");

    draw_startup_status(display, bmi_chip, imu_ok, bmm_chip);
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Text::new(
        "Forward: screen right, camera bottom",
        Point::new(24, 104),
        style,
    )
    .draw(display)
    .ok();

    Rectangle::new(TEXT_ORIGIN - Point::new(2, 2), Size::new(180, 82))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(0, 18, 12), 1))
        .draw(display)
        .ok();
    Rectangle::new(DIAL_ORIGIN - Point::new(2, 2), Size::new(86, 86))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(0, 18, 12), 1))
        .draw(display)
        .ok();

    let mag_offset = run_m5_style_calibration(
        &mut imu,
        &mut delay,
        display,
        &mut text_sprite,
        &mut dial_sprite,
    );

    let mut raw_filter = MagFilter::new(5, 6);
    let mut display_filter = MagFilter::new(19, 20);
    let mut heading_filter = HeadingFilter::default();
    let mut tick = 0u32;

    loop {
        delay.delay(Duration::from_millis(150));
        tick = tick.wrapping_add(1);
        let int_status = imu.interrupt_status_1().ok();
        let pwr_status = imu.power_control().ok();
        let mag = imu.bmm150_aux_magnetic_raw_manual(&mut delay).ok();
        let filtered_mag = raw_filter.update(mag);
        let calibrated_mag = display_filter.update(filtered_mag.map(|mag| mag_offset.apply(mag)));
        let heading = heading_filter.update(calibrated_mag.and_then(m5_heading_centidegrees));

        draw_live_text(
            &mut text_sprite,
            tick,
            int_status,
            pwr_status,
            filtered_mag,
            calibrated_mag,
            heading,
            true,
        );
        text_sprite
            .flush_dirty_at(display, TEXT_ORIGIN)
            .expect("flush compass text");

        draw_dial(&mut dial_sprite, calibrated_mag, heading, true);
        dial_sprite
            .flush_dirty_at(display, DIAL_ORIGIN)
            .expect("flush compass dial");
    }
}

struct MagFilter {
    magnetic: Option<Vector3>,
    previous_weight: i32,
    divisor: i32,
}

impl MagFilter {
    fn new(previous_weight: i32, divisor: i32) -> Self {
        Self {
            magnetic: None,
            previous_weight,
            divisor,
        }
    }

    fn update(&mut self, magnetic: Option<Vector3>) -> Option<Vector3> {
        self.magnetic = match (self.magnetic, magnetic) {
            (Some(previous), Some(current)) => {
                let current_weight = self.divisor - self.previous_weight;
                let smoothed = Vector3::new(
                    ((previous.x * self.previous_weight) + (current.x * current_weight))
                        / self.divisor,
                    ((previous.y * self.previous_weight) + (current.y * current_weight))
                        / self.divisor,
                    ((previous.z * self.previous_weight) + (current.z * current_weight))
                        / self.divisor,
                );
                Some(stabilize_vector(previous, smoothed))
            }
            (_, current) => current,
        };
        self.magnetic
    }
}

#[derive(Clone, Copy, Default)]
struct MagOffset {
    offset: Vector3,
}

impl MagOffset {
    fn apply(self, value: Vector3) -> Vector3 {
        Vector3::new(
            value.x - self.offset.x,
            value.y - self.offset.y,
            value.z - self.offset.z,
        )
    }
}

#[derive(Default)]
struct MagCalibration {
    min: Option<Vector3>,
    max: Option<Vector3>,
}

impl MagCalibration {
    fn update(&mut self, value: Vector3) {
        self.min = Some(match self.min {
            Some(min) => Vector3::new(min.x.min(value.x), min.y.min(value.y), min.z.min(value.z)),
            None => value,
        });
        self.max = Some(match self.max {
            Some(max) => Vector3::new(max.x.max(value.x), max.y.max(value.y), max.z.max(value.z)),
            None => value,
        });
    }

    fn finish(self) -> MagOffset {
        match (self.min, self.max) {
            (Some(min), Some(max)) => MagOffset {
                offset: Vector3::new(
                    min.x + ((max.x - min.x) / 2),
                    min.y + ((max.y - min.y) / 2),
                    min.z + ((max.z - min.z) / 2),
                ),
            },
            _ => MagOffset::default(),
        }
    }
}

#[derive(Default)]
struct HeadingFilter {
    heading: Option<u16>,
}

impl HeadingFilter {
    fn update(&mut self, heading: Option<u16>) -> Option<u16> {
        self.heading = match (self.heading, heading) {
            (Some(previous), Some(current)) => {
                let delta = heading_delta(previous, current);
                if delta.abs() <= 150 {
                    Some(previous)
                } else {
                    Some(wrap_heading_centidegrees(i32::from(previous) + (delta / 5)))
                }
            }
            (_, current) => current,
        };
        self.heading
    }
}

fn stabilize_vector(previous: Vector3, current: Vector3) -> Vector3 {
    const DEAD_BAND: i32 = 3;
    Vector3::new(
        stabilize_axis(previous.x, current.x, DEAD_BAND),
        stabilize_axis(previous.y, current.y, DEAD_BAND),
        stabilize_axis(previous.z, current.z, DEAD_BAND),
    )
}

fn stabilize_axis(previous: i32, current: i32, dead_band: i32) -> i32 {
    if (current - previous).abs() <= dead_band {
        previous
    } else {
        current
    }
}

fn run_m5_style_calibration<I2C, Error>(
    imu: &mut Bmi270<I2C>,
    delay: &mut esp_hal::delay::Delay,
    display: &mut impl DrawTarget<Color = Rgb565>,
    text_sprite: &mut TextSprite,
    dial_sprite: &mut DialSprite,
) -> MagOffset
where
    I2C: embedded_hal::i2c::I2c<Error = Error>,
{
    let mut calibration = MagCalibration::default();
    for sample in 0..CALIBRATION_SAMPLES {
        delay.delay(Duration::from_millis(100));
        let mag = imu.bmm150_aux_magnetic_raw_manual(delay).ok();
        if let Some(mag) = mag {
            calibration.update(mag);
        }
        draw_calibration_progress(text_sprite, sample + 1, mag);
        text_sprite.flush_dirty_at(display, TEXT_ORIGIN).ok();
        draw_calibration_dial(dial_sprite);
        dial_sprite.flush_dirty_at(display, DIAL_ORIGIN).ok();
    }
    calibration.finish()
}

fn draw_calibration_progress<T>(sprite: &mut T, sample: u16, magnetic: Option<Vector3>)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(
        Point::zero(),
        Size::new(u32::from(TEXT_W), u32::from(TEXT_H)),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
    .draw(sprite)
    .ok();
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let mut line: String<64> = String::new();
    write!(
        &mut line,
        "M5 calibration {:>3}%",
        u32::from(sample) * 100 / u32::from(CALIBRATION_SAMPLES)
    )
    .unwrap();
    Text::new(&line, Point::new(0, 12), style).draw(sprite).ok();
    Text::new("Use figure-8 motion", Point::new(0, 30), style)
        .draw(sprite)
        .ok();
    Text::new("until progress ends", Point::new(0, 46), style)
        .draw(sprite)
        .ok();
    if let Some(mag) = magnetic {
        line.clear();
        write!(&mut line, "raw {:>4},{:>4},{:>4}", mag.x, mag.y, mag.z).unwrap();
        Text::new(&line, Point::new(0, 64), accent)
            .draw(sprite)
            .ok();
    }
}

fn draw_calibration_dial<T>(sprite: &mut T)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(
        Point::zero(),
        Size::new(u32::from(DIAL_W), u32::from(DIAL_H)),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
    .draw(sprite)
    .ok();
    let center = Point::new(i32::from(DIAL_W / 2), i32::from(DIAL_H / 2));
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    Circle::with_center(center, 76)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::YELLOW, 1))
        .draw(sprite)
        .ok();
    Text::with_alignment("CAL", center + Point::new(0, 4), style, Alignment::Center)
        .draw(sprite)
        .ok();
}

fn m5_heading_centidegrees(magnetic: Vector3) -> Option<u16> {
    heading_centidegrees(Vector3::new(magnetic.y, magnetic.x, magnetic.z))
}

fn heading_delta(previous: u16, current: u16) -> i32 {
    let mut delta = i32::from(current) - i32::from(previous);
    if delta > 18_000 {
        delta -= 36_000;
    } else if delta < -18_000 {
        delta += 36_000;
    }
    delta
}

fn wrap_heading_centidegrees(value: i32) -> u16 {
    let mut value = value % 36_000;
    if value < 0 {
        value += 36_000;
    }
    value as u16
}

fn draw_startup_status<T>(display: &mut T, bmi_chip: Option<u8>, imu_ok: bool, bmm_chip: Option<u8>)
where
    T: DrawTarget<Color = Rgb565>,
{
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);

    let mut line: String<64> = String::new();
    write!(
        &mut line,
        "BMI270: {} init: {}",
        HexByte(bmi_chip),
        if imu_ok { "OK" } else { "FAIL" }
    )
    .unwrap();
    Text::new(&line, Point::new(24, 72), if imu_ok { ok } else { warn })
        .draw(display)
        .ok();

    line.clear();
    write!(&mut line, "BMM150 via AUX: {}", HexByte(bmm_chip)).unwrap();
    Text::new(
        &line,
        Point::new(24, 88),
        if bmm_chip == Some(0x32) { ok } else { warn },
    )
    .draw(display)
    .ok();
}

fn draw_live_text<T>(
    sprite: &mut T,
    tick: u32,
    int_status: Option<u8>,
    pwr_status: Option<u8>,
    magnetic: Option<Vector3>,
    calibrated: Option<Vector3>,
    heading: Option<u16>,
    calibration_ready: bool,
) where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(
        Point::zero(),
        Size::new(u32::from(TEXT_W), u32::from(TEXT_H)),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
    .draw(sprite)
    .ok();

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    let mut line: String<64> = String::new();
    write!(
        &mut line,
        "tick:{:>5} i:{} p:{}",
        tick,
        HexByte(int_status),
        HexByte(pwr_status)
    )
    .unwrap();
    Text::new(&line, Point::new(0, 12), style).draw(sprite).ok();

    if let Some(mag) = magnetic {
        line.clear();
        write!(&mut line, "raw {:>4},{:>4},{:>4}", mag.x, mag.y, mag.z).unwrap();
        Text::new(&line, Point::new(0, 28), accent)
            .draw(sprite)
            .ok();
    } else {
        Text::new("mag read failed", Point::new(0, 28), error)
            .draw(sprite)
            .ok();
    }

    if let Some(mag) = calibrated {
        line.clear();
        write!(&mut line, "cal {:>4},{:>4},{:>4}", mag.x, mag.y, mag.z).unwrap();
        Text::new(&line, Point::new(0, 44), accent)
            .draw(sprite)
            .ok();
    }

    line.clear();
    match heading {
        Some(heading) => {
            let degrees = heading / 100;
            write!(&mut line, "{:>3}deg {}", degrees, heading_name(degrees)).unwrap();
            Text::new(&line, Point::new(0, 60), style).draw(sprite).ok();
        }
        None => {
            Text::new("heading unavailable", Point::new(0, 60), error)
                .draw(sprite)
                .ok();
        }
    }

    if !calibration_ready {
        Text::new("rotate flat to calibrate", Point::new(0, 76), warn)
            .draw(sprite)
            .ok();
    }
}

fn draw_dial<T>(
    sprite: &mut T,
    magnetic: Option<Vector3>,
    heading: Option<u16>,
    calibration_ready: bool,
) where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(
        Point::zero(),
        Size::new(u32::from(DIAL_W), u32::from(DIAL_H)),
    )
    .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
    .draw(sprite)
    .ok();

    let center = Point::new(i32::from(DIAL_W / 2), i32::from(DIAL_H / 2));
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Circle::with_center(center, 76)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 1))
        .draw(sprite)
        .ok();
    Text::with_alignment(
        "N",
        Point::new(center.x, center.y - 31),
        style,
        Alignment::Center,
    )
    .draw(sprite)
    .ok();
    Text::with_alignment(
        "S",
        Point::new(center.x, center.y + 38),
        style,
        Alignment::Center,
    )
    .draw(sprite)
    .ok();
    Text::with_alignment(
        "W",
        Point::new(center.x - 34, center.y + 4),
        style,
        Alignment::Center,
    )
    .draw(sprite)
    .ok();
    Text::with_alignment(
        "E",
        Point::new(center.x + 34, center.y + 4),
        style,
        Alignment::Center,
    )
    .draw(sprite)
    .ok();

    if !calibration_ready {
        let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
        Text::with_alignment("CAL", center + Point::new(0, 4), warn, Alignment::Center)
            .draw(sprite)
            .ok();
    } else if let Some(mag) = magnetic {
        draw_vector_needle(sprite, center, mag, calibration_ready);
    } else if let Some(heading) = heading {
        draw_quantized_heading_needle(sprite, center, heading / 100);
    }
}

fn draw_vector_needle<T>(sprite: &mut T, center: Point, magnetic: Vector3, calibration_ready: bool)
where
    T: DrawTarget<Color = Rgb565>,
{
    let max_axis = magnetic.x.abs().max(magnetic.y.abs()).max(1);
    let scale = 28;
    let endpoint = Point::new(
        center.x + (magnetic.x * scale / max_axis),
        center.y - (magnetic.y * scale / max_axis),
    );
    let color = if calibration_ready {
        Rgb565::YELLOW
    } else {
        Rgb565::new(31, 32, 0)
    };
    Line::new(center, endpoint)
        .into_styled(PrimitiveStyle::with_stroke(color, 3))
        .draw(sprite)
        .ok();
    Circle::with_center(endpoint, 8)
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(sprite)
        .ok();
}

fn draw_quantized_heading_needle<T>(sprite: &mut T, center: Point, degrees: u16)
where
    T: DrawTarget<Color = Rgb565>,
{
    let endpoint = match ((degrees + 23) / 45) % 8 {
        0 => Point::new(center.x, center.y - 28),
        1 => Point::new(center.x + 20, center.y - 20),
        2 => Point::new(center.x + 28, center.y),
        3 => Point::new(center.x + 20, center.y + 20),
        4 => Point::new(center.x, center.y + 28),
        5 => Point::new(center.x - 20, center.y + 20),
        6 => Point::new(center.x - 28, center.y),
        _ => Point::new(center.x - 20, center.y - 20),
    };
    Line::new(center, endpoint)
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::YELLOW, 3))
        .draw(sprite)
        .ok();
}

fn heading_name(degrees: u16) -> &'static str {
    match ((degrees + 23) / 45) % 8 {
        0 => "N",
        1 => "NE",
        2 => "E",
        3 => "SE",
        4 => "S",
        5 => "SW",
        6 => "W",
        _ => "NW",
    }
}

struct HexByte(Option<u8>);

impl core::fmt::Display for HexByte {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(value) => write!(f, "0x{value:02X}"),
            None => f.write_str("--"),
        }
    }
}
