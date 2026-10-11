# core-s3

A modular, allocator-free `#![no_std]` board support crate for the **M5Stack CoreS3 K128** (ESP32-S3).

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

| Feature      | Description                                                                    |
| ------------ | ------------------------------------------------------------------------------ |
| `esp-hal`    | ESP-HAL-backed CoreS3 initialization on Xtensa ESP32-S3 targets                |
| `camera`     | GC0308 SCCB setup and bounded LCD_CAM DMA capture; implies `esp-hal`           |
| `gateway-h2` | Stacked Gateway H2 metadata, EN/BOOT control, framing, and buffered async UART |
| `sdmmc`      | Conversion to `embedded_sdmmc::SdCard`                                         |
| `defmt`      | Optional formatting implementations                                            |

Default builds remain HAL-independent and `no_std`.

## Usage

```toml
[dependencies]
core-s3 = { version = "0.5.4", features = ["esp-hal"] }
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

## Gateway H2

The official stacked Module Gateway H2 route is:

```text
CoreS3 TX GPIO10 -> H2 RXD
CoreS3 RX GPIO17 <- H2 TXD
CoreS3 GPIO7     -> H2 EN (active-low reset)
CoreS3 GPIO18    -> H2 GPIO9/BOOT
```

`CoreS3::init_gateway_h2_openthread(...)` returns:

- a statically buffered `embedded_io_async 0.7::Read + Write` transport;
- an independently spawned UART pump that continuously drains RX;
- safe reset and ROM-bootloader control.

The BSP does not depend on OpenThread. `examples/gateway_h2_openthread` verifies integration with `openthread 0.4.0`'s `UartSpinelTransport`.

### Gateway H2 DIP switches

For UART-only RCP operation on Module Gateway H2 v0.4, set routed positions **S1-1, S1-2, S1-3, S1-5, S1-6, S1-7, and S1-8 OFF**. S1-4 is unused. UART, EN, and BOOT are fixed M-Bus routes.

H2 SPI is unsupported because its optional routes conflict with the BSP-owned LCD/TF-card bus on GPIO35/36/37 and audio output on GPIO13. PTA is opt-in metadata only; GPIO0 `WL_ACTIVE` conflicts with audio I2S MCLK. Software cannot detect physical DIP positions.

## Building

The ESP path currently targets `esp-hal = "=1.2.2"`. Install the ESP Rust toolchain with [`espup`](https://github.com/esp-rs/espup), then run:

```sh
cargo +stable test -p core-s3 --all-features
cargo +esp check --workspace --all-features --release --target xtensa-esp32s3-none-elf
cargo +esp run -p hello_world --release --target xtensa-esp32s3-none-elf
```

## Scope

Consumer applications own Wi-Fi/IP, Matter/OpenThread/Zigbee runtimes, datasets, commissioning, credentials, persistence, high-throughput audio policy, and camera image processing. The BSP does not allocate, persist frames, or start hidden background tasks.

## Hardware status

Shared LCD/TF-card coexistence and the GC0308 camera preview have been validated on real CoreS3 hardware. The corrected Gateway H2 GPIO10/GPIO17/GPIO7/GPIO18 path has been smoke-tested for CoreS3 boot, reset, and buffered UART initialization on CoreS3 rev1 + Gateway H2 v0.4; a real Spinel exchange remains unvalidated.

## License

Licensed under the repository's license terms.
