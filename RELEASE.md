# core-s3 v0.5.5

`core-s3 v0.5.5` corrects the stacked Module Gateway H2 v0.4 UART route and OpenThread RCP defaults, and upgrades the no-heap OpenThread example to perform a real bounded Spinel startup and capability-discovery handshake.

## Gateway H2 UART correction

The hardware-validated stacked route is:

```text
CoreS3 TX GPIO17 -> H2 RXD
CoreS3 RX GPIO10 <- H2 TXD
CoreS3 GPIO7     -> H2 EN (active-low reset)
CoreS3 GPIO18    -> H2 GPIO9/BOOT
```

This replaces the incorrect GPIO10-TX/GPIO17-RX assignment in v0.5.3/v0.5.4. The corrected types make incorrect construction fail at compile time when applications use `CoreS3GatewayH2Resources`.

For UART-only OpenThread RCP operation on Module Gateway H2 v0.4:

- set all eight S1 switches OFF;
- S1-4 is unused;
- use 8-N-1 without hardware flow control;
- use 460800 baud with the ESP-IDF v5.5.2 `examples/openthread/ot_rcp` image.

`GATEWAY_H2_OPENTHREAD_BAUD`, `GatewayH2OpenThreadConfig::OPENTHREAD_RCP`, and `GatewayH2OpenThreadConfig::default()` now select 460800 baud. The generic lower-level 115200-baud constant remains unchanged for custom/diagnostic compatibility.

## Buffered async transport

The v0.5.2 buffered transport remains the recommended OpenThread path:

- caller-owned static RX/TX storage;
- direct `embedded_io_async 0.7::Read + Write` compatibility;
- an independently scheduled UART pump that continuously drains RX;
- typed UART failures and bounded monotonic diagnostics;
- safe EN/BOOT reset and ROM-loader control;
- no heap, hidden executor, OpenThread dependency, or application policy in the BSP.

Applications using the typed resources must update their pin construction:

```rust
CoreS3GatewayH2Resources {
    uart1: peripherals.UART1,
    tx: peripherals.GPIO17,
    rx: peripherals.GPIO10,
    enable: peripherals.GPIO7,
    boot: peripherals.GPIO18,
}
```

## OpenThread example

`examples/gateway_h2_openthread` now goes beyond constructing `UartSpinelTransport`. It statically allocates all long-lived BSP/OpenThread resources, spawns the BSP UART pump independently, constructs `SpinelRadio`, and calls `Radio::init().await`.

The bounded initialization performs the OpenThread crate's startup sequence, including Spinel reset, protocol-version query, required RCP capability checks, EUI-64 discovery, and radio capability discovery. It prints one of:

```text
Gateway H2 Spinel startup PASS: ...
Gateway H2 Spinel startup FAIL: ...
```

The example includes a local `esp_hal::time::Instant`-backed Embassy time driver for its platform-spin executor. This remains example policy and is not imposed by `core-s3`.

## Gateway H2 SPI status

Gateway H2 SPI is not supported in v0.5.5.

M5Stack's product documentation names optional `SPI_MOSI`, `SPI_MISO`, `SPI_CLK`, and `SPI_CS` routes, but the available authoritative material does not establish the OpenThread SPI interrupt/handshake route, complete DIP-switch semantics for a usable host protocol, or a compatible SPI-capable H2 firmware image. ESP-IDF's UART `ot_rcp` image is not assumed to support SPI.

Safe SPI support would also need to join the BSP-owned LCD/TF shared-SPI2 arbitration and preserve GPIO35 D/C-to-MISO restoration. No raw or unvalidated SPI escape hatch was added.

## Validation

The following completed successfully:

```sh
cargo +stable fmt --all -- --check
cargo +stable test -p core-s3 --all-features --target aarch64-apple-darwin
RUSTDOCFLAGS='-D warnings' cargo +stable doc -p core-s3 --all-features --no-deps
cargo +esp check --workspace --all-features --release --target xtensa-esp32s3-none-elf
cargo +esp clippy --workspace --all-features --release --target xtensa-esp32s3-none-elf -- -D warnings
cargo +esp build -p gateway_h2_openthread --release --target xtensa-esp32s3-none-elf
cargo +stable package -p core-s3 --allow-dirty
```

Host result: 68 tests passed. The OpenThread example linked for `xtensa-esp32s3-none-elf`; the existing linker warning about an RWX LOAD segment remains. Packaging produced 27 files (320.1 KiB uncompressed, 78.0 KiB compressed), verification succeeded, and the archive contents were inspected.

## Hardware status

Previously supplied validation on a stacked M5Stack CoreS3 and Module Gateway H2 v0.4 established S3-hosted H2 ROM-loader entry, flashing and verification, bidirectional UART exchange at 460800 baud, Spinel startup, and RCP capability discovery using the hardware baseline documented above.

For this release candidate, the updated example was erased, programmed, and reset successfully over `/dev/cu.usbmodem11301`. `cargo embed` did not expose the firmware log, and a follow-up `espflash monitor` attempt failed with `Failed to initialize input reader`; therefore that particular run is inconclusive for the new PASS/FAIL output and is not counted as new protocol evidence.

This release does not claim Gateway H2 SPI operation, Thread dataset attachment, routed Thread IPv6, SRP/DNS-SD, Matter traffic, or Matter-over-Thread.

## Publication

The `v0.5.5` tag triggers the crate-only trusted-publishing workflow, which runs:

```sh
cargo +stable publish -p core-s3
```

No firmware artifacts are built or attached by the release workflow.
