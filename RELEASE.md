# core-s3 v0.5.2

`core-s3` v0.5.2 upgrades the ESP32-S3 BSP baseline to `esp-hal = "=1.2.2"` and adds a reusable, statically allocated buffered async UART stream for a CoreS3 host communicating with the M5Stack Gateway H2 running stock OpenThread RCP firmware.

## Why this release exists

In v0.5.1, `CoreS3::init_gateway_h2_openthread()` returned `esp_hal::uart::Uart<'static, Blocking>`. That type does not implement the `embedded_io_async 0.7` traits required by `openthread 0.4.0`'s `UartSpinelTransport`.

Changing only the mode marker to `Async` was not reliable: an unbuffered UART is drained only while a read future is actively polled. A downstream trial reached OpenThread radio initialization but failed with `RxFailed`. Stock RCP firmware can emit unsolicited Spinel property traffic, so the board transport needs an independently scheduled RX drain and bounded storage between protocol reads.

## New transport architecture

The v0.5.2 OpenThread helper returns two ownership-separated objects:

- `CoreS3GatewayH2AsyncUart`: the async byte stream moved into the OpenThread/protocol task;
- `CoreS3GatewayH2UartPump`: the exclusive HAL UART owner, spawned and continuously polled in an independent task.

The pump splits the HAL UART and services RX and TX concurrently. UART RX is copied into a static bounded pipe as bytes arrive. Application TX enters another static pipe; the pump handles partial HAL writes and physically flushes each dequeued batch.

This design:

- keeps UART1/GPIO1/GPIO2 ownership inside the BSP;
- does not alias UART registers or pins;
- does not require heap allocation;
- does not place long-lived buffers on a task stack;
- does not force an Embassy executor on all users;
- exposes a future that can be spawned by the consumer's chosen executor/RTOS integration;
- adds no `openthread`, Matter, Wi-Fi, dataset, commissioning, or storage dependency.

## Public API

Key types/constants:

```rust
CoreS3GatewayH2BufferedUartResources<RX, TX>
CoreS3GatewayH2AsyncUart<'a, RX, TX>
CoreS3GatewayH2UartPump<'a, RX, TX>
CoreS3GatewayH2OpenThreadParts<T, P>
GatewayH2UartError
GatewayH2UartStatus
GATEWAY_H2_OPENTHREAD_BAUD
GATEWAY_H2_MAX_SPINEL_FRAME_SIZE
GATEWAY_H2_MAX_ENCODED_SPINEL_FRAME_SIZE
GATEWAY_H2_MIN_RX_BUFFER_SIZE
GATEWAY_H2_MIN_TX_BUFFER_SIZE
```

The byte stream implements:

```rust
embedded_io_async::Read
embedded_io_async::Write
```

using `embedded-io-async 0.7`, so it can be passed to a compatible Spinel UART consumer without a downstream trait-version adapter.

## Static buffer sizing

The decoded frame bound is 2048 bytes. The encoded bound accounts conservatively for FCS, HDLC escaping of every byte, and frame flags.

- Minimum RX: two maximum encoded frames (`8204` bytes).
- Minimum TX: one maximum encoded frame (`4102` bytes).

Larger capacities are allowed. Initialization rejects smaller capacities. Bounded storage still requires the pump and protocol consumer to be scheduled appropriately; if the consumer stops draining longer than the configured capacity permits, the hardware can overflow. FIFO overflow is counted, latched, and surfaced as a typed error instead of being silently ignored.

## UART settings and ownership

Default stock-RCP transport settings:

```text
UART1
CoreS3 TX GPIO1 -> Gateway H2 RX
CoreS3 RX GPIO2 <- Gateway H2 TX
115200 baud
8 data bits, no parity, 1 stop bit
No RTS/CTS hardware flow control
```

GPIO2 is also CoreS3 camera XCLK. Camera and Gateway H2 UART resources cannot be initialized simultaneously. This conflict is documented rather than hidden or bypassed.

## Migration from v0.5.1

The blocking diagnostic/custom-protocol helper is preserved:

```rust
let parts = CoreS3::init_gateway_h2(resources)?;
```

The OpenThread helper has a correctness-required signature change:

```rust
static H2_BUFFERS: StaticCell<CoreS3GatewayH2BufferedUartResources<
    GATEWAY_H2_MIN_RX_BUFFER_SIZE,
    GATEWAY_H2_MIN_TX_BUFFER_SIZE,
>> = StaticCell::new();

let parts = CoreS3::init_gateway_h2_openthread(
    resources,
    H2_BUFFERS.init(CoreS3GatewayH2BufferedUartResources::new()),
    GatewayH2OpenThreadConfig::default(),
)?;

// Spawn and continuously poll this in its own task:
// parts.pump.run().await

// Move parts.transport into the task that owns UartSpinelTransport.
```

Not polling `pump.run()` removes the continuous-RX guarantee and is an integration error.

## ESP-HAL 1.2.2 migration

The workspace now uses `esp-hal = "=1.2.2"`. Existing board APIs remain available. Example-only migrations include:

- camera DMA storage via `dma_rx_buffer!` and aligned `DmaRxBuf` values;
- I2S `TdmConfig`, `dma_tx_buffer!`, and ownership-based transfer APIs.

The shared LCD/TF-card implementation still owns SPI2, chip selects, and GPIO35 switching. No application-visible GPIO matrix handling was introduced.

## Software validation

Completed successfully:

```sh
cargo +stable fmt --all -- --check
cargo +stable test -p core-s3 --all-features --target aarch64-apple-darwin
cargo +esp check --workspace --all-features --release --target xtensa-esp32s3-none-elf
cargo +esp clippy --workspace --all-features --release --target xtensa-esp32s3-none-elf -- -D warnings
```

Host tests: 65 passed.

The final release-preparation run must also record:

```sh
cargo +stable check -p core-s3 --all-targets
cargo +stable clippy -p core-s3 --all-targets -- -D warnings
cargo +stable check -p core-s3 --no-default-features
cargo +stable check -p core-s3 --features defmt
cargo +esp build -p gateway_h2_transport --release --target xtensa-esp32s3-none-elf
cargo +esp build -p gateway_h2_openthread --release --target xtensa-esp32s3-none-elf
cargo +esp build -p display_sd_coexist --release --target xtensa-esp32s3-none-elf
cargo +stable package -p core-s3 --allow-dirty
```

## Hardware validation matrix

No v0.5.2 Gateway H2 RCP hardware validation has been run yet. Do not treat the following as passed until observed on real hardware.

| #   | Action                                                         | Expected                                                       | Observed | Status  |
| --- | -------------------------------------------------------------- | -------------------------------------------------------------- | -------- | ------- |
| 1   | Verify stock RCP wiring/settings                               | GPIO1/GPIO2, 115200, 8-N-1, no flow control agree on both ends | Not run  | Pending |
| 2   | Power-cycle CoreS3 and H2; request Spinel protocol/version     | Valid response, no `RxFailed`                                  | Not run  | Pending |
| 3   | Repeat reset/reconnect cycles                                  | Transport and RCP recover consistently                         | Not run  | Pending |
| 4   | Leave command task idle during unsolicited RX/property traffic | Pump continues draining; no UART FIFO overflow                 | Not run  | Pending |
| 5   | Initialize OpenThread radio                                    | Real MAC capabilities reported                                 | Not run  | Pending |
| 6   | Extended bounded traffic run                                   | RX/TX counters advance; error/overflow counters remain zero    | Not run  | Pending |
| 7   | Run SD-before-LCD probe after HAL upgrade                      | Pre-inserted card probes successfully                          | Not run  | Pending |
| 8   | Run alternating LCD/SD coexistence after HAL upgrade           | Display stable; SD read/write/readback succeeds                | Not run  | Pending |

Recommended commands for the board regressions:

```sh
cargo +esp run -p sd_block_probe --release --target xtensa-esp32s3-none-elf
cargo +esp run -p display_sd_coexist --release --target xtensa-esp32s3-none-elf
```

## Release status

The source and release metadata are prepared, but publishing and hardware-validation claims are intentionally withheld. Create/push a release tag or publish to crates.io only after explicit user approval and the required hardware results are recorded.
