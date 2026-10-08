# core-s3 v0.5.2

## Summary

This PR prepares `core-s3` v0.5.2 with a statically allocated, continuously buffered asynchronous Gateway H2 UART byte stream suitable for `openthread 0.4.0`'s `openthread::spinel::UartSpinelTransport`.

The ESP32-S3 baseline moves from `esp-hal = "=1.1.2"` to `esp-hal = "=1.2.2"`. The BSP continues to own UART1, TX GPIO1, and RX GPIO2. Applications provide static buffer storage, move the returned byte stream into their protocol task, and independently spawn the returned UART pump. No OpenThread, Matter, network, dataset, commissioning, allocator, or application-policy dependency was added.

## Changes

### Buffered asynchronous Gateway H2 UART

- Added `CoreS3GatewayH2BufferedUartResources<RX, TX>` for caller-owned static RX/TX pipes and transport status.
- Added `CoreS3GatewayH2AsyncUart`, which directly implements `embedded_io_async 0.7::Read + Write`.
- Added `CoreS3GatewayH2UartPump::run()`, a non-terminating future that exclusively owns the async HAL UART and concurrently services its split RX/TX halves.
- UART RX remains active independently of OpenThread command timing while the pump is scheduled.
- TX handles partial HAL writes and `flush()` waits for the pump to complete a physical UART flush.
- Empty reads/writes follow `embedded-io-async` contracts.
- Added typed `GatewayH2UartError` values for FIFO overflow, glitch, frame-format, parity, and TX failures.
- Added `GatewayH2UartStatus` counters for RX/TX bytes, RX/TX errors, and RX FIFO overflows.
- Errors are latched and surfaced through the byte stream rather than silently discarded.

### Bounded static resources

- Added `GATEWAY_H2_MAX_SPINEL_FRAME_SIZE` (`2048`).
- Added the worst-case HDLC-lite encoded-frame bound.
- Minimum RX capacity stores two worst-case encoded frames.
- Minimum TX capacity stores one worst-case encoded frame.
- Initialization rejects undersized buffers, zero baud, zero maximum frame size, and frame bounds above the supported maximum.
- All long-lived storage is supplied statically by the caller; the BSP remains `no_std`, allocator-free, and executor-independent.

### API migration

The lower-level blocking API remains available:

```rust
CoreS3::init_gateway_h2(...)
```

The OpenThread helper now requires static resources and returns both endpoint and pump:

```rust
let parts = CoreS3::init_gateway_h2_openthread(
    resources,
    H2_BUFFERS.init(CoreS3GatewayH2BufferedUartResources::new()),
    GatewayH2OpenThreadConfig::default(),
)?;

// Spawn parts.pump.run() independently.
// Move parts.transport into UartSpinelTransport / the protocol task.
```

This signature change is required because the v0.5.1 blocking UART did not implement the async I/O traits required by `openthread 0.4.0`, and merely converting it to unbuffered async mode did not keep unsolicited RX drained.

### ESP stack migration

- Upgraded to `esp-hal = "=1.2.2"`.
- Migrated the live camera example to `dma_rx_buffer!` and the new aligned DMA buffer API.
- Migrated audio examples to `TdmConfig`, `dma_tx_buffer!`, and ownership-based I2S transfers.
- Preserved shared LCD/TF-card SPI2 ownership and GPIO35 handoff code.

### Documentation and example

- Updated `examples/gateway_h2_transport` to statically allocate the new resources and compile-check the returned stream against a generic `embedded_io_async::Read + Write` consumer.
- Added `examples/gateway_h2_openthread`, which depends on `openthread = "=0.4.0"`, runs the BSP UART pump in an independent executor task, and constructs the real `openthread::spinel::UartSpinelTransport` with static resources.
- Enabled OpenThread's `use-gcc` feature so `openthread-sys` builds its C core with the installed Xtensa GCC rather than unsupported host Clang.
- Documented that the pump must be spawned independently in production.
- Documented the GPIO2 conflict between Gateway H2 RX and camera XCLK.
- Clarified that custom `H2Frame` framing is not stock OpenThread Spinel framing.
- Corrected hardware documentation that previously implied an `rs-matter` re-export.

## Software validation

Completed:

```sh
cargo +stable fmt --all -- --check
cargo +stable test -p core-s3 --all-features --target aarch64-apple-darwin
cargo +esp check --workspace --all-features --release --target xtensa-esp32s3-none-elf
cargo +esp clippy --workspace --all-features --release --target xtensa-esp32s3-none-elf -- -D warnings
cargo +esp build -p gateway_h2_openthread --release --target xtensa-esp32s3-none-elf
```

Host result: 65 tests passed.

Additional publish-readiness commands and package inspection are recorded in `RELEASE.md` after their final run.

## Hardware validation status

Gateway H2 RCP validation has **not yet been run** for v0.5.2. The following remain required before claiming hardware completion:

1. Confirm stock H2 `ot-rcp` UART settings agree with GPIO1/GPIO2, 115200 baud, 8-N-1, no flow control.
2. Obtain a real Spinel protocol/version response.
3. Repeat reset and reconnect cycles.
4. Sustain unsolicited property traffic while the application command task is idle; verify no `RxFailed` or overflow.
5. Verify real radio MAC capabilities.
6. Inspect byte/error/overflow counters over an extended bounded run.
7. Re-run `sd_block_probe` and `display_sd_coexist` on real CoreS3 hardware after the HAL upgrade.

No H2, SD, LCD, or camera hardware result is inferred from compilation.
