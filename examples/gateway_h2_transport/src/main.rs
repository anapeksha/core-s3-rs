#![no_std]
#![no_main]

use core::fmt::Write;

use core_s3::{
    CoreS3,
    bsp::{CoreS3DisplayResources, CoreS3GatewayH2Resources},
    gateway_h2::transport::{FrameKind, H2Frame, TransportError},
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
use heapless::{String, Vec};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
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

    let h2 = CoreS3::init_gateway_h2(CoreS3GatewayH2Resources {
        uart1: peripherals.UART1,
        tx: peripherals.GPIO1,
        rx: peripherals.GPIO2,
    });

    let frame = H2Frame::<32>::new(FrameKind::Request, 1, b"state");
    let encoded = frame.encode::<64>();
    let decoded_ok = encoded
        .as_ref()
        .ok()
        .and_then(|bytes| H2Frame::<32>::decode(bytes).ok())
        .as_ref()
        == Some(&frame);
    let checksum_rejected = encoded
        .as_ref()
        .ok()
        .map(corrupt_last_byte)
        .and_then(|bytes| H2Frame::<32>::decode(&bytes).err())
        == Some(TransportError::BadChecksum);

    let display = &mut display_parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 Gateway H2 transport",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 186))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "H2 TRANSPORT",
        top_left: Point::new(24, 58),
        color: Rgb565::CYAN,
    }
    .draw(display)
    .expect("title");

    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    draw_line(
        display,
        80,
        if h2.is_ok() {
            "UART1 TX1 RX2 @ 115200: OK"
        } else {
            "UART1 TX1 RX2 @ 115200: FAILED"
        },
        if h2.is_ok() {
            Rgb565::GREEN
        } else {
            Rgb565::RED
        },
    );
    draw_line(
        display,
        98,
        "Frame: Request id=1 payload=state",
        Rgb565::WHITE,
    );

    let mut line: String<64> = String::new();
    match encoded.as_ref() {
        Ok(bytes) => {
            let checksum = bytes.last().copied().unwrap_or(0);
            let _ = write!(
                &mut line,
                "Encode: len={} checksum=0x{checksum:02X}",
                bytes.len()
            );
            Text::new(&line, Point::new(24, 116), style)
                .draw(display)
                .ok();
            draw_hex_prefix(display, 134, bytes);
        }
        Err(_) => draw_line(display, 116, "Encode: FAILED", Rgb565::RED),
    }

    draw_line(
        display,
        156,
        if decoded_ok {
            "Decode round-trip: OK"
        } else {
            "Decode round-trip: FAILED"
        },
        if decoded_ok {
            Rgb565::GREEN
        } else {
            Rgb565::RED
        },
    );
    draw_line(
        display,
        174,
        if checksum_rejected {
            "Bad checksum rejected: OK"
        } else {
            "Bad checksum rejected: FAILED"
        },
        if checksum_rejected {
            Rgb565::GREEN
        } else {
            Rgb565::RED
        },
    );
    draw_line(
        display,
        198,
        "No Matter/Thread/Zigbee stack in BSP",
        Rgb565::CYAN,
    );

    loop {
        core::hint::spin_loop();
    }
}

fn corrupt_last_byte<const N: usize>(bytes: &Vec<u8, N>) -> Vec<u8, N> {
    let mut corrupted = bytes.clone();
    if let Some(last) = corrupted.last_mut() {
        *last ^= 0x01;
    }
    corrupted
}

fn draw_line<D>(display: &mut D, y: i32, text: &str, color: Rgb565)
where
    D: DrawTarget<Color = Rgb565>,
{
    let style = MonoTextStyle::new(&FONT_6X10, color);
    Text::new(text, Point::new(24, y), style).draw(display).ok();
}

fn draw_hex_prefix<D, const N: usize>(display: &mut D, y: i32, bytes: &Vec<u8, N>)
where
    D: DrawTarget<Color = Rgb565>,
{
    let mut line: String<64> = String::new();
    let _ = line.push_str("Bytes:");
    for byte in bytes.iter().take(8) {
        let _ = write!(&mut line, " {byte:02X}");
    }
    if bytes.len() > 8 {
        let _ = line.push_str(" ...");
    }
    let style = MonoTextStyle::new(&FONT_6X10, Rgb565::WHITE);
    Text::new(&line, Point::new(24, y), style)
        .draw(display)
        .ok();
}
