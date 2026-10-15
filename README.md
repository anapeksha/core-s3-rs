# core-s3

A modular `#![no_std]` board support crate for the **M5Stack CoreS3 K128** (ESP32-S3). The default/HAL-independent path remains allocator-free; opt-in `rtos` and radio support use a caller-registered bounded internal heap required by the upstream runtime.

It provides reusable board/peripheral bring-up while leaving networking, protocol runtimes, application UI, persistence, and product policy to downstream firmware.

## Supported hardware

- ILI9342C-compatible LCD and FT6336U touch
- AXP2101 PMIC and AW9523B I/O expander
- BMI270 IMU and BMM150 magnetometer helpers
- BM8563 RTC
- ES7210 microphone ADC and AW88298 speaker configuration
- TF-card access with safe LCD/SD shared-SPI coordination
- GC0308 camera capture behind `camera`
- Stacked Module Gateway H2 control and buffered UART transport behind `gateway-h2`

See [`docs/hardware.md`](docs/hardware.md) and the [`examples`](examples) directory for pin maps and complete bring-up flows.

## Features

| Feature             | Description                                                                                    |
| ------------------- | ---------------------------------------------------------------------------------------------- |
| `esp-hal`           | ESP-HAL-backed CoreS3 initialization on Xtensa ESP32-S3 targets                                |
| `rtos`              | One BSP-owned `esp-rtos` startup and primary Embassy executor; implies `esp-hal`               |
| `radio-coexistence` | `esp-radio` Wi-Fi + BLE controller ownership with upstream coexistence enabled; implies `rtos` |
| `camera`            | GC0308 SCCB setup and bounded LCD_CAM DMA capture; implies `esp-hal`                           |
| `gateway-h2`        | Stacked Gateway H2 metadata, EN/BOOT control, framing, and buffered async UART                 |
| `sdmmc`             | Conversion to `embedded_sdmmc::SdCard`                                                         |
| `defmt`             | Optional formatting implementations                                                            |

Default builds remain HAL-independent and `no_std`. The v0.5.6 implementation is staged and unreleased; use the local checkout for validation, not an assumed published v0.5.6 crate. Enable runtime/radio support explicitly in a downstream checkout:

```toml
[dependencies]
core-s3 = { path = "../core-s3-rs/crates/core-s3", features = ["rtos", "radio-coexistence"] }
```

`radio-coexistence` already enables `rtos`; listing both makes ownership intent explicit. There is no `rtos-task` feature alias: it was never published.

## Usage

```toml
[dependencies]
core-s3 = { path = "../core-s3-rs/crates/core-s3", features = ["esp-hal"] }
```

```rust
use core_s3::{CoreS3, bsp::CoreS3DisplayResources};

let mut display = CoreS3::init_display(CoreS3DisplayResources {
    i2c0: peripherals.I2C0,
    i2c_sda: peripherals.GPIO12,
    i2c_scl: peripherals.GPIO11,
    spi2: peripherals.SPI2,
    lcd_sclk: peripherals.GPIO36,
    lcd_mosi: peripherals.GPIO37,
    lcd_dc: peripherals.GPIO35,
    lcd_cs: peripherals.GPIO3,
    tf_card_cs: peripherals.GPIO4,
})?;
```

## LCD and TF-card sharing

The LCD and TF-card share SPI2, including GPIO35 as LCD D/C versus SD MISO. The BSP owns this handoff and guarantees safe chip-select and GPIO restoration on success and error paths.

For reliable use with a pre-inserted card:

1. Initialize shared SPI and SD parts.
2. Initialize CoreS3 power and power-cycle the TF-card rail.
3. Probe SD before any LCD traffic.
4. Initialize the LCD with `CoreS3::init_display_on_powered_shared_spi(...)`.

Use `CoreS3SharedDisplay::with_lcd_transaction(...)` for batched updates and `blit_rgb565_be(...)` for validated zero-copy panel-ready RGB565 data. See `examples/display_sd_coexist`.

## Camera

Enable `camera` for bounded GC0308 capture:

- QQVGA RGB565 (`160x120`)
- QQVGA grayscale/luminance (`160x120`)
- centered `DigitalZoom::{X1, X2, X4}` sensor crop

Capture uses caller-provided ESP-HAL DMA buffers. GPIO2 is camera XCLK and conflicts with Grove Port A pin 2. See `examples/camera_capture` for the live preview and touch zoom demo.

## RTOS and radio coexistence

The `rtos` feature provides one primary `esp-rtos` Embassy executor on CPU0. Initialize the HAL, register caller-static internal heap storage when allocations are needed, call `CoreS3Runtime::start(...)`, optionally call `start_worker(...)`, initialize peripherals/radios, then consume the runtime with `run(...)`:

```rust
static HEAP: InternalHeapStorage<98304> = InternalHeapStorage::new();
static WORKER_STACK: Cpu1WorkerStack<8192> = Cpu1WorkerStack::new();
static EXECUTOR: PrimaryExecutorStorage = PrimaryExecutorStorage::new();

let p = esp_hal::init(esp_hal::Config::default());
HEAP.initialize()?;
let runtime = CoreS3Runtime::start(
    RuntimeConfig::default(),
    TimerGroup::new(p.TIMG0).timer0,
    p.FROM_CPU_INTR0,
)?;
runtime.start_worker(p.CPU_CTRL, p.FROM_CPU_INTR1, &WORKER_STACK, worker)?; // optional
let radios = CoreS3::init_radio_coexistence(
    &runtime,
    CoreS3RadioPeripherals { wifi: p.WIFI, bt: p.BT },
    RadioCoexistenceConfig::default(),
)?;
runtime.run(&EXECUTOR, |spawner| {
    // Spawn application-owned networking/BLE tasks with the returned handles.
});
```

The CPU1 path is one optional synchronous worker with a permanently owned static bootstrap stack. It is not a second executor. The BSP deliberately provides no second executor or time driver and no per-task stack, priority, affinity, or stack high-water API.

`InternalHeapStorage<N>` privately owns aligned caller-static internal RAM, validates its address range, prevents reuse through the BSP, and exposes bounded `snapshot()` totals/current/free values. Peak and allocation counters are unavailable with this dependency contract. Its only unsafe operation is the audited allocator-region registration inside the wrapper; references and raw pointers are not exposed. Initialize it before scheduler startup or any other heap registration, and do not race direct `esp-alloc` registration.

`radio-coexistence` consumes the unique `WIFI` and `BT` peripheral tokens and returns independently movable station/AP interfaces, a `WifiController`, and a BLE connector. `WifiInterfaceMode::{Station, AccessPoint, AccessPointStation}` selects ownership; station is the default. The BSP initializes controllers and enables upstream coexistence but does not start/connect Wi-Fi, select credentials, create an IP stack, start a BLE host, or size host/socket/attribute/L2CAP pools. Those policies and all secrets remain application-owned. Do not log credentials, endpoints, advertisements, received payloads, or other secrets; review upstream log settings too.

`examples/dual_core` demonstrates the CPU1 worker plus the single primary executor. `examples/radio_coexistence` exercises Wi-Fi, an optional TCP echo endpoint, and BLE with build-time `option_env!` configuration. `examples/radio_h2_coexistence` composes those controllers with H2 UART/Spinel and SD-before-LCD board I/O. Their variables are `RADIO_WIFI_SSID`, `RADIO_WIFI_PASSWORD` (an explicit empty value selects an open AP), optional `RADIO_ECHO_IPV4` plus `RADIO_ECHO_PORT`, and, for the H2 composition harness, `RADIO_BLE_SCAN=1`. Values are never printed, but they remain embedded in firmware artifacts, which must be treated as sensitive.

The combined harness runs only three radio iterations and thirty LCD updates/sector-zero reads. SD writes/readback/restore are explicitly **NOT RUN**. BLE scanning is not GATT/L2CAP peer exchange; Spinel discovery is not Thread traffic. Blocking LCD/SD calls can delay the UART pump and async deadlines; tasks stop after their observation windows. The 1,000 sequential/200 parallel requests and two-hour soak require additional application-owned test instrumentation. See the validation record; no aggregate hardware PASS is implied.

## Gateway H2

The official stacked Module Gateway H2 route is:

```text
CoreS3 TX GPIO17 -> H2 RXD
CoreS3 RX GPIO10 <- H2 TXD
CoreS3 GPIO7     -> H2 EN (active-low reset)
CoreS3 GPIO18    -> H2 GPIO9/BOOT
```

`CoreS3::init_gateway_h2_openthread(...)` returns:

- a statically buffered `embedded_io_async 0.7::Read + Write` transport;
- an independently spawned UART pump that continuously drains RX;
- safe reset and ROM-bootloader control.

The BSP does not depend on OpenThread. `examples/gateway_h2_openthread` uses `openthread 0.4.0`'s `UartSpinelTransport` and runs the bounded `SpinelRadio::init()` handshake to query protocol and RCP radio capabilities.

### Gateway H2 DIP switches

For UART-only RCP operation on Module Gateway H2 v0.4, set **all eight S1 switches OFF**. S1-4 is unused. UART, EN, and BOOT are fixed M-Bus routes. `GatewayH2OpenThreadConfig::default()` selects 460800 baud for the hardware-validated ESP-IDF v5.5.2 `examples/openthread/ot_rcp` image.

H2 SPI is unsupported. M5Stack's product PinMap documents optional `SPI_MOSI`, `SPI_MISO`, `SPI_CLK`, and `SPI_CS` M-Bus routes, but does not document the RCP interrupt/handshake route required by OpenThread SPI framing or identify a compatible H2 firmware/protocol. Safe support also requires integration with the BSP-owned LCD/TF-card bus on GPIO35/36/37; the optional CS route conflicts with audio output on GPIO13. PTA is opt-in metadata only; GPIO0 `WL_ACTIVE` conflicts with audio I2S MCLK. Software cannot detect physical DIP positions.

## Building

The default HAL-independent host API retains the workspace MSRV of Rust 1.88 and is tested with stable 1.98. The staged ESP runtime/radio dependency family requires Rust 1.95 or newer and is tested with the `esp` 1.97 nightly toolchain.

| Dependency                 | Compatible version |
| -------------------------- | ------------------ |
| `esp-hal`                  | `1.2.2`            |
| `esp-rtos`                 | `0.4.0`            |
| `esp-radio`                | `1.0.0-beta.1`     |
| `esp-radio-rtos-driver`    | `0.4.2`            |
| `embassy-executor`         | `0.10`             |
| `embassy-time`             | `0.5.1`            |
| `embedded-io-async`        | `0.7`              |
| `static_cell`              | `2.1.1`            |
| `esp-alloc`                | `0.11.0`           |
| `esp-rom-sys`              | `0.1.5`            |
| `embassy-time-queue-utils` | `0.3.2`            |

`esp-radio 1.0.0-beta.1` is the latest published version for this staged implementation. The requested `https://docs.rs/esp-radio/1.0.0-beta.2/esp_radio/` URL is currently unpublished/404; beta.2 must not be claimed as available, used, or tested.

Treat this as one compatibility set; do not independently upgrade its ESP/Embassy members. Install the ESP Rust toolchain with [`espup`](https://github.com/esp-rs/espup), then run:

```sh
cargo +stable test -p core-s3 --all-features
cargo +esp check --workspace --all-features --release --target xtensa-esp32s3-none-elf
cargo +esp run -p hello_world --release --target xtensa-esp32s3-none-elf
```

## Scope

Consumer applications own Wi-Fi/IP, Matter/OpenThread/Zigbee runtimes, datasets, commissioning, credentials, persistence, high-throughput audio policy, and camera image processing. Default library paths do not allocate. Opt-in RTOS/radio dependencies may allocate only after explicit caller-static internal heap registration; the BSP does not persist frames or start hidden background tasks.

## Hardware status

Shared LCD/TF-card coexistence and the GC0308 camera preview have been validated on real CoreS3 hardware. The Gateway H2 v0.4 UART baseline (TX GPIO17, RX GPIO10, EN GPIO7, BOOT GPIO18, 460800 baud, all S1 switches OFF) has passed S3-hosted flashing, verification, UART exchange, Spinel startup, and RCP capability discovery. Dataset attachment and Matter-over-Thread remain application-level validation work. Gateway H2 SPI remains unsupported pending authoritative routes, SPI-capable H2 firmware/protocol, BSP integration, and real hardware validation.

The staged v0.5.6 `rtos` and `radio-coexistence` work has **not** completed hardware validation. No dual-core, Wi-Fi/BLE, H2 composition, reset/cold-boot, load, or soak claim is made for it. All v0.5.6 hardware gates are pending. Do not publish, merge, or tag v0.5.6 until [`docs/v0.5.6-hardware-validation.md`](docs/v0.5.6-hardware-validation.md) is completed with passing evidence.

## License

Licensed under the repository's license terms.
