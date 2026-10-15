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
- single-primary-executor `esp-rtos` startup and optional synchronous CPU1 worker primitives behind `feature = "rtos"`
- protocol-neutral Wi-Fi/BLE controller ownership and upstream coexistence setup behind `feature = "radio-coexistence"`
- Gateway H2 UART/framing transport primitives

Consumer applications own:

- Wi-Fi/IP networking
- Matter, Thread, Zigbee, OpenThread, Spinel protocol runtimes
- Matter endpoints/clusters/commissioning/persistence
- Home Assistant behavior
- camera image-processing/QR-decoding policy and frame persistence
- high-throughput I2S DMA capture/playback
- Wi-Fi connect/reconnect/IP/socket policy and BLE host/protocol/pool sizing
- product secrets, credentials, identities, and provisioning

## Module map

- `board`: static CoreS3 board metadata.
- `pins`: GPIO assignments and peripheral pin groups.
- `devices`: I2C addresses and device metadata.
- `bsp`: ESP-HAL-specific bring-up helpers. Target-gated.
- `display`: ILI9342C-compatible panel driver and dirty-region sprite.
- `ui`: lightweight `embedded-graphics` widgets.
- `touch`: FT6336U touch driver and coordinate transforms.
- `power`: AXP2101 and battery/power-state helpers. Prefer AXP2101 register `0xA4` as CoreS3 SOC source, matching M5Unified; voltage percentage is only a coarse fallback.
- `aw9523b`: AW9523B I/O expander driver.
- `motion`: BMI270 and BMM150 helpers.
- `rtc`: BM8563 driver and `no_std` date/time types.
- `audio`: ES7210/AW88298 configuration helpers.
- `camera`: feature-gated CoreS3 GC0308 metadata, SCCB helpers, and ESP-HAL LCD_CAM camera bring-up/capture wrappers.
- `rtos`: one `esp-rtos` startup token, one primary Embassy executor, safe internal heap registration/storage, static one-shot tasks, and an optional synchronous CPU1 worker with caller-static bootstrap stack.
- `radio`: feature-gated ownership of unique Wi-Fi/BLE peripheral tokens, requested station/AP interfaces, controller initialization, and upstream coexistence enablement; no IP or BLE host policy.
- `gateway_h2`: Gateway H2 metadata, Matter/Thread config structs, and H2 framing transport.

## CoreS3 shared LCD/TF-card SPI notes

- M5Stack's official CoreS3 PinMap is the source of truth: LCD uses GPIO37 MOSI, GPIO36 SCK, GPIO3 CS, GPIO35 D/C; TF-card uses GPIO35 MISO, GPIO37 MOSI, GPIO36 SCK, GPIO4 CS.
- For reliable SD acquisition with a pre-inserted card, initialize/probe SD before LCD SPI traffic: shared SPI, SD parts, internal I2C, CoreS3 power, ALDO4 TF-card rail power-cycle, `CoreS3SharedSdDevice::prepare_for_card_acquire()`, then `embedded-sdmmc::SdCard::num_bytes()`.
- Use `CoreS3::init_display_on_powered_shared_spi(...)` after SD probing when the internal I2C bus has already been initialized/powered.
- Cooperative LCD/SD sharing must keep LCD CS and TF-card CS from being active simultaneously and restore safe idle after error paths: LCD CS high, TF-card CS high, GPIO35 SD MISO/input default, SPI2 SD/full-duplex-safe mode.
- Keep GPIO35 aliasing hidden inside the BSP; downstream applications must not manually steal or mode-switch GPIO35.

## RTOS and radio coexistence notes

- Keep exactly one BSP-owned `esp-rtos` startup and one primary Embassy executor/time driver on CPU0. The optional CPU1 path is one synchronous worker with permanently owned caller-static bootstrap stack, not another executor.
- Do not add per-task stack, priority, arbitrary affinity, stack high-water, second-executor, or second-time-driver APIs.
- Keep `InternalHeapStorage` safe: private aligned caller-static backing, one-time internal-RAM registration, no escaped references/pointers, no reuse after failure, and one narrowly audited allocator-registration unsafe boundary.
- Preserve initialization order: HAL; internal heap when needed; `CoreS3Runtime::start`; optional CPU1 worker; board/H2/radio initialization with interrupts enabled; `CoreS3Runtime::run` last.
- `radio-coexistence` owns controller/peripheral initialization only. Applications own credentials, start/connect/reconnect behavior, IP networking, sockets, BLE host/protocol runtimes, and host pool sizing.
- Preserve unique `WIFI`/`BT` token ownership and upstream RAII behavior. Do not clone/steal tokens, invent retry guarantees, log secrets/payloads, or claim coexistence without real simultaneous-traffic validation.
- Keep the v0.5.6 dependency family together: `esp-hal 1.2.2`, `esp-rtos 0.4.0`, `esp-radio 1.0.0-beta.1`, `esp-radio-rtos-driver 0.4.2`, `esp-alloc 0.11.0`, `esp-rom-sys 0.1.5`, `embassy-time-queue-utils 0.3.2`, `embassy-executor 0.10`, `embassy-time 0.5.1`, `embedded-io-async 0.7`, and `static_cell 2.1.1`.
- Default host MSRV remains workspace Rust 1.88 (tested stable 1.98); ESP dependencies require Rust 1.95 or newer and are tested with `esp` 1.97 nightly.
- `esp-radio 1.0.0-beta.1` is the latest published staged baseline. The requested `https://docs.rs/esp-radio/1.0.0-beta.2/esp_radio/` URL is currently unpublished/404; never claim beta.2 availability or validation.
- Do not add an `rtos-task` alias; it was never published.
- Keep example configuration application-owned through `option_env!`: `RADIO_WIFI_SSID`, `RADIO_WIFI_PASSWORD` (explicit empty means open AP), `RADIO_ECHO_IPV4`, `RADIO_ECHO_PORT`, and combined-harness `RADIO_BLE_SCAN=1`. Values are embedded in artifacts even when withheld from logs.
- The combined harness only performs non-destructive SD reads; write/readback/restore is NOT RUN. Scans do not prove GATT/L2CAP exchanges, Spinel discovery does not prove Thread traffic, and bounded iteration counts do not enforce deadlines around blocking LCD/SD I/O. Never turn partial observations into aggregate PASS claims.

## CoreS3 Gateway H2 notes

- The hardware-validated stacked Module Gateway H2 v0.4 UART is CoreS3 TX GPIO17 -> H2 RXD and CoreS3 RX GPIO10 <- H2 TXD; GPIO7 owns active-low EN and GPIO18 owns H2 GPIO9/BOOT.
- Do not reintroduce PORT.A GPIO1/GPIO2 as the stacked-module route.
- UART-only OpenThread operation requires the optional S1 SPI/PTA routes disconnected. H2 SPI is unsupported unless integrated into the existing BSP LCD/SD shared-SPI arbiter.
- GPIO0 PTA conflicts with audio I2S MCLK; PTA ownership must remain explicit and opt-in.
- H2 UART/EN/BOOT composes with the S3 Wi-Fi/BLE controllers, but H2 remains an independently serviced framed transport. Reserve GPIO17/GPIO10/GPIO7/GPIO18 and do not hand those controls to radio or application GPIO users.

## CoreS3 camera notes

- M5Stack's CoreS3 UserDemo is the source for the camera pin map: XCLK GPIO2, SCCB SDA/SCL GPIO12/GPIO11, PCLK GPIO45, VSYNC GPIO46, HREF GPIO38, D0..D7 GPIO39/40/41/42/15/16/48/47, no PWDN/reset GPIO.
- Keep camera support behind feature `camera`; do not force LCD_CAM/DMA camera dependencies into default users.
- Preserve bounded caller-owned/DMA-backed frame buffers. Do not add QR decoders, image persistence, networking, or application policy to the BSP.
- GPIO2 is also Grove Port A pin 2; document/report that conflict instead of stealing it implicitly.
- Do not claim camera hardware validation unless `examples/camera_capture` has been flashed and repeated frames were observed on real CoreS3 hardware.

## Rules

- Preserve `#![no_std]`.
- Avoid heap allocation in library code.
- Prefer `embedded-hal` / `embedded-hal-async` traits.
- Do not force Embassy on all users; `rtos` and `radio-coexistence` remain opt-in.
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

Hardware examples should be flashed and visually validated on CoreS3 when changing board bring-up. Shared TF-card acquisition changes require `examples/sd_block_probe` validation with the card already inserted across flash, cold boot, repeated reset, remove/reinsert/reset, and LCD-after-SD display bring-up. Cooperative LCD/TF-card SPI changes also require `examples/display_sd_coexist` validation with alternating LCD updates and raw SD read/write/readback while the display remains initialized.
