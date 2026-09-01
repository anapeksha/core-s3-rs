# core-s3-rs

Rust board support package for the **M5Stack CoreS3 K128** (ESP32-S3) with optional support for a stacked **M5Stack Zigbee Gateway H2**.

The crate is intentionally `#![no_std]` and keeps the reusable BSP layer small:

- board metadata and pin/device maps for CoreS3 peripherals
- display constants and a dirty-region sprite framebuffer for efficient partial repainting
- power/battery status types ready for AXP2101 integration
- feature-gated Gateway H2 metadata behind `gateway-h2`
- example firmware crates and CI/release automation

> Hardware note: pin maps are scaffolded from the supplied CoreS3 materials and should be validated on the exact CoreS3/base stack revision before relying on every peripheral in production.

## Repository layout

```text
crates/core-s3/          no_std BSP crate
examples/hello_world/   minimal ESP32-S3 firmware skeleton
examples/dirty_regions/ display sprite example
examples/dual_core/     PRO CPU + APP CPU example
examples/gateway_h2/    Zigbee Gateway H2 feature-gated example
.github/workflows/      PR validation and firmware release
```

## Features

| Feature      | Description                                                         |
| ------------ | ------------------------------------------------------------------- |
| `defmt`      | Enables `defmt` formatting for supported dependencies.              |
| `gateway-h2` | Exposes `core_s3::gateway_h2` for Zigbee Gateway H2 stack metadata. |

## Display dirty-region sprite

`core_s3::display::DirtySprite` stores an off-screen framebuffer and tracks only changed rectangles. Drawing through `embedded-graphics` marks dirty regions automatically; `flush_dirty` then repaints only those pixels into the real display target.

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

## Dual-core support

ESP32-S3 has two Xtensa LX7 cores: the PRO CPU starts `main`, and the APP CPU can be started by firmware. This BSP supports both cores through `esp-hal`'s `CpuControl` API; see `examples/dual_core` for a minimal example that starts the APP CPU with its own stack and shares state through a critical-section mutex.

## Release workflow

Pushing a tag like `v0.1.0` runs `.github/workflows/release.yml`, builds release firmware, converts each example to an ESP32-S3 `.bin` image with `espflash save-image`, and publishes only `.bin` firmware artifacts.
