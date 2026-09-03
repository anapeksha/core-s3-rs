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
- feature-gated Gateway H2 UART/framing/OpenThread transport surfaces and Spinel HDLC-lite codec behind `gateway-h2`
- optional TF-card SD parts compatible with `embedded-sdmmc`

> Hardware note: v0.4.2 keeps the default ESP32-S3 path on the current stable downstream stack around `esp-hal = "=1.1.2"`. Display, touch, battery, motion, compass, RTC, audio, and Gateway H2 examples were smoke-tested during v0.3 development. The shared LCD/TF-card SPI API configures GPIO35 as SD MISO, handles the CoreS3 LCD D/C vs TF-card MISO mode switch inside the BSP, and keeps TF-card CS asserted across CMD0 response polling for reliable pre-inserted-card acquisition.

## Peripheral support

| Peripheral            | Address / pins                                                        | Support                                                                                                                  |
| --------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| ILI9342C SPI LCD      | MOSI GPIO37, SCLK GPIO36, CS GPIO3, D/C GPIO35, TF CS GPIO4 held high | init, RGB565 drawing, clipping, rotation/MADCTL, dirty-region blits, shared-SPI initializer                              |
| FT6336U touch         | I2C `0x38` on SDA GPIO12/SCL GPIO11                                   | touch report parsing, down/up/move, gestures, rotation mapping, hit testing                                              |
| AXP2101 PMIC          | I2C `0x34`                                                            | CoreS3 defaults, backlight rail, battery voltage/status helpers, shutdown/sleep prep                                     |
| AW9523B expander      | I2C `0x58`                                                            | CoreS3 defaults and safe output helpers, LCD reset pin helper                                                            |
| BMI270 IMU            | I2C `0x69`                                                            | init/config, accel/gyro raw reads, offsets, basic motion detection                                                       |
| BMM150 magnetometer   | `0x10` on BMI270 auxiliary sensor-hub I2C                             | generic register helper, hard-iron offset, integer heading helper; CoreS3 access path needs BMI270 sensor-hub validation |
| BM8563 RTC            | I2C `0x51`                                                            | get/set date-time, alarms, timer metadata                                                                                |
| ES7210 microphone ADC | I2C `0x40`, I2S GPIO0/34/33/13/14                                     | configuration helper; I2S DMA remains app/HAL-owned                                                                      |
| AW88298 speaker amp   | I2C `0x36`, I2S GPIO0/34/33/13/14                                     | configuration helper; I2S DMA remains app/HAL-owned                                                                      |
| Gateway H2            | UART1, TX GPIO1, RX GPIO2, 115200 baud                                | UART bring-up, small request/response/event framing, OpenThread/Spinel transport traits, and Spinel HDLC-lite codec      |
| TF-card slot          | SCLK GPIO36, MOSI GPIO37, MISO GPIO35, CS GPIO4                       | slot metadata, card-detect helper, shared-SPI `SpiDevice` parts, optional `embedded-sdmmc::SdCard` conversion            |

## Repository layout

```text
crates/core-s3/                    no_std BSP crate
examples/hello_world/             basic LCD validation
examples/downstream_esp_hal_112/   downstream compatibility build for esp-hal 1.1.2
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
examples/sd_card/                 AW9523B TF-card detect demo
examples/sd_block_probe/          shared-SPI embedded-sdmmc capacity probe
examples/gateway_h2_transport/    H2 framing smoke-test shell
examples/full_board_demo/         board overview smoke-test shell
.github/workflows/                PR validation and firmware release automation
```

## Features

| Feature      | Description                                                                                                                                                                                |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `defmt`      | Enables `defmt` formatting for supported dependency-free public types.                                                                                                                     |
| `esp-hal`    | Enables ESP-HAL-backed CoreS3 bring-up helpers on Xtensa ESP32-S3 targets.                                                                                                                 |
| `gateway-h2` | Exposes `core_s3::gateway_h2`, Gateway H2 metadata, UART bring-up, Matter/Thread config types, H2 framing, OpenThread/Spinel transport traits, and Spinel HDLC-lite encode/decode helpers. |
| `sdmmc`      | Enables conversion from BSP SD parts into `embedded_sdmmc::SdCard<SPI, DELAY>`.                                                                                                            |

## Minimal display example

```rust
use core_s3::{CoreS3, bsp::CoreS3DisplayResources, ui::{Label, Theme}};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};

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

parts.display.clear(Rgb565::BLACK)?;
Label {
    text: "LCD ready",
    top_left: Point::new(24, 48),
    color: Rgb565::CYAN,
}
.draw(&mut parts.display)?;
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

## Shared LCD + TF-card SPI

CoreS3 routes the LCD and TF-card socket through the same SPI signal group:

```text
SPI2
SCLK GPIO36
MOSI GPIO37
MISO GPIO35
LCD CS GPIO3
TF CS GPIO4
```

For firmware that needs both devices, use `CoreS3::init_shared_spi`, store the returned `CoreS3SharedSpiParts` in a `static_cell::StaticCell`, then create the LCD and SD chip-select devices independently with `CoreS3::init_display_on_shared_spi` and `CoreS3::init_sd_on_shared_spi`.

M5Stack's official CoreS3 PinMap lists LCD D/C on GPIO35 and TF-card MISO on the same GPIO35 pad. The BSP configures SPI2 with GPIO35 as MISO and wraps SD access in `CoreS3SharedSdDevice`, which disables the LCD D/C output driver while TF-card CS is active and leaves GPIO35 as a pulled-up MISO input after SD transactions. The LCD D/C facade restores output mode when the display actually writes.

For robust acquisition when a card is already inserted at flash/cold-boot/reset time, initialize and probe SD before LCD SPI traffic: create shared SPI and SD parts, initialize internal I2C, call `CoreS3::init_core_s3_power(...)`, `CoreS3::power_cycle_tf_card_rail(...)`, `sd_parts.spi_device.prepare_for_card_acquire()`, then call `CoreS3SdParts::into_sdmmc()` and `SdCard::num_bytes()`. After the SD probe, initialize the LCD with `CoreS3::init_display_on_powered_shared_spi(...)`.

Downstream firmware can keep using `embedded_hal::spi::SpiDevice` and, with feature `sdmmc`, `CoreS3SdParts::into_sdmmc()` returns an `embedded_sdmmc::SdCard<SPI, DELAY>` suitable for a real `num_bytes()` capacity probe.

The BSP intentionally does not provide credential/token/secret abstractions, fake filesystems, plaintext storage policy, or encryption; downstream firmware should encrypt sensitive bytes before writing them to SD.

## Matter / Gateway H2 scope

The BSP does **not** implement Matter, Thread, Zigbee, OpenThread CLI, or the OpenThread state machine. M5Stack's Gateway H2 Thread Border Router documentation builds ESP-IDF's `examples/openthread/ot_rcp` firmware for the ESP32-H2 module, so `core_s3::gateway_h2::spinel` provides the bounded Spinel HDLC-lite byte-stuffing/FCS codec needed by downstream OpenThread host integrations. The downstream application still owns OpenThread host integration, Matter commissioning/runtime, and protocol policy.

Consumer firmware should own:

- Wi-Fi/IP networking
- Matter server/runtime such as `rs-matter`
- endpoints and clusters
- commissioning and persistence
- Thread/OpenThread/Spinel/Zigbee protocol integration
- Home Assistant behavior

## Building

The default ESP32-S3 build is kept compatible with:

```toml
esp-hal = "=1.1.2"
esp-println = "=0.15.0"
esp-backtrace = "=0.17.0"
esp-rom-sys = "=0.1.4"
esp-alloc = "=0.10.0"
esp-radio = "=1.0.0-beta.0"
esp-radio-rtos-driver = "=0.3.0"
esp-storage = "=0.7.0"
embedded-hal = "1.0"
```

Install the ESP Rust toolchain with [`espup`](https://github.com/esp-rs/espup), then:

```sh
cargo +esp check --workspace --all-features --release --target xtensa-esp32s3-none-elf
cargo +esp build -p display_widgets --release --target xtensa-esp32s3-none-elf
cargo +esp build -p full_board_demo --release --target xtensa-esp32s3-none-elf
```

Flash an example with `cargo-embed` through the workspace runner:

```sh
cargo +esp run -p display_widgets --release --target xtensa-esp32s3-none-elf
```

`Embed.toml` is configured for ESP32-S3 JTAG. GDB is enabled so dynamic examples continue running while the probe session remains attached.

## v0.4 migration notes

- Update dependencies from `core-s3 = "0.3"` to `core-s3 = "0.4"`.
- Prefer `core-s3 = "0.4.2"` or newer for shared LCD + TF-card SPI: v0.4.2 fixes pre-inserted TF-card acquisition by adding CMD0 CS hold handling, SD-safe GPIO35 defaults, 400 kHz idle clocks, and an ALDO4 power-cycle helper.
- ESP-HAL users should pin to the supported `esp-hal = "=1.1.2"` family unless they explicitly opt into and validate a newer stack in their application.
- Existing `CoreS3::init_display` remains available for display-only firmware.
- Firmware that needs both LCD and TF-card access should migrate to `CoreS3::init_shared_spi`, `CoreS3::init_sd_on_shared_spi`, `CoreS3::init_internal_i2c`, `CoreS3::power_cycle_tf_card_rail`, `CoreS3SharedSdDevice::prepare_for_card_acquire`, and `CoreS3::init_display_on_powered_shared_spi` when SD must be acquired before LCD traffic. The older `CoreS3::init_display_on_shared_spi` remains available for display-first code.
- Use feature `sdmmc` if you want BSP SD parts to convert directly into `embedded_sdmmc::SdCard`.
- Gateway H2 Matter/Thread config types remain configuration-only. Use `gateway_h2::transport` for H2 framing and OpenThread transport traits, and use `gateway_h2::spinel` for the Spinel HDLC-lite codec when the H2 is flashed with ESP-IDF OpenThread RCP firmware.
- Full Matter server/runtime code still belongs in consumer applications, not this BSP.

## v0.3 migration notes

- Update dependencies from `core-s3 = "0.2"` to `core-s3 = "0.3"`.
- `BatteryStatus` now includes percentage estimate, charge state, external-power state, and low-battery state. Code using the old `{ millivolts, state }` fields should migrate to `charge_state` and the richer status fields.

## Unsupported / application-owned functionality

- Camera driver support is metadata-only.
- High-throughput I2S DMA capture/playback is application/HAL-owned.
- Matter, Thread, Zigbee, OpenThread, and Spinel protocol stacks are application-owned.
- Voltage-based battery percentage is approximate and should not be used as a precise fuel gauge.
