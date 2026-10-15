#![no_std]
#![no_main]

use core::{fmt::Write, future::pending};

use core_s3::{
    CoreS3,
    bsp::{CoreS3Display, CoreS3DisplayResources},
    rtos::{CoreS3Runtime, Cpu1WorkerStack, PrimaryExecutorStorage, RuntimeConfig},
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, signal::Signal,
};
use embassy_time::{Duration, Ticker, with_timeout};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
};
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use heapless::String;
const ITERATIONS: u32 = 16;
static WORKER_STACK: Cpu1WorkerStack<8192> = Cpu1WorkerStack::new();
static EXECUTOR: PrimaryExecutorStorage = PrimaryExecutorStorage::new();
static MESSAGES: Channel<CriticalSectionRawMutex, u32, 4> = Channel::new();
static DONE: Signal<CriticalSectionRawMutex, u32> = Signal::new();

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

    parts.display.clear(Rgb565::BLACK).expect("clear");
    Rectangle::new(Point::new(0, 0), Size::new(320, 24))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::MAGENTA))
        .draw(&mut parts.display)
        .expect("header");
    let header = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Text::new("DUAL CORE", Point::new(8, 16), header)
        .draw(&mut parts.display)
        .expect("header text");

    Rectangle::new(Point::new(18, 78), Size::new(284, 118))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(&mut parts.display)
        .expect("clear status area");

    let small = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::MAGENTA);
    Text::new("Core 0: Embassy + display", Point::new(34, 96), small)
        .draw(&mut parts.display)
        .expect("draw core 0 status");
    Text::new("Core 1: blocking worker", Point::new(34, 112), small)
        .draw(&mut parts.display)
        .expect("draw core 1 status");
    Text::new(
        "16 bounded cross-core messages",
        Point::new(34, 188),
        accent,
    )
    .draw(&mut parts.display)
    .expect("draw hint");

    draw_counters(&mut parts.display, 0, 0);

    let timers = TimerGroup::new(peripherals.TIMG0);
    let runtime = CoreS3Runtime::start(
        RuntimeConfig::default(),
        timers.timer0,
        peripherals.FROM_CPU_INTR0,
    )
    .expect("start CoreS3 runtime");
    runtime
        .start_worker(
            peripherals.CPU_CTRL,
            peripherals.FROM_CPU_INTR1,
            &WORKER_STACK,
            app_core_task,
        )
        .expect("start CPU1 worker");
    match runtime.run(&EXECUTOR, move |spawner| {
        spawner.spawn(primary_task(parts.display).expect("spawn primary task"));
    }) {
        Ok(never) => match never {},
        Err(error) => panic!("primary executor: {error:?}"),
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

fn app_core_task() {
    let delay = esp_hal::delay::Delay::new();
    let mut sent = 0;
    for sequence in 1..=ITERATIONS {
        delay.delay(esp_hal::time::Duration::from_millis(250));
        if MESSAGES.try_send(sequence).is_ok() {
            sent += 1;
        }
    }
    DONE.signal(sent);
}

#[embassy_executor::task]
async fn primary_task(mut display: CoreS3Display) {
    let mut ticker = Ticker::every(Duration::from_millis(100));
    let mut received = 0;
    let mut ordered = true;
    for expected in 1..=ITERATIONS {
        ticker.next().await;
        match with_timeout(Duration::from_secs(2), MESSAGES.receive()).await {
            Ok(sequence) => {
                received += 1;
                ordered &= sequence == expected;
                draw_counters(&mut display, expected, sequence);
                esp_println::println!("CPU0 periodic work={expected}; CPU1 message={sequence}");
            }
            Err(_) => break,
        }
    }
    let completed = with_timeout(Duration::from_secs(6), DONE.wait()).await;
    let passed = ordered && received == ITERATIONS && completed == Ok(ITERATIONS);
    let result = if passed { "PASS" } else { "FAIL" };
    esp_println::println!("Dual-core {result}: received={received}, worker sent={completed:?}");
    Text::new(
        result,
        Point::new(140, 216),
        MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE),
    )
    .draw(&mut display)
    .ok();
    pending::<()>().await;
}
