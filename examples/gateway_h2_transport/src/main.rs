#![no_std]
#![no_main]

use core_s3::{
    CoreS3,
    bsp::{CoreS3DisplayResources, CoreS3GatewayH2Resources},
    gateway_h2::transport::{FrameKind, H2Frame},
    ui::{Label, StatusBar, Theme},
};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use esp_backtrace as _;

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
    })
    .expect("gateway h2 uart");

    let frame = H2Frame::<32>::new(FrameKind::Request, 1, b"state");
    let _ = frame;
    let _ = h2;

    let display = &mut display_parts.display;
    let theme = Theme::DARK;
    display.clear(Rgb565::BLACK).expect("clear");
    StatusBar {
        bounds: Rectangle::new(Point::new(0, 0), Size::new(320, 24)),
        text: "core-s3 Gateway H2 transport",
    }
    .draw(display, theme)
    .expect("status");
    Rectangle::new(Point::new(12, 36), Size::new(296, 174))
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 2))
        .draw(display)
        .expect("border");
    Label {
        text: "H2 TRANSPORT",
        top_left: Point::new(24, 62),
        color: Rgb565::CYAN,
    }
    .draw(display)
    .expect("title");
    Label {
        text: "UART1 TX1 RX2 @ 115200",
        top_left: Point::new(24, 88),
        color: Rgb565::WHITE,
    }
    .draw(display)
    .expect("uart");
    Label {
        text: "Frame: Request id=1 payload=state",
        top_left: Point::new(24, 106),
        color: Rgb565::WHITE,
    }
    .draw(display)
    .expect("frame");
    Label {
        text: "No Matter/Thread stack in BSP",
        top_left: Point::new(24, 132),
        color: Rgb565::CYAN,
    }
    .draw(display)
    .expect("scope");

    loop {
        core::hint::spin_loop();
    }
}
