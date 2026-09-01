#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::CoreS3DisplayResources,
    gateway_h2::{
        GatewayH2,
        matter::{MatterOverThreadConfig, MatterServerConfig, ThreadDatasetConfig},
    },
};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_6X10},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::Text,
};
use esp_backtrace as _;
use heapless::String;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let gateway = GatewayH2::GROVE_UART;

    let matter = MatterServerConfig::new(0xFFF1, 0x8001, 3840, 20_202_021, "CoreS3 Gateway H2");
    let thread = ThreadDatasetConfig::new(
        "core-s3-thread",
        0x1234,
        [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
        15,
        [0xAA; 16],
    );
    let setup = MatterOverThreadConfig::new(gateway, matter, thread);

    esp_println::println!(
        "Gateway H2 UART tx=GPIO{} rx=GPIO{} matter-port={}",
        setup.gateway.host_uart.tx.0,
        setup.gateway.host_uart.rx.0,
        setup.matter.port
    );

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
            "GATEWAY H2",
            "Matter-over-Thread config scaffold",
            Rgb565::GREEN,
        )
        .expect("draw validation screen");

    Rectangle::new(Point::new(18, 78), Size::new(284, 122))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(&mut parts.display)
        .expect("clear config area");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);

    Text::new("Gateway H2 host link", Point::new(30, 94), accent)
        .draw(&mut parts.display)
        .expect("draw heading");

    let mut uart: String<48> = String::new();
    write!(
        &mut uart,
        "UART: TX GPIO{} / RX GPIO{}",
        setup.gateway.host_uart.tx.0, setup.gateway.host_uart.rx.0
    )
    .unwrap();
    Text::new(&uart, Point::new(30, 112), style)
        .draw(&mut parts.display)
        .expect("draw uart");

    let mut matter_ids: String<64> = String::new();
    write!(
        &mut matter_ids,
        "Matter VID:PID {:04X}:{:04X}",
        setup.matter.vendor_id, setup.matter.product_id
    )
    .unwrap();
    Text::new(&matter_ids, Point::new(30, 130), style)
        .draw(&mut parts.display)
        .expect("draw matter ids");

    let mut pairing: String<64> = String::new();
    write!(
        &mut pairing,
        "Port {}  PIN {}",
        setup.matter.port, setup.matter.setup_passcode
    )
    .unwrap();
    Text::new(&pairing, Point::new(30, 148), style)
        .draw(&mut parts.display)
        .expect("draw pairing");

    let mut thread_line: String<64> = String::new();
    write!(
        &mut thread_line,
        "Thread '{}' ch {} pan {:04X}",
        setup.thread.network_name, setup.thread.channel, setup.thread.pan_id
    )
    .unwrap();
    Text::new(&thread_line, Point::new(30, 166), style)
        .draw(&mut parts.display)
        .expect("draw thread config");

    Text::new(
        "Feature-gated behind gateway-h2",
        Point::new(30, 188),
        accent,
    )
    .draw(&mut parts.display)
    .expect("draw feature gate");

    loop {
        core::hint::spin_loop();
    }
}
