#![no_std]
#![no_main]

use core_s3::{
    CoreS3,
    bsp::{
        CoreS3GatewayH2AsyncUart, CoreS3GatewayH2BufferedUartResources, CoreS3GatewayH2Resources,
        CoreS3GatewayH2UartPump,
    },
    gateway_h2::transport::{
        GATEWAY_H2_MIN_RX_BUFFER_SIZE, GATEWAY_H2_MIN_TX_BUFFER_SIZE, GatewayH2OpenThreadConfig,
    },
};
use embassy_executor::{Executor, Spawner};
use esp_backtrace as _;
use openthread::spinel::{UartSpinelTransport, UartTransportResources};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static BSP_UART_BUFFERS: StaticCell<
    CoreS3GatewayH2BufferedUartResources<
        GATEWAY_H2_MIN_RX_BUFFER_SIZE,
        GATEWAY_H2_MIN_TX_BUFFER_SIZE,
    >,
> = StaticCell::new();
static OPENTHREAD_UART_RESOURCES: StaticCell<UartTransportResources> = StaticCell::new();

#[embassy_executor::task]
async fn run_uart_pump(
    pump: CoreS3GatewayH2UartPump<
        'static,
        GATEWAY_H2_MIN_RX_BUFFER_SIZE,
        GATEWAY_H2_MIN_TX_BUFFER_SIZE,
    >,
) {
    pump.run().await;
}

#[embassy_executor::task]
async fn hold_spinel_transport(
    transport: CoreS3GatewayH2AsyncUart<
        'static,
        GATEWAY_H2_MIN_RX_BUFFER_SIZE,
        GATEWAY_H2_MIN_TX_BUFFER_SIZE,
    >,
) {
    let _spinel_transport = UartSpinelTransport::new(
        transport,
        OPENTHREAD_UART_RESOURCES.init(UartTransportResources::new()),
    );
    core::future::pending::<()>().await;
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let parts = CoreS3::init_gateway_h2_openthread(
        CoreS3GatewayH2Resources {
            uart1: peripherals.UART1,
            tx: peripherals.GPIO1,
            rx: peripherals.GPIO2,
        },
        BSP_UART_BUFFERS.init(CoreS3GatewayH2BufferedUartResources::new()),
        GatewayH2OpenThreadConfig::default(),
    )
    .expect("Gateway H2 buffered UART");

    esp_println::println!(
        "Gateway H2 OpenThread UART transport ready: baud={} max_frame={}",
        parts.baud,
        parts.max_frame_size
    );

    EXECUTOR.init(Executor::new()).run(move |spawner: Spawner| {
        spawner.spawn(run_uart_pump(parts.pump).expect("spawn Gateway H2 UART pump"));
        spawner.spawn(
            hold_spinel_transport(parts.transport).expect("spawn OpenThread Spinel transport"),
        );
    })
}
