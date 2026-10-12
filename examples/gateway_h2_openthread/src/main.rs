#![no_std]
#![no_main]

//! Module Gateway H2 v0.4 UART example.
//!
//! Set all eight S1 DIP switches OFF. S1-4 is unused. This example targets the
//! ESP-IDF v5.5.2 `examples/openthread/ot_rcp` UART image at 460800 baud.

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
use embassy_time_driver::Driver;
use esp_backtrace as _;
use openthread::{
    Radio,
    spinel::{SpinelRadio, SpinelRadioResources, UartSpinelTransport, UartTransportResources},
};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

struct SpinTimeDriver;

impl Driver for SpinTimeDriver {
    fn now(&self) -> u64 {
        esp_hal::time::Instant::now()
            .duration_since_epoch()
            .as_micros()
    }

    fn schedule_wake(&self, _at: u64, waker: &core::task::Waker) {
        // The platform-spin executor polls continuously. Waking immediately
        // keeps Embassy timers progressing without claiming a hardware alarm.
        waker.wake_by_ref();
    }
}

embassy_time_driver::time_driver_impl!(static TIME_DRIVER: SpinTimeDriver = SpinTimeDriver);

static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static BSP_UART_BUFFERS: StaticCell<
    CoreS3GatewayH2BufferedUartResources<
        GATEWAY_H2_MIN_RX_BUFFER_SIZE,
        GATEWAY_H2_MIN_TX_BUFFER_SIZE,
    >,
> = StaticCell::new();
static OPENTHREAD_UART_RESOURCES: StaticCell<UartTransportResources> = StaticCell::new();
static OPENTHREAD_RADIO_RESOURCES: StaticCell<SpinelRadioResources> = StaticCell::new();

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
async fn discover_rcp_capabilities(
    transport: CoreS3GatewayH2AsyncUart<
        'static,
        GATEWAY_H2_MIN_RX_BUFFER_SIZE,
        GATEWAY_H2_MIN_TX_BUFFER_SIZE,
    >,
) {
    let spinel_transport = UartSpinelTransport::new(
        transport,
        OPENTHREAD_UART_RESOURCES.init(UartTransportResources::new()),
    );
    let mut radio = SpinelRadio::new(
        spinel_transport,
        OPENTHREAD_RADIO_RESOURCES.init(SpinelRadioResources::new()),
    );

    match radio.init().await {
        Ok(capabilities) => {
            esp_println::println!("Gateway H2 Spinel startup PASS: {capabilities:?}");
        }
        Err(error) => {
            esp_println::println!("Gateway H2 Spinel startup FAIL: {error:?}");
        }
    }
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let parts = CoreS3::init_gateway_h2_openthread(
        CoreS3GatewayH2Resources {
            uart1: peripherals.UART1,
            tx: peripherals.GPIO17,
            rx: peripherals.GPIO10,
            enable: peripherals.GPIO7,
            boot: peripherals.GPIO18,
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
            discover_rcp_capabilities(parts.transport)
                .expect("spawn bounded OpenThread RCP discovery"),
        );
    })
}
