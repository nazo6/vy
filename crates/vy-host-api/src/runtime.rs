//! Types shared by the host traits and their dispatch functions.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use vy_proto::ErrorCode;

/// The value a handler returns now, or a reply the host sends later.
#[derive(Debug)]
pub enum Reply<T> {
    Value(T),
    Deferred,
}

impl<T> Reply<T> {
    pub fn value(value: T) -> Self {
        Self::Value(value)
    }
}

/// A failure the shim receives as an exception with a stable code.
#[derive(Debug)]
pub struct CallError {
    pub code: ErrorCode,
    pub message: String,
}

impl CallError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidParams, message)
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidState, message)
    }
}

impl From<serde_json::Error> for CallError {
    fn from(error: serde_json::Error) -> Self {
        Self::invalid_params(error.to_string())
    }
}

/// Deserializes the parameters of one method.
///
/// Missing parameters arrive as `null`; treating that as an empty object keeps
/// methods without parameters from needing a special case.
pub fn parse_params<T: DeserializeOwned>(params: &Value) -> Result<T, CallError> {
    let empty = Value::Object(serde_json::Map::new());
    let value = if params.is_null() { &empty } else { params };
    Ok(serde_json::from_value(value.clone())?)
}

/// Serializes a handler's reply for the script boundary.
pub fn erase_reply<T: Serialize>(reply: Reply<T>) -> Result<Reply<Value>, CallError> {
    match reply {
        Reply::Value(value) => Ok(Reply::Value(serde_json::to_value(value)?)),
        Reply::Deferred => Ok(Reply::Deferred),
    }
}
