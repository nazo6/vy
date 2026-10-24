//! Shared `log` method declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::{CallError, Reply};

/// Severity of a log line or notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

/// `log.write` parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LogWriteParams {
    pub channel: String,
    pub level: LogLevel,
    pub message: String,
    #[serde(default)]
    #[ts(type = "unknown")]
    pub data: Value,
}

/// Writes to plugin log channels.
#[vy_api_schema::vy_api_trait("log")]
pub trait LogApi {
    /// Writes one entry to a log channel.
    #[vy_api_schema::vy_api_method]
    fn write(&mut self, params: LogWriteParams) -> Result<Reply<()>, CallError>;
}
