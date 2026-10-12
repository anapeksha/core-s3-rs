# fix(gateway-h2): correct UART route and RCP startup

## Summary

This PR prepares `core-s3 v0.5.5` as a focused correction for the stacked M5Stack CoreS3 + Module Gateway H2 v0.4 UART/OpenThread path.

It corrects the hardware-validated stacked UART route to CoreS3 TX GPIO17 and RX GPIO10, changes the OpenThread RCP default to 460800 baud for the ESP-IDF v5.5.2 `examples/openthread/ot_rcp` image, and upgrades the public OpenThread example from constructor-only compatibility to a bounded real Spinel startup and capability-discovery handshake.

The BSP remains `no_std`, allocator-free, protocol-neutral, and UART-only for Gateway H2 communication. It does not add Thread dataset, attachment, Matter, Wi-Fi, or application policy.

## Changes

### Hardware-validated stacked UART route

The Gateway H2 v0.4 resources and metadata now use:

```text
CoreS3 TX GPIO17 -> H2 RXD
CoreS3 RX GPIO10 <- H2 TXD
CoreS3 GPIO7     -> H2 EN (active-low reset)
CoreS3 GPIO18    -> H2 GPIO9/BOOT
```

All BSP resource declarations, examples, host assertions, and current hardware documentation were updated consistently. UART mode owns only UART1 and these four pins; optional H2 SPI/PTA routes remain unconsumed.

### OpenThread RCP configuration

- `GATEWAY_H2_OPENTHREAD_BAUD` is now `460_800`.
- `GatewayH2OpenThreadConfig::default()` and `OPENTHREAD_RCP` use that OpenThread-specific baud.
- The generic lower-level 115200-baud Gateway H2 default remains unchanged for compatibility.
- Module Gateway H2 v0.4 UART RCP operation requires all eight S1 switches OFF; S1-4 is unused.
- The documented firmware is ESP-IDF v5.5.2 `examples/openthread/ot_rcp`.

### Real bounded Spinel startup example

`examples/gateway_h2_openthread` now:

- statically allocates the BSP RX/TX resources, OpenThread UART resources, and Spinel radio resources;
- independently spawns the continuously draining BSP UART pump;
- constructs `openthread 0.4.0`'s `UartSpinelTransport`;
- constructs `SpinelRadio` and calls `Radio::init().await`;
- performs software reset, Spinel protocol-version discovery, required capability checks, EUI-64 discovery, and radio capability discovery;
- emits one bounded PASS or FAIL result rather than running an unbounded retry loop.

An example-local Embassy time driver uses `esp_hal::time::Instant`. It is not part of the BSP API and does not impose an executor or time-driver policy on downstream users.

### Scope and SPI status

Gateway H2 SPI remains unsupported. M5Stack documents optional SPI signal names, but the available authoritative product information does not establish the OpenThread SPI interrupt/handshake route, complete DIP semantics, or a compatible H2 firmware/protocol. The stock UART `ot_rcp` image is not assumed to support host SPI. Implementing SPI would additionally require integration with the hardware-proven LCD/TF shared-SPI2 arbitration.

This release does not claim Thread dataset attachment, routed IPv6, SRP/DNS-SD, Matter, or Matter-over-Thread validation.

## Validation

Passed locally:

```sh
cargo +stable fmt --all -- --check
cargo +stable test -p core-s3 --all-features --target aarch64-apple-darwin
RUSTDOCFLAGS='-D warnings' cargo +stable doc -p core-s3 --all-features --no-deps
cargo +esp check --workspace --all-features --release --target xtensa-esp32s3-none-elf
cargo +esp clippy --workspace --all-features --release --target xtensa-esp32s3-none-elf -- -D warnings
cargo +esp build -p gateway_h2_openthread --release --target xtensa-esp32s3-none-elf
cargo +stable package -p core-s3 --allow-dirty
```

Host result: 68 tests passed. The ESP example build retains the existing linker warning about an RWX LOAD segment. Package contents and verification are recorded in `RELEASE.md`.

## Hardware evidence

Previously supplied real-hardware validation for a stacked CoreS3 and Module Gateway H2 v0.4 established:

- S3-hosted H2 ROM-loader entry, flashing, and flash verification;
- bidirectional UART byte exchange at 460800 baud;
- Spinel startup;
- RCP capability discovery.

The latest v0.5.5 example was also erased, flashed, and reset successfully over `/dev/cu.usbmodem11301`, but `cargo embed` did not expose firmware logs and the subsequent monitor attempt failed to initialize its input reader. That run is therefore inconclusive for the new PASS/FAIL line and is not represented as additional protocol validation.

No Gateway H2 SPI, Thread dataset attachment, or Matter-over-Thread result is claimed.
