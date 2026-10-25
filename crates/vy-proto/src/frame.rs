use crate::error::ProtoError;

/// Upper bound for a single frame. See "Streams and large data" in `docs/specs/protocol.md`.
///
/// A `buffer/snapshot` of a large file arrives in one frame, so the limit has to leave
/// room for the whole text. Chunked snapshots would let it shrink again.
pub const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

/// Size of the length header: a little-endian u32.
pub const LENGTH_HEADER_LEN: usize = 4;

/// Builds a frame by prefixing the payload with its length.
pub fn encode_frame(payload: &[u8]) -> Result<Vec<u8>, ProtoError> {
    if payload.len() > MAX_FRAME_LEN {
        return Err(ProtoError::FrameTooLarge {
            actual: payload.len(),
            limit: MAX_FRAME_LEN,
        });
    }

    let mut frame = Vec::with_capacity(LENGTH_HEADER_LEN + payload.len());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Encodes a message as MessagePack without the length header.
///
/// Wrap it with [`encode_frame`] to send it.
pub fn encode_message(message: &crate::message::RpcMessage) -> Result<Vec<u8>, ProtoError> {
    rmp_serde::to_vec_named(message).map_err(|error| ProtoError::Encode(error.to_string()))
}

/// Decodes a frame payload back into a message.
pub fn decode_message(payload: &[u8]) -> Result<crate::message::RpcMessage, ProtoError> {
    rmp_serde::from_slice(payload).map_err(|error| ProtoError::Decode(error.to_string()))
}

/// Splits frames out of the bytes read from a stream.
///
/// Streams arrive in pieces, so callers push bytes with [`FrameDecoder::push`] and
/// repeat until [`FrameDecoder::next_frame`] returns [`None`].
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
    start: usize,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        // Compact the buffer once the consumed prefix is at least half of it.
        // Compacting on every call would make this O(n^2).
        if self.start > 0 && self.start * 2 >= self.buffer.len() {
            self.buffer.drain(..self.start);
            self.start = 0;
        }
        self.buffer.extend_from_slice(bytes);
    }

    /// Returns the payload of the next complete frame, or [`None`] if it has not arrived yet.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, ProtoError> {
        let available = &self.buffer[self.start..];
        if available.len() < LENGTH_HEADER_LEN {
            return Ok(None);
        }

        let length = u32::from_le_bytes(
            available[..LENGTH_HEADER_LEN]
                .try_into()
                .expect("4 bytes checked above"),
        ) as usize;

        if length > MAX_FRAME_LEN {
            return Err(ProtoError::FrameTooLarge {
                actual: length,
                limit: MAX_FRAME_LEN,
            });
        }

        if available.len() < LENGTH_HEADER_LEN + length {
            return Ok(None);
        }

        let payload = available[LENGTH_HEADER_LEN..LENGTH_HEADER_LEN + length].to_vec();
        self.start += LENGTH_HEADER_LEN + length;

        if self.start == self.buffer.len() {
            self.buffer.clear();
            self.start = 0;
        }

        Ok(Some(payload))
    }

    /// Number of bytes not yet split into a frame. Used by tests and diagnostics.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len() - self.start
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{RpcMessage, StreamPayload};
    use crate::value::object;

    #[test]
    fn frame_starts_with_little_endian_length() {
        let frame = encode_frame(b"abc").expect("encode");
        assert_eq!(&frame[..4], &[3, 0, 0, 0]);
        assert_eq!(&frame[4..], b"abc");
    }

    #[test]
    fn rejects_payload_over_limit() {
        let payload = vec![0u8; MAX_FRAME_LEN + 1];
        assert_eq!(
            encode_frame(&payload),
            Err(ProtoError::FrameTooLarge {
                actual: MAX_FRAME_LEN + 1,
                limit: MAX_FRAME_LEN,
            })
        );
    }

    #[test]
    fn waits_for_a_complete_frame() {
        let message = RpcMessage::notification("ping", object([("n", 1.into())]));
        let payload = encode_message(&message).expect("encode");
        let frame = encode_frame(&payload).expect("frame");
        let mut decoder = FrameDecoder::new();

        for byte in &frame[..frame.len() - 1] {
            decoder.push(&[*byte]);
            assert_eq!(decoder.next_frame().expect("next"), None);
        }

        decoder.push(&frame[frame.len() - 1..]);
        let payload = decoder.next_frame().expect("next").expect("frame");
        assert_eq!(
            crate::frame::decode_message(&payload).expect("decode"),
            message
        );
        assert_eq!(decoder.buffered_len(), 0);
        assert_eq!(decoder.next_frame().expect("next"), None);
    }

    #[test]
    fn splits_multiple_frames_in_one_push() {
        let first = RpcMessage::notification("a", rmpv::Value::Nil);
        let second = RpcMessage::Stream {
            stream_id: 1,
            seq: 0,
            payload: StreamPayload::End { summary: None },
        };

        let mut bytes = encode_frame(&encode_message(&first).expect("encode")).expect("frame");
        bytes.extend(encode_frame(&encode_message(&second).expect("encode")).expect("frame"));

        let mut decoder = FrameDecoder::new();
        decoder.push(&bytes);

        let one = decoder.next_frame().expect("next").expect("frame");
        let two = decoder.next_frame().expect("next").expect("frame");
        assert_eq!(crate::frame::decode_message(&one).expect("decode"), first);
        assert_eq!(crate::frame::decode_message(&two).expect("decode"), second);
        assert_eq!(decoder.next_frame().expect("next"), None);
    }

    #[test]
    fn rejects_an_oversized_length_header() {
        let mut decoder = FrameDecoder::new();
        decoder.push(&((MAX_FRAME_LEN + 1) as u32).to_le_bytes());
        assert!(matches!(
            decoder.next_frame(),
            Err(ProtoError::FrameTooLarge { .. })
        ));
    }
}
