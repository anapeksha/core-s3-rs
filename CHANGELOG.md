# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.5.2] - 2026-10-09

- Upgraded the ESP32-S3 dependency baseline to `esp-hal = "=1.2.2"` and migrated camera/I2S examples to its DMA APIs.
- Replaced the defective blocking OpenThread UART helper with a statically allocated buffered async UART stream implementing `embedded_io_async 0.7::Read + Write`.
- Added an independently polled full-duplex UART pump so Gateway H2 RX remains drained between application command exchanges.
- Added bounded Spinel buffer sizing constants, capacity validation, typed UART errors, physical-flush tracking, and byte/error/overflow counters.
- Preserved the lower-level blocking Gateway H2 UART API for diagnostics and custom protocols.
- Updated the no-heap Gateway H2 example and migration documentation; real Gateway H2 RCP and post-upgrade LCD/SD hardware validation remain pending.

## [0.5.1] - 2026-10-07

- Added scoped batched LCD transactions for the CoreS3 shared SPI bus, preserving LCD/TF-card CS and GPIO35 handoff invariants with one acquisition/restoration cycle per logical update.
- Added validated zero-copy big-endian RGB565 blits for fully in-bounds landscape regions, with bounded SPI writes and reusable transfer statistics.
- Fixed dirty-region overflow so pending updates collapse to a conservative bounding rectangle instead of losing changed pixels; added clipped invalidation helpers and retry-safe flush semantics.
- Expanded host tests for merge determinism, overflow coverage, clipping, failed-flush retry, byte-length validation, byte order, chunking, and logical-session counts.
- Updated `dirty_regions` and `display_sd_coexist` to exercise overflow-safe dirty tracking and batched zero-copy LCD updates; validated both on real CoreS3 hardware, including 250 LCD/SD cycles, repeated reset, cold boot, and card remove/reinsert/reset.

## [0.5.0] - 2026-09-08

- Added opt-in `camera` feature for CoreS3 GC0308 camera support on ESP-HAL.
- Added `core_s3::camera` with typed frame-size/pixel-format/config/error metadata, buffer-size helpers, QR-friendly QQVGA grayscale config, and GC0308 SCCB probe/configuration helpers.
- Added `CoreS3CameraResources`, `CoreS3Camera`, and `CoreS3::init_camera(...)` for the M5Stack CoreS3 camera pin map, internal-I2C SCCB ownership, ESP-HAL `LCD_CAM`, DMA channel 0, and bounded DMA capture lifecycle.
- Corrected `pins::CameraPins::GC0308` to match M5Stack's CoreS3 UserDemo mapping.
- Added Espressif GC0308 default register initialization plus QQVGA RGB565/grayscale configuration support; larger base sizes and JPEG remain rejected until hardware-validated.
- Added `DigitalZoom::{X1,X2,X4}` for centered GC0308 sensor-crop digital zoom plus `CoreS3Camera::set_zoom(...)`.
- Added `examples/camera_capture` as a live LCD preview demo with left-side preview, right-side touch zoom controls, sensor-crop zoom, and repeated DMA frame capture.

## [0.4.4] - 2026-09-07

- Added a CoreS3-specific shared LCD `SpiDevice` so LCD transactions force TF-card CS high, switch GPIO35 to LCD D/C output only while LCD CS is active, and restore the SD/MISO-safe idle state afterward.
- Centralized TF-card CS ownership in `CoreS3SharedSpiParts` so LCD and SD access share one BSP-controlled SPI2/GPIO35 coordinator.
- Restored GPIO35's ESP32-S3 GPIO matrix role between LCD D/C output and SPI2 MISO using minimal documented ROM matrix calls, matching M5GFX's CoreS3 `cs_control()` behavior.
- Kept SD transactions SD/MISO-safe across LCD traffic while preserving the v0.4.2 SD-before-LCD acquisition sequence and CMD0 CS framing.
- Added pure host tests for CoreS3 shared-SPI invariants and SD command-framing state.
- Added `examples/display_sd_coexist` for real hardware validation of alternating LCD updates and raw SD read/write/readback while the LCD remains initialized.

## [0.4.3] - 2026-09-04

- Added `Axp2101::battery_level_percent()` using AXP2101 register `0xA4`, matching M5Unified's CoreS3 battery-level path.
- Updated `Axp2101::status()` to prefer the AXP2101 gauge SOC when valid and fall back to voltage-derived percentage only as an estimate.
- Added non-breaking `BatteryStatus` fields: `percentage_estimated`, `state_of_charge`, and `battery_present`.
- Added dedicated `Axp2101::charge_state()`, `external_power()`, and `battery_present()` helpers backed by AXP2101 status registers `0x01` and `0x00`.
- Added pure AXP2101 decode helpers and host tests for SOC, charge state, external power, and battery presence decoding.

## [0.4.2] - 2026-09-04

- Fixed CoreS3 TF-card acquisition for cards inserted before flashing, cold boot, or reset by keeping TF-card CS asserted across `embedded-sdmmc` CMD0 command writes and one-byte R1 response polls.
- Added `CoreS3SharedSdDevice::prepare_for_card_acquire()` to release GPIO35 as pulled-up MISO input, apply SPI mode 0 at 400 kHz, hold TF-card CS high, and send at least 80 idle clocks before SD acquisition.
- Added split shared-SPI bring-up helpers: `CoreS3InternalI2cResources`, `CoreS3::init_internal_i2c(...)`, `CoreS3::init_core_s3_power(...)`, `CoreS3DisplayOnPoweredSharedSpiResources`, and `CoreS3::init_display_on_powered_shared_spi(...)`.
- Added `CoreS3::power_cycle_tf_card_rail(...)` for AXP2101 ALDO4 read-modify-write power cycling with hardware-validated off/on delays.
- Updated `examples/sd_block_probe` and downstream validation to initialize/probe SD before LCD SPI traffic.

## [0.4.1] - 2026-09-03

- Fixed CoreS3 shared LCD/TF-card SPI initialization to configure GPIO35 as SPI MISO for real SD reads.
- Added a CoreS3-specific shared SD `SpiDevice` that disables the GPIO35 LCD D/C output driver during TF-card transactions and restores it afterward, keeping downstream `embedded-sdmmc::SdCard::num_bytes()` usage safe and unchanged.
- Added `examples/sd_block_probe` to distinguish AW9523B card-detect from a real SD block-device capacity probe.
- Documented the M5Stack official PinMap source for GPIO35 LCD D/C and TF-card MISO sharing.

## [0.4.0] - 2026-09-03

- Pinned the ESP32-S3/CoreS3 path to the downstream-compatible `esp-hal = "=1.1.2"` dependency family.
- Added shared SPI resource APIs for composing display and TF-card users on the CoreS3 SPI2 signal group.
- Added SD-card parts compatible with `embedded-hal 1.0` `SpiDevice` and optional `embedded-sdmmc` conversion.
- Added Gateway H2 OpenThread/Spinel-facing transport traits and bounded Spinel HDLC-lite encode/decode helpers without adding Matter/Thread protocol stacks to the BSP.
- Added a minimal downstream-style validation crate for plain Cargo `xtensa-esp32s3-none-elf` builds.

## [0.3.0] - 2026-09-01

- Bumped the crate and workspace dependency to `0.3.0`.
- Added AXP2101 PMIC helpers, richer `BatteryStatus`, voltage-based percentage estimation, low-battery thresholds, and voltage smoothing.
- Added AW9523B I/O expander support for CoreS3 display/power control paths.
- Added FT6336U touch support with gestures, down/up/move events, hit testing, and rotation-aware coordinate mapping.
- Added BMI270 accelerometer/gyroscope helpers with configuration, calibration offsets, raw reads, and motion detection.
- Added BMM150 magnetometer helpers with hard-iron offset support and heading helper.
- Added BM8563 RTC helpers with `no_std` date/time types, alarm configuration, and timer metadata.
- Added ES7210 microphone ADC and AW88298 speaker amplifier configuration helpers while keeping I2S DMA in application/HAL code.
- Added lightweight `embedded-graphics` widgets: label, button, toggle, slider, progress bar, battery indicator, status bar, and menu.
- Added Gateway H2 request/response/event framing utilities without implementing Matter, Thread, Zigbee, OpenThread, or Spinel protocols.
- Added hardware smoke-test example crates for display widgets, touch, battery, IMU, compass, RTC, audio init, Gateway H2 transport, and full-board overview.
- Documented v0.3 migration notes and the BSP/application boundary.

## [0.2.0] - 2026-09-01

- Added crate-owned CoreS3 display bring-up using ESP-HAL resources.
- Added CoreS3 AXP2101/AW9523B display power, reset, and backlight initialization.
- Added an RGB565 ILI9342-compatible display driver and validation screens.
- Added smooth dirty-region sprite updates with region blitting and `flush_dirty_at`.
- Added example firmware that visibly validates display, dirty regions, dual-core execution, and Gateway H2 setup.
- Configured `cargo-embed` JTAG flashing/running through the ESP target runner.
- Added Gateway H2 metadata and crate-owned UART bring-up for the CoreS3-to-H2 host link.
- Added Matter-over-Thread configuration scaffolding for Gateway H2 consumer applications.
- Documented that concrete Matter servers, endpoints, persistence, Thread joining, and Home Assistant behavior belong in consumer firmware.

## [0.1.0] - 2026-09-01

- Initial CoreS3 BSP scaffold.
- Added board metadata and pin/device maps.
- Added display constants and initial dirty-region sprite support.
- Added power/battery status types ready for AXP2101 integration.
- Added Gateway H2 feature gate and metadata scaffold.
- Added example firmware crates and CI/release automation.
