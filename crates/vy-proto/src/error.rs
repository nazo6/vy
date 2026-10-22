use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Protocol error codes. Values and meanings are in "Errors" in `docs/protocol.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum ErrorCode {
    InvalidRequest,
    CapabilityDenied,
    NotFound,
    InvalidState,
    Cancelled,
    Timeout,
    UnsupportedProtocolVersion,
    UnsupportedFeature,
    ResourceLimit,
    Internal,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::InvalidRequest => "InvalidRequest",
            Self::CapabilityDenied => "CapabilityDenied",
            Self::NotFound => "NotFound",
            Self::InvalidState => "InvalidState",
            Self::Cancelled => "Cancelled",
            Self::Timeout => "Timeout",
            Self::UnsupportedProtocolVersion => "UnsupportedProtocolVersion",
            Self::UnsupportedFeature => "UnsupportedFeature",
            Self::ResourceLimit => "ResourceLimit",
            Self::Internal => "Internal",
        };
        f.write_str(name)
    }
}

/// A failure while converting frames or messages.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtoError {
    #[error("frame exceeds the {limit} byte limit: {actual} bytes")]
    FrameTooLarge { actual: usize, limit: usize },

    #[error("could not encode the message: {0}")]
    Encode(String),

    #[error("could not decode the message: {0}")]
    Decode(String),
}
