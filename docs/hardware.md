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

ESP32-S3 is a dual-core Xtensa LX7 MCU. CoreS3 firmware starts on the PRO CPU; the APP CPU is available through `esp_hal::system::CpuControl`. Use `examples/dual_core` as the BSP reference for starting the second core with an explicit stack.

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

## Gateway H2 OpenThread transport

The `gateway-h2` feature exposes protocol-neutral metadata/codecs and a statically buffered async UART byte stream for stock ESP32-H2 OpenThread RCP firmware. UART1 uses TX GPIO1 and RX GPIO2 at 115200 baud, 8-N-1, without hardware flow control. GPIO2 conflicts with camera XCLK, so both hardware paths cannot be initialized simultaneously.

`CoreS3::init_gateway_h2_openthread(...)` returns a transport implementing `embedded_io_async 0.7::Read + Write` and a pump future. The pump exclusively owns the HAL UART and must run independently to continuously drain RX into caller-owned static storage. The BSP does not include `openthread`, `rs-matter`, datasets, commissioning, networking, or application orchestration.

## Validation checklist

- Confirm LCD controller init sequence and color order on hardware.
- Confirm backlight/reset control path through AXP2101/AW9523B.
- Probe I²C addresses with a scanner example before enabling high-level drivers.
- Confirm Gateway H2 Grove UART wiring and optional reset/boot pins for the exact stack/base revision.
- Validate `examples/sd_block_probe` on real CoreS3 hardware with an inserted valid TF card when changing SD acquisition behavior, including flash/cold-boot/reset with the card already inserted.
- Validate `examples/display_sd_coexist` when changing shared SPI/GPIO35 behavior after LCD init; it should complete alternating LCD updates and raw SD read/write/readback while the display remains powered and initialized. The v0.5.1 batched-transfer path completed 250 cycles (`Writes: 000250`, `Reads: 000500`, `Errors: 0`) on real CoreS3 hardware with a pre-inserted card, repeated reset, cold boot, and remove/reinsert/reset.
- Validate `examples/camera_capture` on real CoreS3 hardware when changing camera support: boot with `camera`, probe GC0308 product ID, capture repeated QQVGA RGB565 and grayscale frames, stop/restart capture, verify errors return instead of panicking, and confirm display/touch/PMIC/TF-card/Gateway-H2 APIs still compile with camera support enabled.
