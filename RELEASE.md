# Release plan: v0.2.0

This document tracks the `core-s3` `v0.2.0` release.

## Scope

`v0.2.0` turns `core-s3` from a metadata scaffold into a usable CoreS3 BSP:

- crate-owned CoreS3 display bring-up
- AXP2101/AW9523B LCD power, reset, and backlight initialization
- RGB565 ILI9342-compatible display driver
- smooth dirty-region sprite region blitting
- visible hardware-validation examples
- `cargo-embed` JTAG runner configuration
- Gateway H2 UART transport bring-up
- Matter-over-Thread configuration scaffolding for consumer applications

The release intentionally does **not** include a complete Matter server. Matter endpoints, commissioning, persistence, network transport, and Home Assistant behavior are application concerns.

## Pre-release checklist

Run from the workspace root:

```sh
rustup run stable cargo fmt --all -- --check
rustup run stable cargo test -p core-s3 --all-features --target aarch64-apple-darwin
rustup run esp cargo check --workspace --all-features --target xtensa-esp32s3-none-elf
rustup run esp cargo clippy --workspace --all-features --target xtensa-esp32s3-none-elf -- -D warnings
cargo package -p core-s3 --allow-dirty
```

If the Xtensa linker is not on `PATH`, prepend the ESP toolchain bin directory, for example:

```sh
PATH=/Users/anapeksha/.rustup/toolchains/esp/xtensa-esp-elf/esp-15.2.0_20250920/xtensa-esp-elf/bin:/Users/anapeksha/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin \
  rustup run esp cargo check --workspace --all-features --target xtensa-esp32s3-none-elf
```

## Hardware validation checklist

Flash and visually validate each example on a connected CoreS3:

```sh
cargo run -p hello_world --release --target xtensa-esp32s3-none-elf
cargo run -p dirty_regions --release --target xtensa-esp32s3-none-elf
cargo run -p dual_core --release --target xtensa-esp32s3-none-elf
cargo run -p gateway_h2 --release --target xtensa-esp32s3-none-elf
```

Expected screens:

- `hello_world`: board/display bring-up details are readable.
- `dirty_regions`: moving block animates smoothly without clear/redraw flashes.
- `dual_core`: `Core 0 frames` and `Core 1 ticks` counters increase while `cargo-embed` remains attached.
- `gateway_h2`: Gateway H2 UART and Matter/Thread scaffold values are shown. `UART OK; no CLI/AT reply` is acceptable for non-CLI/non-AT H2 firmware.

`Embed.toml` currently enables GDB so `cargo-embed` stays attached and firmware keeps running for interactive validation. Commands may need to be stopped manually or allowed to timeout in automation.

## Release steps

1. Ensure `crates/core-s3/Cargo.toml` version is `0.2.0`.
2. Ensure `Cargo.toml` workspace dependency points at `core-s3` `0.2.0`.
3. Ensure `CHANGELOG.md` contains a `0.2.0` section.
4. Run the pre-release checklist.
5. Commit release prep:

   ```sh
   git add Cargo.toml Cargo.lock crates/core-s3/Cargo.toml CHANGELOG.md RELEASE.md README.md
   git commit -m "chore: prepare v0.2.0 release"
   ```

6. Tag:

   ```sh
   git tag v0.2.0
   ```

7. Push branch and tag:

   ```sh
   git push origin main
   git push origin v0.2.0
   ```

8. Publish crate:

   ```sh
   cargo publish -p core-s3
   ```

Do not publish if `cargo package` shows unexpected contents or if the hardware validation pass is incomplete.

## Consumer application prompt: Matter over Wi-Fi / Home Assistant

Use this prompt to start a separate consumer firmware application that depends on `core-s3`:

```text
Create a new Rust embedded consumer application for M5Stack CoreS3 using the `core-s3` BSP crate. The app should demonstrate a Home Assistant-compatible Matter device over Wi-Fi first, not Matter-over-Thread.

Requirements:

- Use `core-s3 = { version = "0.2", features = ["esp-hal"] }` for board/display bring-up.
- Depend on `rs-matter` directly in the application; do not add the Matter server runtime to the BSP crate.
- Keep application-specific identity and secrets in the app:
  - Matter vendor ID/product ID
  - setup discriminator
  - setup passcode
  - device name
  - Wi-Fi credentials or provisioning flow
  - fabric/session persistence
- Bring up CoreS3 display using `CoreS3::init_display` and show Matter status on-screen:
  - Wi-Fi connecting/connected
  - IP address
  - commissioning status
  - pairing/manual code or QR payload when available
  - fabric count
  - endpoint state
- Implement the smallest useful Matter endpoint first: an On/Off Light endpoint.
- Home Assistant should be able to commission and control the On/Off state.
- Reflect Home Assistant state changes on the CoreS3 display.
- Keep reusable board initialization in `core-s3`; keep Wi-Fi, Matter server, endpoint model, persistence, and Home Assistant behavior in this consumer app.
- Add a README explaining how to run Home Assistant, enable Matter integration, commission the device, and troubleshoot discovery.
- Validate on hardware with `cargo run --release --target xtensa-esp32s3-none-elf` using the workspace cargo-embed runner.

Architecture target:

CoreS3 consumer app
  -> core-s3 BSP for display/pins/power
  -> ESP Wi-Fi/IP networking
  -> rs-matter Matter server
  -> On/Off Light endpoint
  -> Home Assistant Matter integration
```

## Consumer application prompt: Matter over Thread / Gateway H2

Use this later once the Gateway H2 protocol path is selected:

```text
Create a Rust embedded consumer application for M5Stack CoreS3 + Gateway H2 using the `core-s3` BSP crate. The app should target Home Assistant-compatible Matter-over-Thread.

Requirements:

- Use `core-s3 = { version = "0.2", features = ["esp-hal", "gateway-h2"] }`.
- Use `CoreS3::init_display` for display bring-up.
- Use `CoreS3::init_gateway_h2` for the CoreS3-to-Gateway-H2 UART transport.
- Determine and implement the concrete H2 firmware protocol:
  - OpenThread RCP/Spinel over UART, or
  - OpenThread CLI firmware, or
  - custom standalone H2 application protocol.
- Depend on the Matter/OpenThread/Spinel crates directly in the app as needed; do not put the complete server in `core-s3`.
- Join the same Thread network used by Home Assistant's Thread Border Router.
- Run a Matter server with at least one On/Off Light endpoint.
- Show status on the CoreS3 display:
  - H2 UART initialized
  - H2 firmware/protocol detected
  - Thread network state
  - Matter commissioning state
  - pairing/manual code or QR payload
  - endpoint state
- Document required Home Assistant setup:
  - Matter integration
  - Thread integration
  - Thread Border Router/radio such as ZBT-1/SkyConnect
- Validate Home Assistant commissioning and control on hardware.
```
