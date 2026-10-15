# Radio + Gateway H2 staged coexistence harness

Build from the workspace root:

```sh
rustup run esp cargo build -p radio_h2_coexistence --release --target xtensa-esp32s3-none-elf
rustup run esp cargo clippy -p radio_h2_coexistence --release --target xtensa-esp32s3-none-elf -- -D warnings
```

Compile-time configuration (all optional; builds without secrets):

- `RADIO_WIFI_SSID`: nonempty SSID.
- `RADIO_WIFI_PASSWORD`: password; explicitly empty selects an open AP.
- `RADIO_ECHO_IPV4`, `RADIO_ECHO_PORT`: IPv4 TCP service that echoes the exact four-byte probe.
- `RADIO_BLE_SCAN=1`: enable active Trouble scan windows. Other values or missing configuration skip scanning.

Credentials, endpoints, advertisements, Spinel capabilities, and received payloads are not printed. Build-time credentials remain embedded in the firmware artifact: treat it as sensitive.

## Stages

Safe internal heap initialization and one CoreS3Runtime primary executor; shared SPI and internal I2C/power setup; ALDO4 power cycle and SD acquisition/probe before any LCD initialization; powered-shared-SPI LCD initialization; buffered H2 UART at the BSP's default 460800 baud; radio coexistence initialization.

The primary executor polls separate H2 pump, real OpenThread Spinel capability discovery, LCD/SD observation, and Wi-Fi/BLE tasks. H2 uses static BSP and OpenThread transport buffers. Configure the stacked Gateway H2 with all S1 switches OFF and compatible UART RCP firmware (the reference example uses ESP-IDF v5.5.2 ot_rcp).

Wi-Fi/BLE work runs three iterations: association (12 s), DHCP (6 s), optional TCP echo (3 s), BLE startup (5 s) and scan observation (20 s), disconnect (2 s), under a 90 s radio-work deadline. Spinel initialization has a 15 s deadline; the pump is serviced for 95 s. Thirty LCD updates and non-destructive raw sector-zero reads are spaced two seconds apart. Deadlines require executor progress, not hardware-independent preemption.

## Explicit limitations

- This is a partial system-validation harness, not proof of hardware coexistence. No hardware validation has been performed.
- LCD/SD APIs are blocking. Finite operation counts do **not** provide hard wall-clock bounds; these calls can delay UART servicing and all async deadlines. Startup SPI/I2C/power setup is also blocking. Some critical startup failures panic rather than continue testing dependent layers.
- Raw SD write/readback/restore is deliberately NOT RUN. Reads are non-destructive; no write throughput or restoration claim is made.
- Missing Wi-Fi credentials skip the shared radio test, including BLE, but do not skip H2 or LCD/SD. BLE-only validation without Wi-Fi configuration is not implemented. Missing/invalid endpoint configuration skips TCP echo. Zero advertisements is INCONCLUSIVE, not a BLE failure.
- Spinel PASS means actual RCP capability discovery completed, not Thread attachment or traffic. Thread networking is NOT RUN. Pump lifetime is not itself a health test.
- LCD PASS means a driver transfer completed; visually inspect the alternating cyan/black square. There is no framebuffer readback.
- Individual results are observations, not an aggregate PASS. Tasks stop after their observation windows; executor idle afterward is expected. No automatic reset/recovery or long-duration soak is provided.
