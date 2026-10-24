//! Shared `log` method declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::method_decl;

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

method_decl!(
    WRITE,
    "log.write",
    LogWriteParams,
    (),
    "Writes one entry to a log channel."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[WRITE];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "log",
    doc: "Writes to plugin log channels.",
    sugar: None,
};
