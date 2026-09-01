#![no_std]
#![no_main]

use core::{cell::RefCell, fmt::Write};

use core_s3::{
    CoreS3,
    audio::Aw88298,
    aw9523b::{Aw9523b, Port},
    bsp::{CoreS3DisplayResources, CoreS3GatewayH2Resources},
    display::DirtySprite,
    motion::{Bmi270, Bmi270Config, Vector3},
    power::{Axp2101, BatteryStatus, ChargeState, ExternalPower},
    rtc::{Bm8563, DateTime},
    sd,
    touch::Ft6336u,
    ui::{Label, StatusBar, Theme},
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use embedded_hal_bus::i2c::RefCellDevice;
use esp_backtrace as _;
use esp_hal::{delay::Delay, time::Duration};
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

const SPRITE_W: u16 = 284;
const SPRITE_H: u16 = 132;
const SPRITE_ORIGIN: Point = Point::new(20, 82);

type BoardSprite = DirtySprite<Rgb565, SPRITE_W, SPRITE_H, { 284 * 132 }, 8>;

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut delay = Delay::new();
    let mut sprite = BoardSprite::new(Rgb565::BLACK).expect("valid board sprite");

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

    let h2 = CoreS3::init_gateway_h2(CoreS3GatewayH2Resources {
        uart1: peripherals.UART1,
        tx: peripherals.GPIO1,
        rx: peripherals.GPIO2,
    });
    let h2_ok = h2.is_ok();
    let _h2 = h2.ok();

    let i2c_bus = RefCell::new(parts.internal_i2c);
    let mut pmic = Axp2101::new(RefCellDevice::new(&i2c_bus));
    let mut touch = Ft6336u::new(RefCellDevice::new(&i2c_bus));
    let mut imu = Bmi270::new(RefCellDevice::new(&i2c_bus));
    let mut rtc = Bm8563::new(RefCellDevice::new(&i2c_bus));
    let mut aw9523 = Aw9523b::new(RefCellDevice::new(&i2c_bus));
    let mut speaker = Aw88298::new(RefCellDevice::new(&i2c_bus));

    let pmic_ok = pmic.init_core_s3_defaults().is_ok();
    let touch_ok = touch.init().is_ok();
    let bmi_chip = imu.chip_id().ok();
    let imu_ok = imu
        .init_with_delay(Bmi270Config::DEFAULT, &mut delay)
        .is_ok();
    let rtc_ok = rtc.init().is_ok();
    let audio_ok = speaker.read_register16(0x00).is_ok();

    let display = &mut parts.display;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 v0.3 full board smoke",
    }
    .draw(display, Theme::DARK)
    .expect("status");
    Rectangle::new(Point::new(12, 34), Size::new(296, 192))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "FULL BOARD LIVE",
        top_left: Point::new(24, 56),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");

    let mut tick = 0u32;
    loop {
        delay.delay(Duration::from_millis(500));
        let battery = pmic.status().ok();
        let touch_count = touch
            .read_report()
            .ok()
            .map(|report| report.events.iter().filter(|event| event.is_some()).count() as u8);
        let accel = imu.acceleration_raw().ok();
        let datetime = rtc.datetime().ok();
        let vl_lost = rtc.clock_integrity_lost().ok();
        let sd_present = aw9523
            .read_input_port(Port::P0)
            .ok()
            .map(sd::core_s3_card_present_from_aw9523_p0);

        draw_dashboard(
            &mut sprite,
            Dashboard {
                tick,
                pmic_ok,
                touch_ok,
                bmi_chip,
                imu_ok,
                rtc_ok,
                audio_ok,
                h2_ok,
                battery,
                touch_count,
                accel,
                datetime,
                vl_lost,
                sd_present,
            },
        );
        sprite.flush_dirty_at(display, SPRITE_ORIGIN).ok();
        tick = tick.wrapping_add(1);
    }
}

#[derive(Clone, Copy)]
struct Dashboard {
    tick: u32,
    pmic_ok: bool,
    touch_ok: bool,
    bmi_chip: Option<u8>,
    imu_ok: bool,
    rtc_ok: bool,
    audio_ok: bool,
    h2_ok: bool,
    battery: Option<BatteryStatus>,
    touch_count: Option<u8>,
    accel: Option<Vector3>,
    datetime: Option<DateTime>,
    vl_lost: Option<bool>,
    sd_present: Option<bool>,
}

fn draw_dashboard(sprite: &mut BoardSprite, state: Dashboard) {
    sprite.clear(Rgb565::BLACK);
    let white = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let ok = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);
    let warn = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);
    let bad = MonoTextStyle::new(&FONT_6X10, Rgb565::RED);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);

    let mut line: String<96> = String::new();
    write!(&mut line, "tick:{:<5} LCD:OK", state.tick).unwrap();
    Text::new(&line, Point::new(0, 10), ok).draw(sprite).ok();

    line.clear();
    write!(&mut line, "AXP:{}", ok_text(state.pmic_ok)).unwrap();
    if let Some(battery) = state.battery {
        write!(
            &mut line,
            " {}mV {}% {} {}",
            battery.millivolts,
            battery.percentage,
            charge_text(battery.charge_state),
            external_text(battery.external_power)
        )
        .unwrap();
    } else {
        line.push_str(" battery:read failed").unwrap();
    }
    Text::new(
        &line,
        Point::new(0, 28),
        if state.battery.is_some() { white } else { bad },
    )
    .draw(sprite)
    .ok();

    line.clear();
    write!(
        &mut line,
        "Touch:{} points:{}  SD:{}",
        ok_text(state.touch_ok),
        option_u8(state.touch_count),
        sd_text(state.sd_present)
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(0, 46),
        if state.touch_ok { white } else { warn },
    )
    .draw(sprite)
    .ok();

    line.clear();
    write!(
        &mut line,
        "BMI270:{} chip:{} accel:{}",
        ok_text(state.imu_ok),
        hex_byte(state.bmi_chip),
        vector_text(state.accel)
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(0, 64),
        if state.imu_ok && state.accel.is_some() {
            white
        } else {
            warn
        },
    )
    .draw(sprite)
    .ok();

    line.clear();
    write!(
        &mut line,
        "BM8563:{} {} VL:{}",
        ok_text(state.rtc_ok),
        datetime_text(state.datetime),
        vl_text(state.vl_lost)
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(0, 82),
        if state.rtc_ok { white } else { warn },
    )
    .draw(sprite)
    .ok();

    line.clear();
    write!(
        &mut line,
        "Audio AW88298:{}  Gateway H2 UART:{}",
        ok_text(state.audio_ok),
        ok_text(state.h2_ok)
    )
    .unwrap();
    Text::new(
        &line,
        Point::new(0, 100),
        if state.audio_ok && state.h2_ok {
            accent
        } else {
            warn
        },
    )
    .draw(sprite)
    .ok();

    Text::new(
        "Live smoke only; protocol/filesystem stay app-owned",
        Point::new(0, 122),
        accent,
    )
    .draw(sprite)
    .ok();
}

fn ok_text(ok: bool) -> &'static str {
    if ok { "OK" } else { "FAIL" }
}

fn charge_text(state: ChargeState) -> &'static str {
    match state {
        ChargeState::Unknown => "unk",
        ChargeState::Discharging => "dis",
        ChargeState::Charging => "chg",
        ChargeState::Full => "full",
    }
}

fn external_text(state: ExternalPower) -> &'static str {
    match state {
        ExternalPower::Unknown => "ext?",
        ExternalPower::Disconnected => "bat",
        ExternalPower::Connected => "ext",
    }
}

fn sd_text(present: Option<bool>) -> &'static str {
    match present {
        Some(true) => "in",
        Some(false) => "out",
        None => "?",
    }
}

fn vl_text(lost: Option<bool>) -> &'static str {
    match lost {
        Some(true) => "LOST",
        Some(false) => "OK",
        None => "?",
    }
}

fn option_u8(value: Option<u8>) -> &'static str {
    match value {
        Some(0) => "0",
        Some(1) => "1",
        Some(2) => "2",
        _ => "?",
    }
}

fn hex_byte(value: Option<u8>) -> HexByte {
    HexByte(value)
}

fn vector_text(value: Option<Vector3>) -> VectorText {
    VectorText(value)
}

fn datetime_text(value: Option<DateTime>) -> DateTimeText {
    DateTimeText(value)
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

struct VectorText(Option<Vector3>);

impl core::fmt::Display for VectorText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(v) => write!(f, "{},{},{}", v.x, v.y, v.z),
            None => f.write_str("--,--,--"),
        }
    }
}

struct DateTimeText(Option<DateTime>);

impl core::fmt::Display for DateTimeText {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(dt) => write!(
                f,
                "{:02}:{:02}:{:02}",
                dt.time.hour, dt.time.minute, dt.time.second
            ),
            None => f.write_str("--:--:--"),
        }
    }
}
