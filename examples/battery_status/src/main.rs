#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    power::{Axp2101, BatteryStatus, ChargeState, ExternalPower},
    ui::{BatteryIndicator, Label, StatusBar, Theme},
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

    let mut pmic = Axp2101::new(parts.internal_i2c);
    let pmic_init = pmic.init_core_s3_defaults().is_ok();
    let delay = esp_hal::delay::Delay::new();

    let display = &mut parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 AXP2101 battery demo",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 190))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::GREEN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "BATTERY STATUS",
        top_left: Point::new(24, 62),
        color: Rgb565::GREEN,
    }
    .draw(display)
    .expect("title");
    Label {
        text: if pmic_init {
            "AXP2101 init: OK"
        } else {
            "AXP2101 init: FAILED"
        },
        top_left: Point::new(24, 88),
        color: if pmic_init { Rgb565::CYAN } else { Rgb565::RED },
    }
    .draw(display)
    .expect("init");

    let mut last_status: Option<BatteryStatus> = None;
    let mut last_read_ok = false;

    loop {
        delay.delay(Duration::from_millis(500));
        match pmic.status() {
            Ok(status) => {
                if last_status != Some(status) || !last_read_ok {
                    draw_battery_status(display, theme, status);
                    last_status = Some(status);
                    last_read_ok = true;
                }
            }
            Err(_) => {
                if last_read_ok {
                    clear_status_area(display);
                    Text::new(
                        "AXP2101 read failed",
                        Point::new(24, 124),
                        MonoTextStyle::new(&FONT_6X10, Rgb565::RED),
                    )
                    .draw(display)
                    .ok();
                    last_read_ok = false;
                }
            }
        }
    }
}

fn draw_battery_status<T>(display: &mut T, theme: Theme, status: BatteryStatus)
where
    T: DrawTarget<Color = Rgb565>,
{
    clear_status_area(display);
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::CYAN);
    let warning = MonoTextStyle::new(&FONT_6X10, Rgb565::YELLOW);

    let mut voltage: String<48> = String::new();
    write!(&mut voltage, "Voltage: {} mV", status.millivolts).unwrap();
    Text::new(&voltage, Point::new(24, 122), style)
        .draw(display)
        .ok();

    let mut pct: String<48> = String::new();
    let source = if status.percentage_estimated {
        "estimate"
    } else {
        "gauge"
    };
    write!(&mut pct, "Battery: {}% ({source})", status.percentage).unwrap();
    Text::new(&pct, Point::new(24, 140), accent)
        .draw(display)
        .ok();

    let state = match status.charge_state {
        ChargeState::Unknown => "charge: unknown",
        ChargeState::Discharging => "charge: discharging",
        ChargeState::Charging => "charge: charging",
        ChargeState::Full => "charge: full/external",
    };
    Text::new(state, Point::new(24, 158), style)
        .draw(display)
        .ok();

    let external = match status.external_power {
        ExternalPower::Unknown => "external: unknown",
        ExternalPower::Disconnected => "external: disconnected",
        ExternalPower::Connected => "external: connected",
    };
    Text::new(external, Point::new(24, 176), style)
        .draw(display)
        .ok();

    let present = match status.battery_present {
        Some(true) => "battery: present",
        Some(false) => "battery: absent",
        None => "battery: unknown",
    };
    Text::new(present, Point::new(24, 194), style)
        .draw(display)
        .ok();

    let low = if status.low_battery {
        "LOW BATTERY"
    } else {
        "battery threshold OK"
    };
    Text::new(
        low,
        Point::new(24, 210),
        if status.low_battery { warning } else { accent },
    )
    .draw(display)
    .ok();

    BatteryIndicator {
        bounds: Rectangle::new(Point::new(190, 124), Size::new(84, 18)),
        status,
    }
    .draw(display, theme)
    .ok();
}

fn clear_status_area<T>(display: &mut T)
where
    T: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(20, 110), Size::new(270, 116))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(display)
        .ok();
}
