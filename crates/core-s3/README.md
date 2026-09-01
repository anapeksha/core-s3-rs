# core-s3

`core-s3` is a `#![no_std]` board support crate for the **M5Stack CoreS3 K128** ESP32-S3 kit.

It provides:

- board metadata and pin/device maps for CoreS3 peripherals
- display constants and a dirty-region sprite framebuffer for efficient partial repainting
- power/battery status types ready for AXP2101 integration
- feature-gated Zigbee Gateway H2 metadata behind `gateway-h2`

See the repository README for workspace examples, flashing, CI, and firmware release details.
