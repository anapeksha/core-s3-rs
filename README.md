# core-s3

Rust board support package for the **M5Stack CoreS3 K128** (ESP32-S3) with optional support for an **M5Stack Gateway H2** Thread/Zigbee co-processor.

The crate is intentionally `#![no_std]` and keeps the reusable BSP layer modular:

- board metadata and pin/device maps for CoreS3 peripherals
- crate-owned ILI9342C-compatible display bring-up
- lightweight `embedded-graphics` widgets and dirty-region helpers
- FT6336U touch parsing and rotation-aware coordinate mapping
- AXP2101 power/battery helpers and AW9523B expander support
- BMI270/BMM150 motion/orientation helpers
- BM8563 RTC helpers with small `no_std` date/time types
- ES7210/AW88298 audio configuration helpers
- feature-gated Gateway H2 UART/framing transport behind `gateway-h2`

> Hardware note: v0.3.0 expands the BSP surface significantly. Display bring-up was validated in v0.2.0. New sensor/audio/power drivers are register-level BSP foundations cross-checked against the official M5Stack CoreS3 docs and should be smoke-tested on your exact CoreS3 hardware before production use.

## Peripheral support

| Peripheral            | Address / pins                                                        | v0.3 support                                                                                                             |
| --------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| ILI9342C SPI LCD      | MOSI GPIO37, SCLK GPIO36, CS GPIO3, D/C GPIO35, TF CS GPIO4 held high | init, RGB565 drawing, clipping, rotation/MADCTL, dirty-region blits                                                      |
| FT6336U touch         | I2C `0x38` on SDA GPIO12/SCL GPIO11                                   | touch report parsing, down/up/move, gestures, rotation mapping, hit testing                                              |
| AXP2101 PMIC          | I2C `0x34`                                                            | CoreS3 defaults, backlight rail, battery voltage/status helpers, shutdown/sleep prep                                     |
| AW9523B expander      | I2C `0x58`                                                            | CoreS3 defaults and safe output helpers, LCD reset pin helper                                                            |
| BMI270 IMU            | I2C `0x69`                                                            | init/config, accel/gyro raw reads, offsets, basic motion detection                                                       |
| BMM150 magnetometer   | `0x10` on BMI270 auxiliary sensor-hub I2C                             | generic register helper, hard-iron offset, integer heading helper; CoreS3 access path needs BMI270 sensor-hub validation |
| BM8563 RTC            | I2C `0x51`                                                            | get/set date-time, alarms, timer metadata                                                                                |
| ES7210 microphone ADC | I2C `0x40`, I2S GPIO0/34/33/13/14                                     | configuration helper; I2S DMA remains app/HAL-owned                                                                      |
| AW88298 speaker amp   | I2C `0x36`, I2S GPIO0/34/33/13/14                                     | configuration helper; I2S DMA remains app/HAL-owned                                                                      |
| Gateway H2            | UART1, TX GPIO1, RX GPIO2, 115200 baud                                | UART bring-up plus small request/response/event framing layer                                                            |

## Repository layout

```text
crates/core-s3/                    no_std BSP crate
examples/hello_world/             basic LCD validation
examples/dirty_regions/           dirty-region animation
examples/dual_core/               PRO CPU + APP CPU example
examples/gateway_h2/              Gateway H2 UART scaffold
examples/display_widgets/         widget rendering smoke test
examples/touch_demo/              FT6336U smoke-test shell
examples/battery_status/          AXP2101/battery smoke-test shell
examples/imu/                     BMI270 smoke-test shell
examples/compass/                 BMM150 smoke-test shell
examples/rtc/                     BM8563 smoke-test shell
examples/audio_init/              ES7210/AW88298 smoke-test shell
examples/gateway_h2_transport/    H2 framing smoke-test shell
examples/full_board_demo/         board overview smoke-test shell
.github/workflows/                PR validation and firmware release automation
```

## Features

| Feature      | Description                                                                                                                  |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------- |
| `defmt`      | Enables `defmt` formatting for supported dependency-free public types.                                                       |
| `esp-hal`    | Enables ESP-HAL-backed CoreS3 bring-up helpers on Xtensa ESP32-S3 targets.                                                   |
| `gateway-h2` | Exposes `core_s3::gateway_h2`, Gateway H2 metadata, UART bring-up, Matter/Thread config types, and the H2 framing transport. |

## Minimal display example

```rust
use core_s3::{CoreS3, bsp::CoreS3DisplayResources};
use embedded_graphics::pixelcolor::Rgb565;

let mut parts = CoreS3::init_display(CoreS3DisplayResources {
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

parts.display.draw_validation_screen("CoreS3", "LCD ready", Rgb565::CYAN)?;
```

## Widgets and dirty-region sprite

`core_s3::display::DirtySprite` stores an off-screen framebuffer and tracks only changed rectangles. Drawing through `embedded-graphics` marks dirty regions automatically; `flush_dirty` / `flush_dirty_at` then blit only final pixels for changed regions into the real display target.

`core_s3::ui` provides small reusable widgets without a GUI framework dependency:

- `Label`
- `Button`
- `Toggle`
- `Slider`
- `ProgressBar`
- `BatteryIndicator`
- `StatusBar`
- `Menu`

## Matter / Gateway H2 scope

The BSP does **not** implement Matter, Thread, Zigbee, OpenThread CLI, or Spinel. It provides Gateway H2 metadata, CoreS3-side UART bring-up, and a small framing layer suitable for a higher-level application protocol later.

Consumer firmware should own:

- Wi-Fi/IP networking
- Matter server/runtime such as `rs-matter`
- endpoints and clusters
- commissioning and persistence
- Thread/OpenThread/Spinel/Zigbee protocol integration
- Home Assistant behavior

## Building

Install the ESP Rust toolchain with [`espup`](https://github.com/esp-rs/espup), then:

```sh
cargo +esp check --workspace --all-features --target xtensa-esp32s3-none-elf
cargo +esp build -p display_widgets --release --target xtensa-esp32s3-none-elf
cargo +esp build -p full_board_demo --release --target xtensa-esp32s3-none-elf
```

Flash an example with `cargo-embed` through the workspace runner:

```sh
cargo +esp run -p display_widgets --release --target xtensa-esp32s3-none-elf
```

`Embed.toml` is configured for ESP32-S3 JTAG. GDB is enabled so dynamic examples continue running while the probe session remains attached.

## v0.3 migration notes

- Update dependencies from `core-s3 = "0.2"` to `core-s3 = "0.3"`.
- `BatteryStatus` now includes percentage estimate, charge state, external-power state, and low-battery state. Code using the old `{ millivolts, state }` fields should migrate to `charge_state` and the richer status fields.
- Gateway H2 Matter/Thread config types remain configuration-only. Use `gateway_h2::transport` for the new BSP framing layer if your H2 firmware has a compatible host protocol.
- Full Matter server/runtime code still belongs in consumer applications, not this BSP.

## Unsupported / application-owned functionality

- Camera driver support is metadata-only.
- High-throughput I2S DMA capture/playback is application/HAL-owned.
- Matter, Thread, Zigbee, OpenThread, and Spinel protocol stacks are application-owned.
- Voltage-based battery percentage is approximate and should not be used as a precise fuel gauge.
