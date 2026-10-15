#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU32, Ordering};
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
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use esp_radio::wifi::{AuthenticationMethodConfig, Config, sta::StationConfig};
use trouble_host::prelude::*;

// Build-time only; never print credentials, endpoint addresses, or received data.
const SSID: Option<&str> = option_env!("RADIO_WIFI_SSID");
const PASSWORD: Option<&str> = option_env!("RADIO_WIFI_PASSWORD");
// Optional IPv4 TCP echo service: must echo the exact four-byte probe.
const ECHO_IP: Option<&str> = option_env!("RADIO_ECHO_IPV4");
const ECHO_PORT: Option<&str> = option_env!("RADIO_ECHO_PORT");
const ITERATIONS: u32 = 3;
static HEAP: InternalHeapStorage<98304> = InternalHeapStorage::new();
static EXECUTOR: PrimaryExecutorStorage = PrimaryExecutorStorage::new();
static NET: static_cell::StaticCell<StackResources<3>> = static_cell::StaticCell::new();
static REPORTS: AtomicU32 = AtomicU32::new(0);

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
            "RESULT CONFIGURATION MISSING: RADIO_WIFI_SSID and RADIO_WIFI_PASSWORD required; values withheld; radios NOT RUN"
        );
        None
    };
    match runtime.run(&EXECUTOR, move |spawner| {
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
