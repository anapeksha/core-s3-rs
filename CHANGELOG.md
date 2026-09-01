# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

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
