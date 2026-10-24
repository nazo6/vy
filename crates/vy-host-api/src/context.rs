//! Shared `context` method declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::method_decl;

/// `context.info` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PluginContextReply {
    pub plugin_id: String,
    pub version: String,
    pub remote: RemoteInfo,
}

/// Information about the remote connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub is_remote: bool,
    pub name: String,
    pub os: String,
    pub path_separator: String,
}

method_decl!(
    INFO,
    "context.info",
    PluginContextReply,
    "Returns the plugin id, version, and remote connection information."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[INFO];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "context",
    doc: "Reports the plugin and connection metadata.",
    sugar: None,
};
