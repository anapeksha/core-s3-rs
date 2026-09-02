//! Framed CoreS3-to-Gateway-H2 transport helpers.
//!
//! This module provides protocol-neutral Gateway H2 framing plus traits for
//! OpenThread/Spinel integration in downstream firmware.
//!
//! The BSP does **not** implement Matter, Thread, Zigbee, OpenThread, or Spinel
//! protocol logic. Gateway H2 firmware must be explicitly selected and validated
//! by the application. If the attached ESP32-H2 runs OpenThread RCP firmware,
//! downstream code can implement [`GatewayH2SpinelTransport`] over the UART
//! using [`crate::gateway_h2::spinel`] for HDLC-lite byte-stuffing and FCS.

use heapless::Vec;

pub const SOF: u8 = 0xA5;
pub const MAX_PAYLOAD: usize = 128;

/// Default CoreS3 Gateway H2 UART baud rate.
pub const DEFAULT_GATEWAY_H2_BAUD: u32 = 115_200;
/// Conservative maximum Spinel frame payload size for application-owned buffers.
pub const DEFAULT_SPINEL_MAX_FRAME_SIZE: usize = 2048;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    Request = 0x01,
    Response = 0x02,
    Event = 0x03,
    Error = 0x7F,
}

impl FrameKind {
    const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::Request),
            0x02 => Some(Self::Response),
            0x03 => Some(Self::Event),
            0x7F => Some(Self::Error),
            _ => None,
        }
    }
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    PayloadTooLong,
    BufferTooSmall,
    InvalidStart,
    InvalidKind,
    InvalidLength,
    BadChecksum,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct H2Frame<const N: usize = MAX_PAYLOAD> {
    pub kind: FrameKind,
    pub id: u8,
    pub payload: Vec<u8, N>,
}

impl<const N: usize> H2Frame<N> {
    pub fn new(kind: FrameKind, id: u8, payload: &[u8]) -> Self {
        let mut data = Vec::new();
        for byte in payload.iter().copied().take(N) {
            let _ = data.push(byte);
        }
        Self {
            kind,
            id,
            payload: data,
        }
    }

    pub fn encode<const OUT: usize>(&self) -> Result<Vec<u8, OUT>, TransportError> {
        if self.payload.len() > u8::MAX as usize {
            return Err(TransportError::PayloadTooLong);
        }
        let needed = 5 + self.payload.len();
        if OUT < needed {
            return Err(TransportError::BufferTooSmall);
        }
        let mut out = Vec::new();
        out.push(SOF).map_err(|_| TransportError::BufferTooSmall)?;
        out.push(self.kind as u8)
            .map_err(|_| TransportError::BufferTooSmall)?;
        out.push(self.id)
            .map_err(|_| TransportError::BufferTooSmall)?;
        out.push(self.payload.len() as u8)
            .map_err(|_| TransportError::BufferTooSmall)?;
        for byte in self.payload.iter().copied() {
            out.push(byte).map_err(|_| TransportError::BufferTooSmall)?;
        }
        out.push(checksum(&out[1..]))
            .map_err(|_| TransportError::BufferTooSmall)?;
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, TransportError> {
        if bytes.len() < 5 {
            return Err(TransportError::InvalidLength);
        }
        if bytes[0] != SOF {
            return Err(TransportError::InvalidStart);
        }
        let kind = FrameKind::from_u8(bytes[1]).ok_or(TransportError::InvalidKind)?;
        let id = bytes[2];
        let len = usize::from(bytes[3]);
        if bytes.len() != len + 5 {
            return Err(TransportError::InvalidLength);
        }
        if checksum(&bytes[1..bytes.len() - 1]) != bytes[bytes.len() - 1] {
            return Err(TransportError::BadChecksum);
        }
        if len > N {
            return Err(TransportError::PayloadTooLong);
        }
        let mut payload = Vec::new();
        for byte in bytes[4..4 + len].iter().copied() {
            payload
                .push(byte)
                .map_err(|_| TransportError::PayloadTooLong)?;
        }
        Ok(Self { kind, id, payload })
    }
}

/// Gateway H2 firmware mode assumed by downstream OpenThread integration.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GatewayH2FirmwareMode {
    /// ESP32-H2 runs OpenThread Radio Co-Processor firmware and exposes Spinel.
    OpenThreadRcp,
    /// ESP32-H2 runs OpenThread Network Co-Processor firmware.
    OpenThreadNcp,
    /// ESP32-H2 runs a vendor/custom Thread coprocessor protocol.
    CustomThreadCoprocessor,
    /// ESP32-H2 runs a vendor/custom Matter bridge protocol.
    CustomMatterBridge,
    /// Firmware mode has not been identified by the application.
    Unknown,
}

/// Static Gateway H2 OpenThread transport metadata.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GatewayH2OpenThreadConfig {
    /// UART baud rate used by the host link.
    pub baud: u32,
    /// Maximum frame size downstream buffers should reserve.
    pub max_frame_size: usize,
    /// H2 firmware mode expected by the application.
    pub firmware_mode: GatewayH2FirmwareMode,
    /// Whether frames are expected to use Spinel HDLC-lite escaping/framing.
    pub hdlc_lite: bool,
    /// Whether CRC/FCS is provided by the host framing layer.
    pub has_crc: bool,
}

impl GatewayH2OpenThreadConfig {
    /// Conservative defaults for an ESP32-H2 running OpenThread RCP firmware.
    pub const OPENTHREAD_RCP: Self = Self {
        baud: DEFAULT_GATEWAY_H2_BAUD,
        max_frame_size: DEFAULT_SPINEL_MAX_FRAME_SIZE,
        firmware_mode: GatewayH2FirmwareMode::OpenThreadRcp,
        hdlc_lite: true,
        has_crc: true,
    };
}

/// OpenThread device role as reported by an H2 Thread controller backend.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadRole {
    Disabled,
    Detached,
    Child,
    Router,
    Leader,
    BorderRouter,
}

/// High-level Gateway H2 Thread event for custom/non-Spinel H2 firmware.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GatewayH2Event {
    Ready,
    Attached(ThreadRole),
    Detached,
    DatasetAccepted,
    DatasetRejected,
    Error,
}

/// Async-friendly transport trait for application-owned UART implementations.
pub trait GatewayH2Transport {
    type Error;

    fn send<const N: usize>(&mut self, frame: &H2Frame<N>) -> Result<(), Self::Error>;
    fn poll_receive<const N: usize>(&mut self) -> Result<Option<H2Frame<N>>, Self::Error>;
}

/// Synchronous Spinel frame transport for OpenThread RCP/NCP firmware.
///
/// For standard OpenThread RCP UART use, implementations should pass unescaped
/// Spinel payloads through [`crate::gateway_h2::spinel`] before writing bytes to
/// UART and should feed UART bytes into the same codec before returning frames.
pub trait GatewayH2SpinelTransport {
    type Error;

    /// Send one Spinel frame or encoded HDLC-lite frame, depending on implementation docs.
    fn send_spinel_frame(&mut self, frame: &[u8]) -> Result<(), Self::Error>;

    /// Poll for one received Spinel frame into `out`.
    ///
    /// Returns `Ok(None)` when no complete frame is available yet. Returns
    /// `Ok(Some(len))` when `out[..len]` contains one complete frame.
    fn poll_spinel_frame(&mut self, out: &mut [u8]) -> Result<Option<usize>, Self::Error>;
}

/// Async Spinel frame transport for OpenThread RCP/NCP firmware.
pub trait AsyncGatewayH2SpinelTransport {
    type Error;

    /// Send one Spinel frame or encoded HDLC-lite frame, depending on implementation docs.
    fn send_spinel_frame(
        &mut self,
        frame: &[u8],
    ) -> impl core::future::Future<Output = Result<(), Self::Error>>;

    /// Receive one complete Spinel frame into `out`, applying implementation-defined timeout policy.
    fn receive_spinel_frame(
        &mut self,
        out: &mut [u8],
    ) -> impl core::future::Future<Output = Result<usize, Self::Error>>;
}

/// Higher-level Thread controller trait for H2 firmware that does not expose Spinel.
pub trait GatewayH2ThreadController {
    type Error;

    fn is_ready(&mut self) -> Result<bool, Self::Error>;
    fn set_active_dataset(&mut self, dataset_tlv: &[u8]) -> Result<(), Self::Error>;
    fn attach(&mut self) -> Result<(), Self::Error>;
    fn detach(&mut self) -> Result<(), Self::Error>;
    fn role(&mut self) -> Result<ThreadRole, Self::Error>;
    fn poll_event(&mut self, out: &mut [u8]) -> Result<Option<GatewayH2Event>, Self::Error>;
}

pub const fn checksum(bytes: &[u8]) -> u8 {
    let mut sum = 0u8;
    let mut index = 0;
    while index < bytes.len() {
        sum = sum.wrapping_add(bytes[index]);
        index += 1;
    }
    !sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_frame() {
        let frame = H2Frame::<16>::new(FrameKind::Request, 7, b"state");
        let bytes = frame.encode::<16>().unwrap();
        let decoded = H2Frame::<16>::decode(&bytes).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn rejects_bad_checksum() {
        let frame = H2Frame::<16>::new(FrameKind::Event, 1, b"x");
        let mut bytes = frame.encode::<16>().unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0x01;
        assert_eq!(
            H2Frame::<16>::decode(&bytes),
            Err(TransportError::BadChecksum)
        );
    }
}
