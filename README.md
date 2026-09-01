# core-s3

Rust board support package for the **M5Stack CoreS3 K128** (ESP32-S3) with optional support for an **M5Stack Gateway H2** Thread/Zigbee co-processor.

The crate is intentionally `#![no_std]` and keeps the reusable BSP layer small:

- board metadata and pin/device maps for CoreS3 peripherals
- display constants and a dirty-region sprite framebuffer for efficient partial repainting
- power/battery status types ready for AXP2101 integration
- feature-gated Gateway H2 metadata and UART bring-up behind `gateway-h2`
- example firmware crates and CI/release automation

> Hardware note: pin maps are scaffolded from the supplied CoreS3 materials and should be validated on the exact CoreS3/base stack revision before relying on every peripheral in production.

## Repository layout

```text
crates/core-s3/          no_std BSP crate
examples/hello_world/   minimal ESP32-S3 firmware skeleton
examples/dirty_regions/ display sprite example
examples/dual_core/     PRO CPU + APP CPU example
examples/gateway_h2/    Gateway H2 UART + Matter/Thread scaffold example
.github/workflows/      PR validation and firmware release
```

## Features

| Feature      | Description                                                                                                            |
| ------------ | ---------------------------------------------------------------------------------------------------------------------- |
| `defmt`      | Enables `defmt` formatting for supported dependencies.                                                                 |
| `gateway-h2` | Exposes `core_s3::gateway_h2`, Gateway H2 metadata, BSP UART bring-up, and `rs-matter` Matter-over-Thread setup types. |

## Display dirty-region sprite

`core_s3::display::DirtySprite` stores an off-screen framebuffer and tracks only changed rectangles. Drawing through `embedded-graphics` marks dirty regions automatically; `flush_dirty` / `flush_dirty_at` then blit only final pixels for the changed regions into the real display target, avoiding visible clear-then-redraw flashes.

```rust
use core_s3::display::DirtySprite;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::{PrimitiveStyle, Rectangle}};

type FullscreenSprite = DirtySprite<Rgb565, 320, 240, { 320 * 240 }, 32>;

let mut sprite = FullscreenSprite::new(Rgb565::BLACK)?;
Rectangle::new(Point::new(16, 16), Size::new(64, 32))
    .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
    .draw(&mut sprite)
    .unwrap();

// Later, once a concrete LCD DrawTarget is configured:
// sprite.flush_dirty(&mut display)?;
# Ok::<(), core_s3::display::DirtySpriteError>(())
```

For RAM-sensitive UI, prefer smaller per-widget sprites and compose them into the panel.

## Building

Install the ESP Rust toolchain with [`espup`](https://github.com/esp-rs/espup), then:

```sh
cargo +esp check --workspace --all-features --target xtensa-esp32s3-none-elf
cargo +esp build -p hello_world --release --target xtensa-esp32s3-none-elf
cargo +esp build -p dirty_regions --release --target xtensa-esp32s3-none-elf
cargo +esp build -p dual_core --release --target xtensa-esp32s3-none-elf
cargo +esp build -p gateway_h2 --release --features gateway-h2 --target xtensa-esp32s3-none-elf
```

Flash an example with `cargo-embed`:

```sh
cargo +esp embed --package hello_world --release --target xtensa-esp32s3-none-elf
# or, through the workspace alias:
cargo +esp flash --package hello_world --release
```

`Embed.toml` is configured for the ESP32-S3 target and probe-rs flashing. If you have more than one compatible probe connected, set the probe VID/PID locally in `Embed.toml`.

## Matter over Thread with Gateway H2

Enable `gateway-h2` to use Gateway H2 metadata, the crate-owned CoreS3-to-H2 UART bring-up helper, and Matter-over-Thread setup types. The BSP re-exports `rs-matter` as `core_s3::gateway_h2::matter::stack` so firmware crates can instantiate the concrete Matter server while keeping regular CoreS3 builds free of Matter dependencies.

Gateway H2 firmware is commonly OpenThread RCP/Spinel, OpenThread CLI, or a standalone Thread/Zigbee application depending on what is flashed to the ESP32-H2. It is not assumed to be an AT-command modem. The BSP initializes the host UART transport; consumer firmware is responsible for the concrete H2 protocol driver, Thread joining/commissioning flow, Matter endpoints, and Home Assistant behavior.

For Home Assistant validation, a Raspberry Pi 5 running Home Assistant OS also needs a Thread Border Router/radio, such as Home Assistant Connect ZBT-1/SkyConnect or another supported OpenThread Border Router. This crate can provide the CoreS3 device side, but Home Assistant will only discover it once consumer firmware runs a real Matter server over a Thread network visible to Home Assistant.

```toml
[dependencies]
core-s3 = { version = "0.1", features = ["gateway-h2"] }
```

```rust
use core_s3::gateway_h2::{
    matter::{MatterOverThreadConfig, MatterServerConfig, ThreadDatasetConfig},
    GatewayH2,
};

let gateway = GatewayH2::GROVE_UART;
let matter = MatterServerConfig::new(0xFFF1, 0x8001, 3840, 20_202_021, "CoreS3 Gateway H2");
let thread = ThreadDatasetConfig::new("core-s3-thread", 0x1234, [0; 8], 15, [0xAA; 16]);
let setup = MatterOverThreadConfig::new(gateway, matter, thread);
# let _ = setup;
```

## Dual-core support

ESP32-S3 has two Xtensa LX7 cores: the PRO CPU starts `main`, and the APP CPU can be started by firmware. This BSP supports both cores through `esp-hal`'s `CpuControl` API; see `examples/dual_core` for a minimal example that starts the APP CPU with its own stack and shares state through a critical-section mutex.

## Release workflow

Pushing a tag like `v0.1.0` runs `.github/workflows/release.yml`, builds release firmware, converts each example to an ESP32-S3 `.bin` image with `espflash save-image`, and publishes only `.bin` firmware artifacts.
