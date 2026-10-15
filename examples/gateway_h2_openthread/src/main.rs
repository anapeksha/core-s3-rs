#![no_std]
#![no_main]

//! Module Gateway H2 v0.4 UART example.
//!
//! Set all eight S1 DIP switches OFF. S1-4 is unused. This example targets the
//! ESP-IDF v5.5.2 `examples/openthread/ot_rcp` UART image at 460800 baud.

use core_s3::rtos::{CoreS3Runtime, PrimaryExecutorStorage, RuntimeConfig};
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
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use openthread::{
    Radio,
    spinel::{SpinelRadio, SpinelRadioResources, UartSpinelTransport, UartTransportResources},
};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

static EXECUTOR: PrimaryExecutorStorage = PrimaryExecutorStorage::new();
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
    let timers = TimerGroup::new(peripherals.TIMG0);
    let runtime = CoreS3Runtime::start(
        RuntimeConfig::default(),
        timers.timer0,
        peripherals.FROM_CPU_INTR0,
    )
    .expect("start CoreS3 runtime");
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

    match runtime.run(&EXECUTOR, move |spawner| {
        spawner.spawn(run_uart_pump(parts.pump).expect("spawn Gateway H2 UART pump"));
        spawner.spawn(
            discover_rcp_capabilities(parts.transport)
                .expect("spawn bounded OpenThread RCP discovery"),
        );
    }) {
        Ok(never) => match never {},
        Err(error) => panic!("primary executor: {error:?}"),
    }
}
