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

GPIO35 is therefore a physically shared pad: display writes drive it as LCD D/C, while SD reads need it as SPI MISO. The `core-s3` v0.4.2 BSP configures SPI2 with GPIO35 as MISO, leaves GPIO35 as a pulled-up input for SD acquisition, keeps TF-card CS asserted across `embedded-sdmmc` CMD0 response polling, and restores LCD D/C output only when the display writes. `examples/sd_card` demonstrates AW9523B card-detect only; `examples/sd_block_probe` performs the SD-before-LCD sequence and a real `embedded-sdmmc::SdCard::num_bytes()` probe.

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

## AXP2101 battery status

M5Unified's CoreS3 battery percentage path reads AXP2101 register `0xA4` directly. `core-s3` follows that behavior through `Axp2101::battery_level_percent()` and uses voltage-derived percentage only as a coarse fallback. AXP2101 register `0x01` bits 5:6 report charging/ discharging/standby state; register `0x00` bit `0x20` reports VBUS-good external power, and bit `0x08` reports battery presence. CoreS3 does not expose battery current through the AXP2101 path used by this BSP, so current-based coulomb counting is not available via AXP2101 alone.

## Matter over Thread

The `gateway-h2` feature exposes `core_s3::gateway_h2::matter`, which combines Gateway H2 transport metadata with Matter commissioning and Thread dataset configuration. The module re-exports `rs-matter` for firmware crates that instantiate a real Matter server and bind it to the ESP32-H2/Thread transport.

## Validation checklist

- Confirm LCD controller init sequence and color order on hardware.
- Confirm backlight/reset control path through AXP2101/AW9523B.
- Probe I²C addresses with a scanner example before enabling high-level drivers.
- Confirm Gateway H2 Grove UART wiring and optional reset/boot pins for the exact stack/base revision.
- Validate `examples/sd_block_probe` on real CoreS3 hardware with an inserted valid TF card when changing shared SPI/GPIO35 behavior, including flash/cold-boot/reset with the card already inserted.
