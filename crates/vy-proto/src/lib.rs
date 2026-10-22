//! RPC types, framing, and version for the link between the Vy client and core.
//!
//! See [`protocol.md`](../../../docs/protocol.md) for how messages are sent and
//! received. This crate has no I/O. It provides only frame assembly and splitting;
//! `vy-server` and `vy-app` do the reading and writing.

pub mod error;
pub mod frame;
pub mod message;
pub mod value;

pub use error::{ErrorCode, ProtoError};
pub use frame::{
    FrameDecoder, LENGTH_HEADER_LEN, MAX_FRAME_LEN, decode_message, encode_frame, encode_message,
};
pub use message::{PROTOCOL_VERSION, RpcMessage, RpcResult, StreamPayload};
pub use value::{get, object};
