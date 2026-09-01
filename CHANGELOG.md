# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

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
