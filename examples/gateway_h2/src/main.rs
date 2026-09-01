#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::{CoreS3DisplayResources, CoreS3GatewayH2Resources},
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
use esp_hal::time::Duration;
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
    .expect("initialize CoreS3 display");

    let mut gateway_parts = CoreS3::init_gateway_h2(CoreS3GatewayH2Resources {
        uart1: peripherals.UART1,
        tx: peripherals.GPIO1,
        rx: peripherals.GPIO2,
    })
    .expect("initialize Gateway H2 UART");

    let uart_probe = probe_gateway_h2_uart(&mut gateway_parts.uart);

    display_parts.display.clear(Rgb565::BLACK).expect("clear");
    Rectangle::new(Point::new(0, 0), Size::new(320, 24))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        .draw(&mut display_parts.display)
        .expect("header");
    let header = MonoTextStyle::new(&FONT_6X10, Rgb565::BLACK);
    Text::new("GATEWAY H2", Point::new(8, 16), header)
        .draw(&mut display_parts.display)
        .expect("header text");

    Rectangle::new(Point::new(18, 78), Size::new(284, 122))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(&mut display_parts.display)
        .expect("clear config area");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    let accent = MonoTextStyle::new(&FONT_6X10, Rgb565::GREEN);

    Text::new("Gateway H2 host link", Point::new(30, 94), accent)
        .draw(&mut display_parts.display)
        .expect("draw heading");

    let mut uart: String<64> = String::new();
    write!(
        &mut uart,
        "UART1 {} baud TX{} RX{}",
        gateway_parts.baud, setup.gateway.host_uart.tx.0, setup.gateway.host_uart.rx.0
    )
    .unwrap();
    Text::new(&uart, Point::new(30, 112), style)
        .draw(&mut display_parts.display)
        .expect("draw uart");

    let mut probe: String<64> = String::new();
    match uart_probe {
        UartProbe::Response { bytes } => {
            write!(&mut probe, "Probe: {} byte response", bytes).unwrap();
        }
        UartProbe::NoResponse => {
            probe.push_str("UART OK; no CLI/AT reply").unwrap();
        }
        UartProbe::WriteFailed => {
            probe.push_str("Probe: UART write failed").unwrap();
        }
        UartProbe::ReadFailed => {
            probe.push_str("Probe: UART read failed").unwrap();
        }
    }
    Text::new(&probe, Point::new(30, 130), accent)
        .draw(&mut display_parts.display)
        .expect("draw uart probe");

    let mut matter_ids: String<64> = String::new();
    write!(
        &mut matter_ids,
        "Matter {:04X}:{:04X} port {}",
        setup.matter.vendor_id, setup.matter.product_id, setup.matter.port
    )
    .unwrap();
    Text::new(&matter_ids, Point::new(30, 148), style)
        .draw(&mut display_parts.display)
        .expect("draw matter ids");

    let mut pairing: String<64> = String::new();
    write!(
        &mut pairing,
        "Disc {} PIN {}",
        setup.matter.discriminator, setup.matter.setup_passcode
    )
    .unwrap();
    Text::new(&pairing, Point::new(30, 166), style)
        .draw(&mut display_parts.display)
        .expect("draw pairing");

    let mut thread_line: String<64> = String::new();
    write!(
        &mut thread_line,
        "Thread '{}' ch {}",
        setup.thread.network_name, setup.thread.channel
    )
    .unwrap();
    Text::new(&thread_line, Point::new(30, 184), style)
        .draw(&mut display_parts.display)
        .expect("draw thread config");

    loop {
        core::hint::spin_loop();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UartProbe {
    Response { bytes: usize },
    NoResponse,
    WriteFailed,
    ReadFailed,
}

fn probe_gateway_h2_uart(uart: &mut core_s3::bsp::CoreS3GatewayH2Uart) -> UartProbe {
    let delay = esp_hal::delay::Delay::new();

    // This is intentionally a soft probe only. Gateway H2 firmware is commonly
    // OpenThread RCP/Spinel or a standalone Thread application, not AT-command
    // firmware. A response here proves an interactive CLI/AT-style firmware; no
    // response still leaves the UART transport initialized for a real driver.
    if uart.write(b"state\r\n").is_err() || uart.flush().is_err() {
        return UartProbe::WriteFailed;
    }

    delay.delay(Duration::from_millis(250));

    let mut buffer = [0u8; 64];
    match uart.read_buffered(&mut buffer) {
        Ok(0) => UartProbe::NoResponse,
        Ok(bytes) => UartProbe::Response { bytes },
        Err(_) => UartProbe::ReadFailed,
    }
}
