//! Vy のクライアントとコアをつなぐ RPC の型、フレーム、版。
//!
//! 送受信の方法は [`protocol.md`](../../../docs/protocol.md) にあります。
//! このクレートは I/O を持ちません。フレームの組み立てと分解だけを提供し、
//! 読み書きは `vy-server` と `vy-app` が行います。

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
