//! Framed CoreS3-to-Gateway-H2 transport helpers.
//!
//! This is a small host-link framing layer only. It intentionally does not
//! implement Matter, Thread, Zigbee, OpenThread CLI, or Spinel.

use heapless::Vec;

pub const SOF: u8 = 0xA5;
pub const MAX_PAYLOAD: usize = 128;

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

/// Async-friendly transport trait for application-owned UART implementations.
pub trait GatewayH2Transport {
    type Error;

    fn send<const N: usize>(&mut self, frame: &H2Frame<N>) -> Result<(), Self::Error>;
    fn poll_receive<const N: usize>(&mut self) -> Result<Option<H2Frame<N>>, Self::Error>;
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
