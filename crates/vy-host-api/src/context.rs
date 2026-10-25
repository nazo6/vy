//! Shared `context` method declarations.

use crate::{CallError, Reply};
use serde::{Deserialize, Serialize};

/// `context.info` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PluginContextReply {
    pub plugin_id: String,
    pub version: String,
    pub remote: RemoteInfo,
}

/// Information about the remote connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub is_remote: bool,
    pub name: String,
    pub os: String,
    pub path_separator: String,
}

/// Reports the plugin and connection metadata.
#[vy_api_schema::vy_api_trait("context")]
pub trait ContextApi {
    /// Returns the plugin id, version, and remote connection information.
    #[vy_api_schema::vy_api_method]
    fn info(&mut self) -> Result<Reply<PluginContextReply>, CallError>;
}
