#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    motion::{Bmi270, Bmi270Config, Vector3},
    ui::{Label, StatusBar, Theme},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use esp_hal::time::Duration;
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
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
    let chip_id = imu.chip_id().ok();
    let mut delay = esp_hal::delay::Delay::new();
    let imu_init = imu
        .init_with_delay(Bmi270Config::DEFAULT, &mut delay)
        .is_ok();
    let internal_status = imu.internal_status().ok();
    let power_control = imu.power_control().ok();

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 BMI270 IMU demo",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "IMU LIVE READ",
        top_left: Point::new(24, 62),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");

    let mut chip_line: String<48> = String::new();
    match chip_id {
        Some(id) => write!(&mut chip_line, "BMI270 chip id: 0x{:02X}", id).unwrap(),
        None => chip_line.push_str("BMI270 chip id: read failed").unwrap(),
    }
    Label {
        text: &chip_line,
        top_left: Point::new(24, 88),
        color: if chip_id == Some(0x24) {
            Rgb565::CYAN
        } else {
            Rgb565::YELLOW
        },
    }
    .draw(display)
    .expect("chip id");
    Label {
        text: if imu_init {
            "BMI270 init: OK"
        } else {
            "BMI270 init: FAILED"
        },
        top_left: Point::new(24, 106),
        color: if imu_init { Rgb565::CYAN } else { Rgb565::RED },
    }
    .draw(display)
    .expect("init");

    let mut status_line: String<64> = String::new();
    write!(
        &mut status_line,
        "internal: {} pwr: {}",
        HexByte(internal_status),
        HexByte(power_control)
    )
    .unwrap();
    Label {
        text: &status_line,
        top_left: Point::new(24, 124),
        color: if internal_status.map(|s| s & 0x0F) == Some(0x01) {
            Rgb565::CYAN
        } else {
            Rgb565::YELLOW
        },
    }
    .draw(display)
    .expect("status");

    let calibration = calibrate_imu(&mut imu, &mut delay, display);
    imu.set_accel_offset(calibration.accel_offset);
    imu.set_gyro_offset(calibration.gyro_offset);
    draw_calibration(display, calibration);

    let mut filter = ImuFilter::default();
    let mut last_accel = None;
    let mut last_gyro = None;

    loop {
        delay.delay(Duration::from_millis(100));
        let int_status = imu.interrupt_status_1().ok();
        let accel = imu.acceleration_raw().ok();
        let gyro = imu.gyroscope_raw().ok();
        let filtered = filter.update(accel, gyro);
        if filtered.accel != last_accel || filtered.gyro != last_gyro {
            draw_imu(display, int_status, filtered.accel, filtered.gyro);
            last_accel = filtered.accel;
            last_gyro = filtered.gyro;
        }
    }
}

#[derive(Clone, Copy)]
struct ImuCalibration {
    accel_offset: Vector3,
    gyro_offset: Vector3,
}

#[derive(Default)]
struct ImuFilter {
    accel: Option<Vector3>,
    gyro: Option<Vector3>,
}

struct FilteredImu {
    accel: Option<Vector3>,
    gyro: Option<Vector3>,
}

impl ImuFilter {
    fn update(&mut self, accel: Option<Vector3>, gyro: Option<Vector3>) -> FilteredImu {
        self.accel = smooth_vector(self.accel, accel);
        self.gyro = smooth_vector(self.gyro, gyro).map(apply_deadband);
        FilteredImu {
            accel: self.accel,
            gyro: self.gyro,
        }
    }
}

fn smooth_vector(previous: Option<Vector3>, current: Option<Vector3>) -> Option<Vector3> {
    match (previous, current) {
        (Some(prev), Some(now)) => Some(Vector3::new(
            smooth_axis(prev.x, now.x),
            smooth_axis(prev.y, now.y),
            smooth_axis(prev.z, now.z),
        )),
        (_, current) => current,
    }
}

fn smooth_axis(previous: i32, current: i32) -> i32 {
    ((previous * 7) + current) / 8
}

fn apply_deadband(value: Vector3) -> Vector3 {
    const GYRO_DEADBAND: i32 = 8;
    Vector3::new(
        deadband_axis(value.x, GYRO_DEADBAND),
        deadband_axis(value.y, GYRO_DEADBAND),
        deadband_axis(value.z, GYRO_DEADBAND),
    )
}

fn deadband_axis(value: i32, threshold: i32) -> i32 {
    if value.abs() <= threshold { 0 } else { value }
}

fn calibrate_imu<I2C, Error>(
    imu: &mut Bmi270<I2C>,
    delay: &mut esp_hal::delay::Delay,
    display: &mut impl DrawTarget<Color = Rgb565>,
) -> ImuCalibration
where
    I2C: embedded_hal::i2c::I2c<Error = Error>,
{
    const SAMPLES: i32 = 96;
    const ACCEL_1G_COUNTS: i32 = 8192;

    Rectangle::new(Point::new(20, 142), Size::new(274, 72))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    Text::new("Calibrating: keep device still", Point::new(24, 158), style)
        .draw(display)
        .ok();

    let mut accel_sum = Vector3::default();
    let mut gyro_sum = Vector3::default();
    let mut count = 0;

    while count < SAMPLES {
        delay.delay(Duration::from_millis(10));
        let accel = imu.acceleration_raw();
        let gyro = imu.gyroscope_raw();
        if let (Ok(a), Ok(g)) = (accel, gyro) {
            accel_sum = Vector3::new(accel_sum.x + a.x, accel_sum.y + a.y, accel_sum.z + a.z);
            gyro_sum = Vector3::new(gyro_sum.x + g.x, gyro_sum.y + g.y, gyro_sum.z + g.z);
            count += 1;
        }
    }

    let accel_avg = div_vector(accel_sum, SAMPLES);
    let gyro_avg = div_vector(gyro_sum, SAMPLES);
    let gravity = expected_gravity(accel_avg, ACCEL_1G_COUNTS);

    ImuCalibration {
        accel_offset: Vector3::new(
            accel_avg.x - gravity.x,
            accel_avg.y - gravity.y,
            accel_avg.z - gravity.z,
        ),
        gyro_offset: gyro_avg,
    }
}

fn div_vector(value: Vector3, divisor: i32) -> Vector3 {
    Vector3::new(value.x / divisor, value.y / divisor, value.z / divisor)
}

fn expected_gravity(value: Vector3, one_g: i32) -> Vector3 {
    let abs_x = value.x.abs();
    let abs_y = value.y.abs();
    let abs_z = value.z.abs();
    if abs_x >= abs_y && abs_x >= abs_z {
        Vector3::new(value.x.signum() * one_g, 0, 0)
    } else if abs_y >= abs_z {
        Vector3::new(0, value.y.signum() * one_g, 0)
    } else {
        Vector3::new(0, 0, value.z.signum() * one_g)
    }
}

fn draw_calibration<T>(display: &mut T, calibration: ImuCalibration)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(20, 142), Size::new(274, 72))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);
    let mut line: String<64> = String::new();
    write!(
        &mut line,
        "acc offset {:>5},{:>5},{:>5}",
        calibration.accel_offset.x, calibration.accel_offset.y, calibration.accel_offset.z
    )
    .unwrap();
    Text::new(&line, Point::new(24, 158), style)
        .draw(display)
        .ok();
    line.clear();
    write!(
        &mut line,
        "gyr offset {:>5},{:>5},{:>5}",
        calibration.gyro_offset.x, calibration.gyro_offset.y, calibration.gyro_offset.z
    )
    .unwrap();
    Text::new(&line, Point::new(24, 176), style)
        .draw(display)
        .ok();
    Text::new("Showing calibrated + filtered", Point::new(24, 194), style)
        .draw(display)
        .ok();
}

fn draw_imu<T>(
    display: &mut T,
    int_status: Option<u8>,
    accel: Option<Vector3>,
    gyro: Option<Vector3>,
) where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(20, 142), Size::new(274, 72))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let error = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);

    if let Some(a) = accel {
        let mut line: String<64> = String::new();
        write!(&mut line, "acc raw x={:>6} y={:>6} z={:>6}", a.x, a.y, a.z).unwrap();
        Text::new(&line, Point::new(24, 158), accent)
            .draw(display)
            .ok();
    } else {
        Text::new("acc read failed", Point::new(24, 158), error)
            .draw(display)
            .ok();
    }

    if let Some(g) = gyro {
        let mut line: String<64> = String::new();
        write!(&mut line, "gyr raw x={:>6} y={:>6} z={:>6}", g.x, g.y, g.z).unwrap();
        Text::new(&line, Point::new(24, 176), style)
            .draw(display)
            .ok();
    } else {
        Text::new("gyro read failed", Point::new(24, 176), error)
            .draw(display)
            .ok();
    }

    let mut status_line: String<32> = String::new();
    write!(&mut status_line, "int status: {}", HexByte(int_status)).unwrap();
    Text::new(&status_line, Point::new(24, 194), style)
        .draw(display)
        .ok();
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
