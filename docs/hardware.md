# M5Stack CoreS3 hardware notes

Source documents supplied during BSP creation:

- M5Stack CoreS3 v1.0 schematic
- DIN Base v1.1 schematic
- ESP32-S3 technical reference manual
- LTR-553ALS-WA ambient/proximity sensor datasheet
- GC0308 camera datasheet
- ES7210 microphone ADC datasheet
- BMM150 magnetometer datasheet
- BMI270 IMU datasheet
- BM8563 RTC datasheet
- AXP2101 PMU datasheet
- AW88298 amplifier datasheet
- AW9523B GPIO expander datasheet

## CPU cores

ESP32-S3 is a dual-core Xtensa LX7 MCU. CoreS3 firmware starts on the PRO CPU; the APP CPU is available through `esp_hal::system::CpuControl`. Use `examples/dual_core` as the BSP reference for starting the second core with an explicit stack.

## Internal buses/devices

| Device        | Function          | Bus/address       |
| ------------- | ----------------- | ----------------- |
| AXP2101       | PMU/charger       | I²C `0x34`        |
| BM8563        | RTC               | I²C `0x51`        |
| BMI270        | 6-axis IMU        | I²C `0x68`        |
| BMM150        | Magnetometer      | I²C `0x10`        |
| LTR-553ALS-WA | Ambient/proximity | I²C `0x23`        |
| AW9523B       | GPIO expander     | I²C `0x58`        |
| GC0308        | Camera            | DVP + control bus |
| ES7210        | Microphone ADC    | I²S + control bus |
| AW88298       | Speaker amplifier | I²S + control bus |

## Validation checklist

- Confirm LCD controller init sequence and color order on hardware.
- Confirm backlight/reset control path through AXP2101/AW9523B.
- Probe I²C addresses with a scanner example before enabling high-level drivers.
- Validate Gateway H2 Grove UART wiring and optional reset/boot pins for the exact stack/base revision.
