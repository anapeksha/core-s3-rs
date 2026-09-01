//! Bosch BMI270 configuration file used by the CoreS3 onboard IMU.
//!
//! BMI270 does not produce valid accel/gyro data until this vendor-provided
//! configuration payload is uploaded after reset. The bytes are sourced from the
//! Bosch BMI270 configuration data distributed by M5Unified for CoreS3 and are
//! embedded in the crate so applications do not need to carry an external asset.

/// Size of the BMI270 configuration payload in bytes.
pub const BMI270_CONFIG_LEN: usize = 8192;

/// Bytes aligned for targets that prefer naturally aligned flash/ROM reads.
#[repr(C, align(4))]
pub struct AlignedBytes<const N: usize>(pub [u8; N]);

/// The baked-in Bosch BMI270 configuration file.
pub static BMI270_CONFIG: AlignedBytes<BMI270_CONFIG_LEN> =
    AlignedBytes(*include_bytes!("bmi270_config.bin"));

/// Returns the baked-in BMI270 configuration bytes.
pub fn config_file() -> &'static [u8] {
    &BMI270_CONFIG.0
}
