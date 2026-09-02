//! OpenThread Spinel HDLC-lite framing helpers for Gateway H2 RCP firmware.
//!
//! M5Stack's Gateway H2 Thread Border Router flow builds ESP-IDF's
//! `examples/openthread/ot_rcp` firmware for the ESP32-H2 module and runs the
//! border-router host on the ESP32-S3 side. That RCP link uses OpenThread's
//! Spinel UART HDLC-lite framing. This module provides the bounded, no-heap
//! byte-stuffing and FCS layer needed by downstream firmware; it does not
//! implement the OpenThread protocol state machine itself.

/// HDLC-lite frame delimiter used by Spinel over UART.
pub const FLAG: u8 = 0x7e;
/// HDLC-lite escape byte.
pub const ESCAPE: u8 = 0x7d;
/// HDLC-lite escape transform.
pub const ESCAPE_XOR: u8 = 0x20;
/// XON byte escaped by OpenThread HDLC-lite.
pub const XON: u8 = 0x11;
/// XOFF byte escaped by OpenThread HDLC-lite.
pub const XOFF: u8 = 0x13;
/// Vendor-reserved byte escaped by OpenThread HDLC-lite.
pub const SPECIAL_ESCAPE: u8 = 0xf8;

const FCS_INIT: u16 = 0xffff;
const FCS_GOOD: u16 = 0xf0b8;
const FCS_POLY: u16 = 0x8408;

/// Errors returned by Spinel HDLC-lite framing helpers.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinelHdlcError {
    /// Caller-provided output buffer is too small for the encoded/decoded frame.
    BufferTooSmall,
    /// The received frame exceeds the decoder's bounded internal buffer.
    FrameTooLong,
    /// A delimiter arrived after an escape byte without an escaped payload byte.
    UnexpectedEscape,
    /// A received frame was shorter than the mandatory 16-bit FCS.
    FrameTooShort,
    /// The received frame's FCS did not validate.
    BadFcs,
}

/// Encode one raw Spinel frame into HDLC-lite UART bytes.
///
/// The returned frame includes both leading and trailing [`FLAG`] delimiters and
/// appends the OpenThread/PPP-style 16-bit FCS in little-endian order before
/// byte-stuffing. The input should be the unescaped Spinel payload bytes.
pub fn encode_frame(frame: &[u8], out: &mut [u8]) -> Result<usize, SpinelHdlcError> {
    let mut written = 0;
    push_raw(FLAG, out, &mut written)?;

    let mut fcs = FCS_INIT;
    for byte in frame.iter().copied() {
        fcs = update_fcs(fcs, byte);
        push_escaped(byte, out, &mut written)?;
    }

    let fcs = !fcs;
    push_escaped((fcs & 0x00ff) as u8, out, &mut written)?;
    push_escaped((fcs >> 8) as u8, out, &mut written)?;
    push_raw(FLAG, out, &mut written)?;
    Ok(written)
}

/// Bounded incremental Spinel HDLC-lite decoder.
///
/// Feed bytes from the Gateway H2 UART into [`Self::push_byte`]. When a complete
/// frame is received and the FCS validates, `Ok(Some(len))` is returned and
/// `out[..len]` contains the unescaped Spinel payload without the FCS bytes.
pub struct SpinelHdlcDecoder<const N: usize> {
    frame: [u8; N],
    len: usize,
    escaped: bool,
    in_frame: bool,
}

impl<const N: usize> SpinelHdlcDecoder<N> {
    /// Create an empty decoder with a fixed internal receive buffer.
    pub const fn new() -> Self {
        Self {
            frame: [0; N],
            len: 0,
            escaped: false,
            in_frame: false,
        }
    }

    /// Reset all partial receive state.
    pub fn reset(&mut self) {
        self.len = 0;
        self.escaped = false;
        self.in_frame = false;
    }

    /// Push one UART byte and optionally emit one decoded Spinel frame.
    pub fn push_byte(
        &mut self,
        byte: u8,
        out: &mut [u8],
    ) -> Result<Option<usize>, SpinelHdlcError> {
        if byte == FLAG {
            return self.finish_or_start(out);
        }

        if !self.in_frame {
            return Ok(None);
        }

        if byte == ESCAPE {
            self.escaped = true;
            return Ok(None);
        }

        let decoded = if self.escaped {
            self.escaped = false;
            byte ^ ESCAPE_XOR
        } else {
            byte
        };

        if self.len == N {
            self.reset();
            return Err(SpinelHdlcError::FrameTooLong);
        }

        self.frame[self.len] = decoded;
        self.len += 1;
        Ok(None)
    }

    fn finish_or_start(&mut self, out: &mut [u8]) -> Result<Option<usize>, SpinelHdlcError> {
        if !self.in_frame || self.len == 0 {
            self.in_frame = true;
            self.escaped = false;
            self.len = 0;
            return Ok(None);
        }

        if self.escaped {
            self.reset();
            return Err(SpinelHdlcError::UnexpectedEscape);
        }

        if self.len < 2 {
            self.reset();
            return Err(SpinelHdlcError::FrameTooShort);
        }

        let mut fcs = FCS_INIT;
        let mut index = 0;
        while index < self.len {
            fcs = update_fcs(fcs, self.frame[index]);
            index += 1;
        }

        if fcs != FCS_GOOD {
            self.reset();
            return Err(SpinelHdlcError::BadFcs);
        }

        let payload_len = self.len - 2;
        if out.len() < payload_len {
            self.reset();
            return Err(SpinelHdlcError::BufferTooSmall);
        }

        out[..payload_len].copy_from_slice(&self.frame[..payload_len]);
        self.len = 0;
        self.escaped = false;
        self.in_frame = true;
        Ok(Some(payload_len))
    }
}

impl<const N: usize> Default for SpinelHdlcDecoder<N> {
    fn default() -> Self {
        Self::new()
    }
}

const fn should_escape(byte: u8) -> bool {
    matches!(byte, FLAG | ESCAPE | XON | XOFF | SPECIAL_ESCAPE)
}

fn push_raw(byte: u8, out: &mut [u8], written: &mut usize) -> Result<(), SpinelHdlcError> {
    if *written == out.len() {
        return Err(SpinelHdlcError::BufferTooSmall);
    }
    out[*written] = byte;
    *written += 1;
    Ok(())
}

fn push_escaped(byte: u8, out: &mut [u8], written: &mut usize) -> Result<(), SpinelHdlcError> {
    if should_escape(byte) {
        push_raw(ESCAPE, out, written)?;
        push_raw(byte ^ ESCAPE_XOR, out, written)
    } else {
        push_raw(byte, out, written)
    }
}

fn update_fcs(mut fcs: u16, byte: u8) -> u16 {
    fcs ^= u16::from(byte);
    let mut bit = 0;
    while bit < 8 {
        if (fcs & 1) != 0 {
            fcs = (fcs >> 1) ^ FCS_POLY;
        } else {
            fcs >>= 1;
        }
        bit += 1;
    }
    fcs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_spinel_payload() {
        let payload = [0x80, 0x06, 0x00, 0x7e, 0x7d, 0x11, 0x13, 0xf8];
        let mut encoded = [0u8; 64];
        let len = encode_frame(&payload, &mut encoded).unwrap();

        assert_eq!(encoded[0], FLAG);
        assert_eq!(encoded[len - 1], FLAG);
        assert!(encoded[..len].windows(2).any(|pair| pair == [ESCAPE, 0x5e]));
        assert!(encoded[..len].windows(2).any(|pair| pair == [ESCAPE, 0x5d]));

        let mut decoder = SpinelHdlcDecoder::<32>::new();
        let mut decoded = [0u8; 32];
        let mut out_len = None;
        for byte in encoded[..len].iter().copied() {
            out_len = decoder.push_byte(byte, &mut decoded).unwrap().or(out_len);
        }

        assert_eq!(out_len, Some(payload.len()));
        assert_eq!(&decoded[..payload.len()], payload);
    }

    #[test]
    fn rejects_bad_fcs() {
        let payload = [0x80, 0x06, 0x00];
        let mut encoded = [0u8; 32];
        let len = encode_frame(&payload, &mut encoded).unwrap();
        encoded[len - 2] ^= 0x01;

        let mut decoder = SpinelHdlcDecoder::<16>::new();
        let mut decoded = [0u8; 16];
        let mut error = None;
        for byte in encoded[..len].iter().copied() {
            match decoder.push_byte(byte, &mut decoded) {
                Ok(_) => {}
                Err(err) => error = Some(err),
            }
        }

        assert_eq!(error, Some(SpinelHdlcError::BadFcs));
    }

    #[test]
    fn reports_output_buffer_too_small_without_truncating() {
        let payload = [1, 2, 3, 4];
        let mut encoded = [0u8; 32];
        let len = encode_frame(&payload, &mut encoded).unwrap();
        let mut decoder = SpinelHdlcDecoder::<16>::new();
        let mut decoded = [0u8; 3];
        let mut error = None;

        for byte in encoded[..len].iter().copied() {
            match decoder.push_byte(byte, &mut decoded) {
                Ok(_) => {}
                Err(err) => error = Some(err),
            }
        }

        assert_eq!(error, Some(SpinelHdlcError::BufferTooSmall));
    }

    #[test]
    fn ignores_bytes_until_first_flag() {
        let mut decoder = SpinelHdlcDecoder::<16>::new();
        let mut decoded = [0u8; 16];
        assert_eq!(decoder.push_byte(0x80, &mut decoded), Ok(None));
        assert_eq!(decoder.push_byte(FLAG, &mut decoded), Ok(None));
    }
}
