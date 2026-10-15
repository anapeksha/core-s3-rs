# M5Stack CoreS3 hardware notes

Source documents supplied during BSP creation:

- M5Stack CoreS3 v1.0 schematic
- DIN Base v1.1 schematic
- ESP32-S3 technical reference manual
- LTR-553ALS-WA ambient/proximity sensor datasheet
- GC0308 camera datasheet
- ES7210 microphone ADC datasheet
- BMM150 magnetometer datasheet
- BMI270 IMU datasheet
- BM8563 RTC datasheet
- AXP2101 PMU datasheet
- AW88298 amplifier datasheet
- AW9523B GPIO expander datasheet

## CPU cores

ESP32-S3 is a dual-core Xtensa LX7 MCU. With feature `rtos`, `CoreS3Runtime::start(...)` owns the single `esp-rtos` startup and `CoreS3Runtime::run(...)` owns the single primary Embassy executor/time driver on CPU0. `start_worker(...)` may start one synchronous CPU1 worker using a permanently owned caller-static `Cpu1WorkerStack`; it is not a second executor. The BSP intentionally exposes no per-task stacks, priorities, arbitrary affinity, stack high-water API, second executor, or second time driver. Use `examples/dual_core` as the reference sequence.

## Shared LCD / TF-card SPI wiring

M5Stack's official CoreS3 documentation is the source of truth for the shared LCD and microSD wiring:

| Function     | ESP32-S3 pins                                  |
| ------------ | ---------------------------------------------- |
| LCD ILI9342C | MOSI GPIO37, SCK GPIO36, CS GPIO3, D/C GPIO35  |
| TF card      | MISO GPIO35, MOSI GPIO37, SCK GPIO36, CS GPIO4 |

GPIO35 is therefore a physically shared pad: display writes drive it as LCD D/C, while SD reads need it as SPI MISO. M5GFX's CoreS3 panel switches GPIO35 on LCD CS boundaries: LCD CS active routes GPIO35 to ordinary GPIO output for D/C, and LCD CS inactive disables output so GPIO35 can serve SPI MISO. The BSP follows that model with CoreS3-specific LCD and SD `SpiDevice` wrappers: LCD transactions force TF-card CS high, route GPIO35 as LCD D/C only while LCD CS is active, and restore LCD CS high, TF-card CS high, GPIO35 SD/MISO input, and SD-safe SPI settings afterward. v0.5.1 adds scoped batched LCD sessions so address-window commands and bounded panel-ready RGB565 byte writes share one acquisition/restoration cycle. `examples/sd_card` demonstrates AW9523B card-detect only; `examples/sd_block_probe` performs the SD-before-LCD capacity probe; `examples/display_sd_coexist` alternates batched LCD updates with raw SD read/write/readback validation.

## Internal buses/devices

| Device        | Function          | Bus/address       |
| ------------- | ----------------- | ----------------- |
| AXP2101       | PMU/charger       | I²C `0x34`        |
| BM8563        | RTC               | I²C `0x51`        |
| BMI270        | 6-axis IMU        | I²C `0x68`        |
| BMM150        | Magnetometer      | I²C `0x10`        |
| LTR-553ALS-WA | Ambient/proximity | I²C `0x23`        |
| AW9523B       | GPIO expander     | I²C `0x58`        |
| GC0308        | Camera            | DVP + control bus |
| ES7210        | Microphone ADC    | I²S + control bus |
| AW88298       | Speaker amplifier | I²S + control bus |

## Camera wiring

M5Stack's CoreS3 UserDemo configures the built-in GC0308 DVP camera with this mapping:

| Function | ESP32-S3 pin  |
| -------- | ------------- |
| XCLK     | GPIO2         |
| SCCB SDA | GPIO12        |
| SCCB SCL | GPIO11        |
| PCLK     | GPIO45        |
| VSYNC    | GPIO46        |
| HREF     | GPIO38        |
| D0       | GPIO39        |
| D1       | GPIO40        |
| D2       | GPIO41        |
| D3       | GPIO42        |
| D4       | GPIO15        |
| D5       | GPIO16        |
| D6       | GPIO48        |
| D7       | GPIO47        |
| PWDN     | absent / `-1` |
| RESET    | absent / `-1` |

`core-s3` feature `camera` exposes `CoreS3::init_camera(...)` for this mapping and uses ESP-HAL `LCD_CAM` plus DMA channel 0. The SCCB control path is the same internal I²C pins used for PMIC/AW9523B/touch/sensors, so camera ownership consumes a configured `CoreS3I2c` while the camera is active. GPIO2 is also Grove Port A pin 2, so camera use conflicts with treating that pin as application-owned Grove GPIO/UART/I²C.

v0.5.0 supports QQVGA RGB565 and QQVGA grayscale/luminance configuration only. `DigitalZoom::{X1,X2,X4}` programs a centered GC0308 crop window; live previews can scale the cropped frame on the LCD. Broader base sizes and JPEG are intentionally rejected until validated on real CoreS3 hardware.

## AXP2101 battery status

M5Unified's CoreS3 battery percentage path reads AXP2101 register `0xA4` directly. `core-s3` follows that behavior through `Axp2101::battery_level_percent()` and uses voltage-derived percentage only as a coarse fallback. AXP2101 register `0x01` bits 5:6 report charging/ discharging/standby state; register `0x00` bit `0x20` reports VBUS-good external power, and bit `0x08` reports battery presence. CoreS3 does not expose battery current through the AXP2101 path used by this BSP, so current-based coulomb counting is not available via AXP2101 alone.

## RTOS and radio controllers

The staged v0.5.6 `rtos` path requires this order: initialize ESP-HAL; initialize caller-static `InternalHeapStorage` if upstream components allocate; call `CoreS3Runtime::start(...)`; optionally start the CPU1 worker; initialize board, H2, and radio resources with interrupts enabled; call `CoreS3Runtime::run(...)` last. `InternalHeapStorage` privately retains aligned internal-RAM backing, permits one registration, never returns a backing reference/pointer, and cannot be reused after a failed claim. The wrapper's sole unsafe boundary is the documented `esp-alloc` region registration. `snapshot()` reports total/current/free values; peak and allocation counters are unavailable in this dependency configuration.

Feature `radio-coexistence` consumes unique ESP-HAL `WIFI` and `BT` tokens. `WifiInterfaceMode` requests station, access point, or both; `CoreS3RadioParts` returns only those interfaces plus the Wi-Fi controller and BLE connector. Upstream coexistence is enabled, but the BSP does not start/connect/reconnect Wi-Fi, provide credentials, create an IP stack or sockets, start a BLE host/protocol runtime, or size host pools. Applications own those policies and must not log credentials, endpoint addresses, advertisements, received payloads, or other secrets.

The compatible dependency family is `esp-hal 1.2.2`, `esp-rtos 0.4.0`, `esp-radio 1.0.0-beta.1`, `esp-radio-rtos-driver 0.4.2`, `esp-alloc 0.11.0`, `esp-rom-sys 0.1.5`, `embassy-time-queue-utils 0.3.2`, `embassy-executor 0.10`, `embassy-time 0.5.1`, `embedded-io-async 0.7`, and `static_cell 2.1.1`. The default host MSRV remains workspace Rust 1.88 and is tested with stable 1.98. The ESP family requires Rust 1.95 or newer and is tested with the `esp` 1.97 nightly toolchain. There is no `rtos-task` feature alias because it was never published.

`examples/radio_coexistence` uses build-time `option_env!` values `RADIO_WIFI_SSID`, `RADIO_WIFI_PASSWORD`, `RADIO_ECHO_IPV4`, and `RADIO_ECHO_PORT`. `examples/radio_h2_coexistence` additionally accepts `RADIO_BLE_SCAN=1`; it initializes the internal heap/runtime, probes SD before LCD, starts the independent H2 UART pump/Spinel discovery, and exercises Wi-Fi/BLE work on the one primary executor. An empty password explicitly selects an open AP. Missing optional values skip their tests. Values are withheld from logs but embedded in the firmware binary, so artifacts are sensitive.

`esp-radio 1.0.0-beta.1` is the latest published staged baseline. The requested `https://docs.rs/esp-radio/1.0.0-beta.2/esp_radio/` URL is currently unpublished/404; beta.2 is not an available or tested dependency.

The combined harness runs three radio iterations and thirty LCD updates with non-destructive sector-zero reads; SD write/readback/restore is explicitly **NOT RUN**. Scanning does not validate real BLE GATT/L2CAP exchanges. Spinel discovery does not validate Thread traffic. Blocking LCD/SD operations can delay H2 pump servicing and async deadlines; finite iterations are not hard wall-clock bounds. Separate application-owned instrumentation and external peers are needed for load, recovery, and soak gates.

## Gateway H2 OpenThread transport

The `gateway-h2` feature exposes protocol-neutral metadata/codecs and a statically buffered async UART byte stream for stock ESP32-H2 OpenThread RCP firmware. The hardware-validated stacked Module Gateway H2 v0.4 route is UART1 TX GPIO17 -> H2 RXD and RX GPIO10 <- H2 TXD, 460800 baud, 8-N-1, without hardware flow control when using the ESP-IDF v5.5.2 `examples/openthread/ot_rcp` image. GPIO7 drives active-low EN/reset and GPIO18 drives H2 GPIO9/BOOT. GPIO9 is pulled high for normal execution and must be low before EN rises to request ROM download mode.

`CoreS3::init_gateway_h2_openthread(...)` returns safe control, a transport implementing `embedded_io_async 0.7::Read + Write`, and a pump future. The pump exclusively owns the HAL UART and must run independently to continuously drain RX into caller-owned static storage. The BSP does not include `openthread`, `rs-matter`, datasets, commissioning, networking, or application orchestration.

Module Gateway H2 v0.4 UART-only operation requires all eight S1 switches OFF; S1-4 is unused. Software records an expected routing but cannot detect physical switch positions. The H2 UART/EN/BOOT path composes with S3 Wi-Fi/BLE controller ownership, but remains a separately serviced framed transport; reserve GPIO17/GPIO10/GPIO7/GPIO18 for H2 control and transport. Product material describes optional SPI/PTA routing, but Gateway H2 SPI remains unsupported until authoritative route and switch documentation, a concrete SPI-capable H2 firmware/protocol, integration with the BSP-owned LCD/TF shared-SPI arbiter, and real bidirectional hardware validation are all available. PTA remains application-owned and opt-in; GPIO0 `WL_ACTIVE` conflicts with I2S MCLK.

## Validation checklist

The staged v0.5.6 RTOS/radio work has not been run or validated on hardware. Do not infer coexistence, dual-core stability, reset recovery, load, or soak results, and do not publish, merge, or tag v0.5.6 until [`v0.5.6-hardware-validation.md`](v0.5.6-hardware-validation.md) is completed with passing evidence.

- Confirm LCD controller init sequence and color order on hardware.
- Confirm backlight/reset control path through AXP2101/AW9523B.
- Probe I²C addresses with a scanner example before enabling high-level drivers.
- Confirm stacked Gateway H2 TX GPIO17/RX GPIO10 UART, GPIO7 EN, GPIO18 BOOT, all S1 switches OFF, exact H2 firmware, and 460800 baud on Module Gateway H2 v0.4.
- Validate `examples/sd_block_probe` on real CoreS3 hardware with an inserted valid TF card when changing SD acquisition behavior, including flash/cold-boot/reset with the card already inserted.
- Validate `examples/display_sd_coexist` when changing shared SPI/GPIO35 behavior after LCD init; it should complete alternating LCD updates and raw SD read/write/readback while the display remains powered and initialized. The v0.5.1 batched-transfer path completed 250 cycles (`Writes: 000250`, `Reads: 000500`, `Errors: 0`) on real CoreS3 hardware with a pre-inserted card, repeated reset, cold boot, and remove/reinsert/reset.
- Validate `examples/camera_capture` on real CoreS3 hardware when changing camera support: boot with `camera`, probe GC0308 product ID, capture repeated QQVGA RGB565 and grayscale frames, stop/restart capture, verify errors return instead of panicking, and confirm display/touch/PMIC/TF-card/Gateway-H2 APIs still compile with camera support enabled.
