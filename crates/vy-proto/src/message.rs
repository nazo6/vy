use rmpv::Value;
use serde::{Deserialize, Serialize};

use crate::error::ErrorCode;

/// Protocol version. Matched during `initialize`.
///
/// Version 2 returns undo and redo results as ordered batches of edits.
pub const PROTOCOL_VERSION: u32 = 2;

/// Payload of every RPC frame.
///
/// It follows the message envelope in `docs/protocol.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RpcMessage {
    /// A request from the client to the server.
    Request {
        id: u64,
        session_id: u64,
        method: String,
        params: Value,
        deadline_ms: Option<u32>,
    },
    /// A response to a request.
    Response { id: u64, result: RpcResult },
    /// Cancels a request that is still running.
    Cancel { id: u64, reason: Option<String> },
    /// A one-way notification, sent in either direction.
    Notification { method: String, params: Value },
    /// A chunk of streamed data.
    Stream {
        stream_id: u64,
        seq: u64,
        payload: StreamPayload,
    },
}

impl RpcMessage {
    pub fn request(id: u64, session_id: u64, method: impl Into<String>, params: Value) -> Self {
        Self::Request {
            id,
            session_id,
            method: method.into(),
            params,
            deadline_ms: None,
        }
    }

    pub fn ok(id: u64, value: Value) -> Self {
        Self::Response {
            id,
            result: RpcResult::Ok { value },
        }
    }

    pub fn err(id: u64, code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Response {
            id,
            result: RpcResult::Err {
                code,
                message: message.into(),
                data: None,
            },
        }
    }

    pub fn notification(method: impl Into<String>, params: Value) -> Self {
        Self::Notification {
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RpcResult {
    Ok {
        value: Value,
    },
    Err {
        code: ErrorCode,
        message: String,
        data: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StreamPayload {
    Chunk(Value),
    End { summary: Option<Value> },
    Error { code: ErrorCode, message: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{decode_message, encode_message};
    use crate::value::object;

    #[test]
    fn round_trips_every_variant() {
        let messages = [
            RpcMessage::request(1, 7, "initialize", object([("protocol_version", 1.into())])),
            RpcMessage::ok(2, object([("session_id", 7.into())])),
            RpcMessage::err(3, ErrorCode::NotFound, "missing"),
            RpcMessage::Cancel {
                id: 4,
                reason: Some("user".into()),
            },
            RpcMessage::notification("initialized", object([("session_id", 7.into())])),
            RpcMessage::Stream {
                stream_id: 1,
                seq: 0,
                payload: StreamPayload::Chunk("hello".into()),
            },
        ];

        for message in messages {
            let encoded = encode_message(&message).expect("encode");
            let decoded = decode_message(&encoded).expect("decode");
            assert_eq!(decoded, message);
        }
    }

    #[test]
    fn serializes_with_type_tag() {
        let message = RpcMessage::notification("initialized", object([("session_id", 7.into())]));
        let encoded = encode_message(&message).expect("encode");
        let value: Value = rmpv::decode::read_value(&mut encoded.as_slice()).expect("value");
        assert_eq!(
            crate::value::get(&value, "type").and_then(Value::as_str),
            Some("Notification")
        );
    }
}
