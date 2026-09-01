#![no_std]

//! Typed board metadata and small reusable drivers for the M5Stack CoreS3.
//!
//! The crate intentionally keeps most APIs HAL-agnostic. Examples can bind these
//! constants and helper types to `esp-hal`, while tests and UI code can exercise
//! display logic on the host.

pub mod audio;
pub mod aw9523b;
pub mod board;
#[cfg(all(feature = "esp-hal", target_arch = "xtensa"))]
pub mod bsp;
pub mod devices;
pub mod display;
pub mod motion;
pub mod pins;
pub mod power;
pub mod rtc;
pub mod sd;
pub mod touch;
pub mod ui;

#[cfg(feature = "gateway-h2")]
pub mod gateway_h2;

pub use board::{Board, CoreS3};

#[cfg(test)]
extern crate std;
