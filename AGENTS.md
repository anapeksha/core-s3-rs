# AGENTS.md

Guidance for future AI agents working on `core-s3-rs`.

## Project intent

`core-s3` is a Rust `no_std` BSP crate for the M5Stack CoreS3 K128 / ESP32-S3. It should own reusable board/peripheral bring-up while leaving application policy and unrelated ESP peripherals to consumer firmware.

The BSP should stay modular. Do not collapse everything into a monolithic board object.

## Architecture boundaries

The BSP owns:

- board metadata
- pin/device maps
- display power/reset/backlight/LCD initialization
- small reusable peripheral drivers/config helpers
- HAL-agnostic types and pure logic
- ESP-HAL board bring-up helpers behind `feature = "esp-hal"` and `target_arch = "xtensa"`
- Gateway H2 UART/framing transport primitives

Consumer applications own:

- Wi-Fi/IP networking
- Matter, Thread, Zigbee, OpenThread, Spinel protocol runtimes
- Matter endpoints/clusters/commissioning/persistence
- Home Assistant behavior
- high-throughput I2S DMA capture/playback
- product secrets, credentials, identities, and provisioning

## Module map

- `board`: static CoreS3 board metadata.
- `pins`: GPIO assignments and peripheral pin groups.
- `devices`: I2C addresses and device metadata.
- `bsp`: ESP-HAL-specific bring-up helpers. Target-gated.
- `display`: ILI9342C-compatible panel driver and dirty-region sprite.
- `ui`: lightweight `embedded-graphics` widgets.
- `touch`: FT6336U touch driver and coordinate transforms.
- `power`: AXP2101 and battery/power-state helpers.
- `aw9523b`: AW9523B I/O expander driver.
- `motion`: BMI270 and BMM150 helpers.
- `rtc`: BM8563 driver and `no_std` date/time types.
- `audio`: ES7210/AW88298 configuration helpers.
- `gateway_h2`: Gateway H2 metadata, Matter/Thread config structs, and H2 framing transport.

## CoreS3 shared LCD/TF-card SPI notes

- M5Stack's official CoreS3 PinMap is the source of truth: LCD uses GPIO37 MOSI, GPIO36 SCK, GPIO3 CS, GPIO35 D/C; TF-card uses GPIO35 MISO, GPIO37 MOSI, GPIO36 SCK, GPIO4 CS.
- For reliable SD acquisition with a pre-inserted card, initialize/probe SD before LCD SPI traffic: shared SPI, SD parts, internal I2C, CoreS3 power, ALDO4 TF-card rail power-cycle, `CoreS3SharedSdDevice::prepare_for_card_acquire()`, then `embedded-sdmmc::SdCard::num_bytes()`.
- Use `CoreS3::init_display_on_powered_shared_spi(...)` after SD probing when the internal I2C bus has already been initialized/powered.
- Keep GPIO35 aliasing hidden inside the BSP; downstream applications must not manually steal or mode-switch GPIO35.

## Rules

- Preserve `#![no_std]`.
- Avoid heap allocation in library code.
- Prefer `embedded-hal` / `embedded-hal-async` traits.
- Do not force Embassy on all users.
- Do not add GUI frameworks unless explicitly requested and justified.
- Do not add Matter/OpenThread/Zigbee protocol stacks to `core-s3`.
- Keep examples visible and useful on real CoreS3 hardware.
- Do not create `examples/support`.
- Do not publish crates or create GitHub releases unless the user explicitly asks.
- If release notes are requested, create markdown files only.

## Validation commands

Preferred checks:

```sh
rustup run stable cargo fmt --all -- --check
rustup run stable cargo test -p core-s3 --all-features --target aarch64-apple-darwin
rustup run esp cargo check --workspace --all-features --target xtensa-esp32s3-none-elf
rustup run esp cargo clippy --workspace --all-features --target xtensa-esp32s3-none-elf -- -D warnings
```

Hardware examples should be flashed and visually validated on CoreS3 when changing board bring-up. Shared TF-card changes require `examples/sd_block_probe` validation with the card already inserted across flash, cold boot, repeated reset, remove/reinsert/reset, and LCD-after-SD display bring-up.
