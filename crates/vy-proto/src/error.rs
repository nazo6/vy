use serde::{Deserialize, Serialize};
use thiserror::Error;

/// プロトコルのエラーコードです。値と意味は `docs/protocol.md` の「エラー」にあります。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// フレームとメッセージの変換で起きた失敗です。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProtoError {
    #[error("フレームが上限 {limit} バイトを超えています: {actual} バイト")]
    FrameTooLarge { actual: usize, limit: usize },

    #[error("メッセージを符号化できません: {0}")]
    Encode(String),

    #[error("メッセージを復号できません: {0}")]
    Decode(String),
}
