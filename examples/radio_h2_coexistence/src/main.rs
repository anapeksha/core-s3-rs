#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU32, Ordering};
use core_s3::bsp::*;
use core_s3::gateway_h2::transport::{
    GATEWAY_H2_MIN_RX_BUFFER_SIZE, GATEWAY_H2_MIN_TX_BUFFER_SIZE, GatewayH2OpenThreadConfig,
};
use core_s3::{
    CoreS3,
    radio::{CoreS3RadioParts, CoreS3RadioPeripherals, RadioCoexistenceConfig},
    rtos::{CoreS3Runtime, InternalHeapStorage, PrimaryExecutorStorage, RuntimeConfig},
};
use embassy_futures::{
    join::join,
    select::{Either, select},
};
use embassy_net::{StackResources, tcp::TcpSocket};
use embassy_time::{Duration, Timer, with_timeout};
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
};
use embedded_sdmmc::{Block, BlockDevice, BlockIdx};
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::wifi::{AuthenticationMethodConfig, Config, sta::StationConfig};
use openthread::{
    Radio,
    spinel::{SpinelRadio, SpinelRadioResources, UartSpinelTransport, UartTransportResources},
};
use static_cell::StaticCell;
use trouble_host::prelude::*;

// Build-time only; never print credentials, endpoint addresses, or received data.
const SSID: Option<&str> = option_env!("RADIO_WIFI_SSID");
const PASSWORD: Option<&str> = option_env!("RADIO_WIFI_PASSWORD");
// Optional IPv4 TCP echo service: must echo the exact four-byte probe.
const ECHO_IP: Option<&str> = option_env!("RADIO_ECHO_IPV4");
const ECHO_PORT: Option<&str> = option_env!("RADIO_ECHO_PORT");
const BLE_SCAN: Option<&str> = option_env!("RADIO_BLE_SCAN");
const ITERATIONS: u32 = 3;
static HEAP: InternalHeapStorage<98304> = InternalHeapStorage::new();
static EXECUTOR: PrimaryExecutorStorage = PrimaryExecutorStorage::new();
static NET: static_cell::StaticCell<StackResources<3>> = static_cell::StaticCell::new();
static REPORTS: AtomicU32 = AtomicU32::new(0);

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
    if with_timeout(Duration::from_secs(95), pump.run())
        .await
        .is_ok()
    {
        println!("H2 pump FAIL: exited early");
    } else {
        println!("H2 pump INCONCLUSIVE: service window ended; not a link health assertion");
    }
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

    match with_timeout(Duration::from_secs(15), radio.init()).await {
        Ok(Ok(_)) => println!("SPINEL capability discovery PASS"),
        Ok(Err(_)) => println!("SPINEL capability discovery FAIL"),
        Err(_) => println!("SPINEL capability discovery FAIL: deadline"),
    }
    println!("THREAD network/traffic NOT RUN: capability discovery only");
}

esp_bootloader_esp_idf::esp_app_desc!();

fn snapshot(stage: &str) {
    if let Ok(s) = HEAP.snapshot() {
        println!(
            "HEAP {} total={} current={} free={} peak=UNAVAILABLE",
            stage, s.total, s.current, s.free
        );
    }
}

#[esp_hal::main]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default());
    HEAP.initialize().expect("internal heap initialization");
    snapshot("initialized");
    let runtime = CoreS3Runtime::start(
        RuntimeConfig::default(),
        TimerGroup::new(p.TIMG0).timer0,
        p.FROM_CPU_INTR0,
    )
    .expect("runtime startup");
    println!("H2 requires UART RCP firmware at 460800 baud; all S1 switches OFF");
    println!(
        "SD write/readback/restore NOT RUN: non-destructive harness; blocking I/O has no enforceable async deadline"
    );
    println!(
        "LCD/SD INCONCLUSIVE: blocking driver calls; finite count only, no hard wall-clock bound"
    );
    let shared = SHARED.init(
        CoreS3::init_shared_spi(CoreS3SharedSpiResources {
            spi2: p.SPI2,
            sclk: p.GPIO36,
            mosi: p.GPIO37,
            miso: p.GPIO35,
        })
        .expect("shared SPI"),
    );
    let mut i2c = CoreS3::init_internal_i2c(CoreS3InternalI2cResources {
        i2c0: p.I2C0,
        i2c_sda: p.GPIO12,
        i2c_scl: p.GPIO11,
    })
    .expect("internal I2C");
    CoreS3::init_core_s3_power(&mut i2c).expect("board power");
    CoreS3::power_cycle_tf_card_rail(&mut i2c).expect("TF rail cycle");
    let sd = CoreS3::init_sd_on_shared_spi(CoreS3SdOnSharedSpiResources {
        shared_spi: shared,
        tf_card_cs: p.GPIO4,
    })
    .ok()
    .and_then(|mut parts| {
        if parts.spi_device.prepare_for_card_acquire().is_err() {
            println!("SD acquire FAIL; reads NOT RUN");
            return None;
        }
        let card = parts.into_sdmmc();
        match card.num_bytes() {
            Ok(_) => {
                println!("SD probe PASS (before LCD)");
                Some(card)
            }
            Err(_) => {
                println!("SD probe FAIL; reads NOT RUN");
                None
            }
        }
    });
    if sd.is_none() {
        println!("SD FAIL: unavailable; periodic reads NOT RUN");
    }
    let display =
        CoreS3::init_display_on_powered_shared_spi(CoreS3DisplayOnPoweredSharedSpiResources {
            shared_spi: shared,
            internal_i2c: i2c,
            lcd_cs: p.GPIO3,
        })
        .map(|p| p.display)
        .ok();
    println!(
        "LCD initialization {}",
        if display.is_some() {
            "PASS"
        } else {
            "FAIL; updates NOT RUN"
        }
    );
    let h2 = CoreS3::init_gateway_h2_openthread(
        CoreS3GatewayH2Resources {
            uart1: p.UART1,
            tx: p.GPIO17,
            rx: p.GPIO10,
            enable: p.GPIO7,
            boot: p.GPIO18,
        },
        BSP_UART_BUFFERS.init(CoreS3GatewayH2BufferedUartResources::new()),
        GatewayH2OpenThreadConfig::default(),
    )
    .ok();
    println!(
        "H2 UART initialization {}",
        if h2.is_some() {
            "PASS"
        } else {
            "FAIL; pump/Spinel NOT RUN"
        }
    );
    let parts = if SSID.is_some_and(|s| !s.is_empty()) && PASSWORD.is_some() {
        CoreS3::init_radio_coexistence(
            &runtime,
            CoreS3RadioPeripherals {
                wifi: p.WIFI,
                bt: p.BT,
            },
            RadioCoexistenceConfig::default(),
        )
        .map_err(|_| println!("RESULT FAIL: radio initialization; test NOT RUN"))
        .ok()
    } else {
        println!(
            "RESULT CONFIGURATION MISSING: RADIO_WIFI_SSID and RADIO_WIFI_PASSWORD required; values withheld; WiFi/DHCP/TCP/BLE NOT RUN (shared radio test requires WiFi configuration)"
        );
        None
    };
    match runtime.run(&EXECUTOR, move |spawner| {
        if let Some(h2) = h2 {
            spawner.spawn(run_uart_pump(h2.pump).expect("pump task"));
            spawner.spawn(discover_rcp_capabilities(h2.transport).expect("Spinel task"));
        }
        spawner.spawn(board_io(display, sd).expect("board task"));
        spawner.spawn(test(parts).expect("test task"));
    }) {
        Ok(never) => match never {},
        Err(_) => panic!("primary executor startup failed"),
    }
}

struct Reports;
impl EventHandler for Reports {
    fn on_adv_reports(&self, reports: LeAdvReportsIter<'_>) {
        for report in reports {
            if report.is_ok() {
                let _ = REPORTS.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                    Some(n.saturating_add(1))
                });
            }
        }
    }
}

#[embassy_executor::task]
async fn test(parts: Option<CoreS3RadioParts>) {
    let Some(parts) = parts else {
        snapshot("not run");
        return;
    };
    let Some(ssid) = SSID.filter(|s| !s.is_empty()) else {
        println!("RESULT CONFIGURATION MISSING: RADIO_WIFI_SSID; radios NOT RUN");
        return;
    };
    let Some(password) = PASSWORD else {
        println!(
            "RESULT CONFIGURATION MISSING: RADIO_WIFI_PASSWORD (empty permits open AP); radios NOT RUN"
        );
        return;
    };
    let (Ok(ssid), Ok(password)) = (ssid.try_into(), password.try_into()) else {
        println!("RESULT CONFIGURATION INVALID: credential length; values withheld");
        return;
    };
    let mut wifi = parts.wifi_controller;
    let auth = if PASSWORD == Some("") {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(password)
    };
    // esp-radio beta.1 starts Wi-Fi in set_config/new; no async start API exists.
    if wifi
        .set_config(&Config::Station(
            StationConfig::default()
                .with_ssid(ssid)
                .with_authentication(auth),
        ))
        .is_err()
    {
        println!("RESULT FAIL: Wi-Fi configuration/start");
        return;
    }
    snapshot("radio configured");
    let (net, mut net_runner) = embassy_net::new(
        parts.station.expect("station interface"),
        embassy_net::Config::dhcpv4(Default::default()),
        NET.init(StackResources::new()),
        0x7353_2026,
    );
    let controller = ExternalController::<_, 1>::new(parts.ble_connector);
    let mut resources = HostResources::<_, DefaultPacketPool, 1, 1>::new();
    let stack = trouble_host::new(controller, &mut resources).build();
    let mut runner = stack.runner();
    let mut scanner = Scanner::new(stack.central());
    let work = async {
        for iteration in 0..ITERATIONS {
            let before = REPORTS.load(Ordering::Relaxed);
            let ble = async {
                if BLE_SCAN != Some("1") {
                    println!("BLE NOT RUN: set RADIO_BLE_SCAN=1 to enable");
                    return false;
                }
                let config = ScanConfig {
                    active: true,
                    interval: Duration::from_millis(100),
                    window: Duration::from_millis(50),
                    ..Default::default()
                };
                let session =
                    match with_timeout(Duration::from_secs(5), scanner.scan(&config)).await {
                        Ok(Ok(session)) => session,
                        _ => {
                            println!("BLE FAIL: scan startup/timeout");
                            return false;
                        }
                    };
                Timer::after_secs(20).await;
                drop(session);
                // Keep the runner polled to process the cancellation/disable command.
                Timer::after_millis(500).await;
                let count = REPORTS.load(Ordering::Relaxed).saturating_sub(before);
                println!("BLE scan window complete reports={}", count);
                if count == 0 {
                    println!("BLE INCONCLUSIVE: no advertisements observed");
                }
                if count > 0 {
                    println!("BLE observation PASS");
                }
                count > 0
            };
            let network = async {
                if !matches!(
                    with_timeout(Duration::from_secs(12), wifi.connect_async()).await,
                    Ok(Ok(_))
                ) {
                    println!("WIFI FAIL: association/timeout; DHCP and health NOT RUN");
                    return false;
                }
                println!("WIFI association PASS");
                if with_timeout(Duration::from_secs(6), net.wait_config_up())
                    .await
                    .is_err()
                {
                    println!("DHCP FAIL: timeout; health NOT RUN");
                    return false;
                }
                println!("DHCP PASS");
                health(net).await
            };
            let (ble_ok, network_ok) = join(ble, network).await;
            println!(
                "ITERATION {} BLE={} NETWORK={}",
                iteration + 1,
                ble_ok,
                network_ok
            );
            snapshot("iteration");
            if !matches!(
                with_timeout(Duration::from_secs(2), wifi.disconnect_async()).await,
                Ok(Ok(_))
            ) {
                println!("RESULT FAIL: disconnect/timeout; remaining iterations NOT RUN");
                return;
            }
        }
        println!("RESULT COMPLETE: bounded observations only; see per-layer results");
    };
    let drivers = select(net_runner.run(), runner.run_with_handler(&Reports));
    match select(drivers, with_timeout(Duration::from_secs(90), work)).await {
        Either::First(_) => println!("RESULT FAIL: driver exited"),
        Either::Second(Err(_)) => println!("RESULT FAIL: overall deadline"),
        Either::Second(Ok(())) => {}
    }
    snapshot("finished");
}

async fn health(net: embassy_net::Stack<'_>) -> bool {
    let endpoint = ECHO_IP
        .and_then(|s| s.parse::<embassy_net::Ipv4Address>().ok())
        .zip(
            ECHO_PORT
                .and_then(|s| s.parse::<u16>().ok())
                .filter(|p| *p != 0),
        );
    let Some((ip, port)) = endpoint else {
        println!(
            "HEALTH NOT RUN: CONFIGURATION MISSING/INVALID RADIO_ECHO_IPV4 or RADIO_ECHO_PORT; requires TCP echo service"
        );
        return false;
    };
    let mut rx = [0; 128];
    let mut tx = [0; 128];
    let mut socket = TcpSocket::new(net, &mut rx, &mut tx);
    let exchange = async {
        socket.connect((ip, port)).await.map_err(|_| ())?;
        let probe = *b"CS3!";
        let mut sent = 0;
        for _ in 0..4 {
            if sent == probe.len() {
                break;
            }
            let n = socket.write(&probe[sent..]).await.map_err(|_| ())?;
            if n == 0 {
                return Err(());
            }
            sent += n;
        }
        let mut reply = [0; 4];
        let mut received = 0;
        for _ in 0..4 {
            if received == reply.len() {
                break;
            }
            let n = socket.read(&mut reply[received..]).await.map_err(|_| ())?;
            if n == 0 {
                return Err(());
            }
            received += n;
        }
        if sent == 4 && received == 4 && reply == probe {
            Ok(())
        } else {
            Err(())
        }
    };
    let ok = matches!(
        with_timeout(Duration::from_secs(3), exchange).await,
        Ok(Ok(()))
    );
    socket.abort();
    println!(
        "HEALTH TCP echo {}",
        if ok { "PASS" } else { "FAIL/timeout" }
    );
    ok
}

static SHARED: StaticCell<CoreS3SharedSpiParts> = StaticCell::new();
type Card = embedded_sdmmc::SdCard<CoreS3SharedSdSpiDevice, esp_hal::delay::Delay>;

#[embassy_executor::task]
async fn board_io(mut display: Option<CoreS3SharedDisplay>, sd: Option<Card>) {
    for iteration in 0..30 {
        if let Some(display) = display.as_mut() {
            let color = if iteration % 2 == 0 {
                Rgb565::CYAN
            } else {
                Rgb565::BLACK
            };
            let result = Rectangle::new(Point::new(20, 40), Size::new(32, 32))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(display);
            println!(
                "LCD update {}",
                if result.is_ok() {
                    "PASS: transfer only, visual verification required"
                } else {
                    "FAIL"
                }
            );
        }
        if let Some(sd) = sd.as_ref() {
            let mut block = Block::new();
            println!(
                "SD raw read {}",
                if sd
                    .read(core::slice::from_mut(&mut block), BlockIdx(0), "probe")
                    .is_ok()
                {
                    "PASS"
                } else {
                    "FAIL"
                }
            );
        }
        Timer::after_secs(2).await;
    }
    println!("LCD/SD finite observation window COMPLETE");
}
