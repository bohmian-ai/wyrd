//! Incremental protobuf length-delimited frame encoding and decoding.

use prost::Message;

/// Errors produced by the bounded frame codec.
#[derive(Debug, thiserror::Error)]
pub enum FrameCodecError {
    /// A length prefix exceeds the configured payload ceiling.
    #[error("protobuf frame length {length} exceeds maximum {maximum}")]
    Oversize {
        /// Declared payload length.
        length: usize,
        /// Configured payload ceiling.
        maximum: usize,
    },
    /// A varint length prefix is malformed or exceeds `usize`.
    #[error("malformed protobuf frame length prefix")]
    MalformedLength,
    /// End-of-stream arrived with a partial prefix or payload.
    #[error("truncated protobuf frame")]
    Truncated,
    /// The bounded payload was not a valid protobuf message.
    #[error("invalid protobuf frame: {0}")]
    Decode(#[from] prost::DecodeError),
    /// Encoding the message failed.
    #[error("failed to encode protobuf frame: {0}")]
    Encode(#[from] prost::EncodeError),
}

/// Stateless encoder for standard protobuf length-delimited frames.
pub struct FrameEncoder;

impl FrameEncoder {
    /// Encodes one message with its protobuf varint length prefix.
    ///
    /// # Errors
    /// Returns [`FrameCodecError::Encode`] when prost cannot encode the message.
    pub fn encode<M: Message>(message: &M) -> Result<Vec<u8>, FrameCodecError> {
        let mut output = Vec::with_capacity(message.encoded_len() + 10);
        message.encode_length_delimited(&mut output)?;
        Ok(output)
    }
}

/// Incremental decoder for one or more bounded protobuf frames.
pub struct FrameDecoder {
    /// Maximum accepted decoded payload length.
    maximum: usize,
    /// Incrementally buffered canonical varint prefix.
    prefix: Vec<u8>,
    /// Incrementally buffered bounded protobuf payload.
    payload: Vec<u8>,
    /// Declared payload length after prefix validation.
    expected: Option<usize>,
}

impl FrameDecoder {
    /// Constructs a decoder enforcing `maximum` before payload allocation.
    #[must_use]
    pub fn new(maximum: usize) -> Self {
        Self {
            maximum,
            prefix: Vec::with_capacity(10),
            payload: Vec::new(),
            expected: None,
        }
    }

    /// Consumes an arbitrary body chunk and returns every complete message.
    ///
    /// # Errors
    /// Returns an oversize or malformed-prefix error before allocating its
    /// declared payload, or a decode error for invalid complete protobuf.
    pub fn push<M>(&mut self, chunk: &[u8]) -> Result<Vec<M>, FrameCodecError>
    where
        M: Message + Default,
    {
        let mut messages = Vec::new();
        for &byte in chunk {
            if self.expected.is_none() {
                self.prefix.push(byte);
                if self.prefix.len() > 10 {
                    return Err(FrameCodecError::MalformedLength);
                }
                if byte & 0x80 == 0 {
                    let length = decode_varint(&self.prefix)?;
                    if length > self.maximum {
                        return Err(FrameCodecError::Oversize {
                            length,
                            maximum: self.maximum,
                        });
                    }
                    self.payload = Vec::with_capacity(length);
                    self.expected = Some(length);
                    self.prefix.clear();
                    if length == 0 {
                        messages.push(M::decode(&self.payload[..])?);
                        self.expected = None;
                    }
                }
                continue;
            }
            self.payload.push(byte);
            if self.payload.len() == self.expected.expect("expected length is set") {
                messages.push(M::decode(&self.payload[..])?);
                self.payload.clear();
                self.expected = None;
            }
        }
        Ok(messages)
    }

    /// Confirms that EOF occurred exactly between frames.
    ///
    /// # Errors
    /// Returns [`FrameCodecError::Truncated`] for a partial prefix or payload.
    pub fn finish(&self) -> Result<(), FrameCodecError> {
        if self.prefix.is_empty() && self.expected.is_none() {
            Ok(())
        } else {
            Err(FrameCodecError::Truncated)
        }
    }
}

/// Decodes one completed canonical unsigned protobuf varint.
///
/// # Errors
/// Returns [`FrameCodecError::MalformedLength`] for overflow, overlong, empty,
/// or noncanonical encodings.
fn decode_varint(bytes: &[u8]) -> Result<usize, FrameCodecError> {
    if bytes.is_empty()
        || bytes.len() > 10
        || (bytes.len() == 10 && bytes[9] > 1)
        || (bytes.len() > 1 && bytes.last() == Some(&0))
    {
        return Err(FrameCodecError::MalformedLength);
    }
    let mut value = 0_u64;
    for (shift, byte) in bytes.iter().copied().enumerate() {
        let bits = u64::from(byte & 0x7f);
        value |= bits
            .checked_shl(u32::try_from(shift * 7).map_err(|_| FrameCodecError::MalformedLength)?)
            .ok_or(FrameCodecError::MalformedLength)?;
    }
    usize::try_from(value).map_err(|_| FrameCodecError::MalformedLength)
}

#[cfg(test)]
mod tests {
    use super::{FrameCodecError, FrameDecoder, FrameEncoder};
    use crate::wyrd::v1::QueryBatchFrame;

    /// Proves every byte boundary preserves exact Arrow payload bytes.
    #[test]
    fn split_at_every_boundary_round_trips_exact_bytes() {
        let message = QueryBatchFrame {
            arrow_ipc_batch: vec![0, 1, 2, 127, 128, 255],
        };
        let encoded = FrameEncoder::encode(&message).expect("frame encodes");
        for split in 0..=encoded.len() {
            let mut decoder = FrameDecoder::new(1024);
            let mut decoded = decoder
                .push::<QueryBatchFrame>(&encoded[..split])
                .expect("first chunk decodes");
            decoded.extend(
                decoder
                    .push::<QueryBatchFrame>(&encoded[split..])
                    .expect("second chunk decodes"),
            );
            decoder.finish().expect("frame is complete");
            assert_eq!(decoded, vec![message.clone()]);
        }
    }

    /// Proves multiple adjacent frames are retained.
    #[test]
    fn many_frames_in_one_chunk_round_trip() {
        let first = QueryBatchFrame {
            arrow_ipc_batch: vec![1],
        };
        let second = QueryBatchFrame {
            arrow_ipc_batch: vec![2],
        };
        let mut encoded = FrameEncoder::encode(&first).expect("first encodes");
        encoded.extend(FrameEncoder::encode(&second).expect("second encodes"));
        let mut decoder = FrameDecoder::new(8);
        assert_eq!(
            decoder
                .push::<QueryBatchFrame>(&encoded)
                .expect("frames decode"),
            vec![first, second]
        );
    }

    /// Proves an oversize prefix fails before payload buffering.
    #[test]
    fn oversize_prefix_is_rejected() {
        let mut decoder = FrameDecoder::new(4);
        assert!(matches!(
            decoder.push::<QueryBatchFrame>(&[5]),
            Err(FrameCodecError::Oversize { .. })
        ));
    }

    /// Proves malformed and truncated frames remain distinct.
    #[test]
    fn malformed_and_truncated_frames_are_rejected() {
        let mut malformed = FrameDecoder::new(1024);
        assert!(matches!(
            malformed.push::<QueryBatchFrame>(&[0x80; 11]),
            Err(FrameCodecError::MalformedLength)
        ));
        let mut truncated = FrameDecoder::new(1024);
        truncated
            .push::<QueryBatchFrame>(&[2, 0])
            .expect("partial payload is accepted until EOF");
        assert!(matches!(
            truncated.finish(),
            Err(FrameCodecError::Truncated)
        ));
    }

    /// Overflowing and noncanonical ten-byte prefixes fail before allocation.
    #[test]
    fn overflowing_and_noncanonical_varints_are_rejected() {
        for prefix in [
            vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02],
            vec![0x80, 0x00],
            vec![0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x00],
        ] {
            let mut decoder = FrameDecoder::new(usize::MAX);
            assert!(matches!(
                decoder.push::<QueryBatchFrame>(&prefix),
                Err(FrameCodecError::MalformedLength)
            ));
        }
    }
}
