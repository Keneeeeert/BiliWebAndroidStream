//! Firefox Native Messaging framing.
//!
//! Native Messaging transports one UTF-8 JSON object per message.  The object
//! is prefixed by a four-byte unsigned little-endian length.  The framing
//! layer deliberately knows nothing about the application protocol.

use std::io::{Read, Write};

use crate::error::HelperError;

/// Maximum accepted inbound/outbound JSON payload.
///
/// This protects the long-lived helper from an extension accidentally (or
/// maliciously) asking it to allocate an unbounded buffer.
pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

pub fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Vec<u8>>, HelperError> {
    let mut length_bytes = [0_u8; 4];
    match reader.read(&mut length_bytes[..1])? {
        0 => return Ok(None),
        1 => {}
        _ => unreachable!("a one-byte read cannot return more than one byte"),
    }
    let prefix_received = read_until_eof(reader, &mut length_bytes[1..])?;
    if prefix_received != 3 {
        return Err(HelperError::TruncatedFrame {
            expected: 4,
            received: prefix_received + 1,
        });
    }

    let length = u32::from_le_bytes(length_bytes) as usize;
    if length > MAX_MESSAGE_BYTES {
        return Err(HelperError::FrameTooLarge {
            actual: length,
            limit: MAX_MESSAGE_BYTES,
        });
    }

    let mut payload = vec![0_u8; length];
    let received = read_until_eof(reader, &mut payload)?;
    if received != length {
        return Err(HelperError::TruncatedFrame {
            expected: length,
            received,
        });
    }
    Ok(Some(payload))
}

fn read_until_eof<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<usize, HelperError> {
    let mut offset = 0;
    while offset < buffer.len() {
        match reader.read(&mut buffer[offset..]) {
            Ok(0) => break,
            Ok(read) => offset += read,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(offset)
}

pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> Result<(), HelperError> {
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(HelperError::FrameTooLarge {
            actual: payload.len(),
            limit: MAX_MESSAGE_BYTES,
        });
    }

    let length = u32::try_from(payload.len()).map_err(|_| HelperError::FrameTooLarge {
        actual: payload.len(),
        limit: u32::MAX as usize,
    })?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_frame() {
        let payload = br#"{"type":"ping"}"#;
        let mut encoded = Vec::new();
        write_frame(&mut encoded, payload).unwrap();
        assert_eq!(&encoded[..4], &(payload.len() as u32).to_le_bytes());

        let decoded = read_frame(&mut encoded.as_slice()).unwrap();
        assert_eq!(decoded.as_deref(), Some(payload.as_slice()));
    }

    #[test]
    fn clean_eof_returns_none() {
        let mut input = &[][..];
        assert!(read_frame(&mut input).unwrap().is_none());
    }

    #[test]
    fn rejects_oversized_frame_before_allocating() {
        let length = (MAX_MESSAGE_BYTES as u32 + 1).to_le_bytes();
        let mut input = length.as_slice();
        assert!(matches!(
            read_frame(&mut input),
            Err(HelperError::FrameTooLarge { .. })
        ));
    }
}
