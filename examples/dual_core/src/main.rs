#![no_std]
#![no_main]

use core::{
    fmt::Write,
    sync::atomic::{AtomicU32, Ordering},
};

use core_s3::{CoreS3, bsp::CoreS3DisplayResources};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use esp_backtrace as _;
use esp_hal::{
    system::{CpuControl, Stack},
    time::Duration,
};
use heapless::String;
use static_cell::StaticCell;

static APP_CORE_STACK: StaticCell<Stack<8192>> = StaticCell::new();
static APP_CORE_TICKS: AtomicU32 = AtomicU32::new(0);

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
    .expect("initialize CoreS3 display");

    parts
        .display
        .draw_validation_screen(
            "DUAL CORE",
            "Core 0 watches Core 1 progress",
            Rgb565::MAGENTA,
        )
        .expect("draw validation screen");

    Rectangle::new(Point::new(18, 78), Size::new(284, 118))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(&mut parts.display)
        .expect("clear status area");

    let small = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::MAGENTA);
    Text::new("Core 0: redraws the display", Point::new(34, 96), small)
        .draw(&mut parts.display)
        .expect("draw core 0 status");
    Text::new("Core 1: increments a counter", Point::new(34, 112), small)
        .draw(&mut parts.display)
        .expect("draw core 1 status");
    Text::new(
        "Core 0 changes 2/sec; Core 1 4/sec.",
        Point::new(34, 188),
        accent,
    )
    .draw(&mut parts.display)
    .expect("draw hint");

    draw_counters(&mut parts.display, 0, 0);

    let delay = esp_hal::delay::Delay::new();
    let mut cpu_control = CpuControl::new(peripherals.CPU_CTRL);
    let app_core_stack = APP_CORE_STACK.init(Stack::new());
    let _app_core_guard = cpu_control
        .start_app_core(app_core_stack, || app_core_task())
        .expect("start ESP32-S3 app core");

    let mut pro_frames = 0u32;
    loop {
        delay.delay(Duration::from_millis(500));
        pro_frames = pro_frames.wrapping_add(1);
        let app_ticks = APP_CORE_TICKS.load(Ordering::Relaxed);
        esp_println::println!("PRO frames={pro_frames}; APP ticks={app_ticks}");
        draw_counters(&mut parts.display, pro_frames, app_ticks);
    }
}

fn draw_counters<T>(display: &mut T, pro_frames: u32, app_ticks: u32)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(28, 124), Size::new(264, 54))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();

    Rectangle::new(Point::new(32, 128), Size::new(256, 20))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::MAGENTA, 1))
        .draw(display)
        .ok();
    Rectangle::new(Point::new(32, 154), Size::new(256, 20))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::MAGENTA, 1))
        .draw(display)
        .ok();

    let text = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);

    let mut core0: String<32> = String::new();
    write!(&mut core0, "Core 0 frames: {pro_frames}").unwrap();
    Text::with_alignment(&core0, Point::new(160, 142), text, Alignment::Center)
        .draw(display)
        .ok();

    let mut core1: String<32> = String::new();
    write!(&mut core1, "Core 1 ticks: {app_ticks}").unwrap();
    Text::with_alignment(&core1, Point::new(160, 168), text, Alignment::Center)
        .draw(display)
        .ok();
}

fn app_core_task() -> ! {
    let delay = esp_hal::delay::Delay::new();

    loop {
        delay.delay(Duration::from_millis(250));
        APP_CORE_TICKS.fetch_add(1, Ordering::Relaxed);
    }
}
