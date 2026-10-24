//! `context` namespace.

/// Reports the plugin and connection metadata.
#[vy_api_schema::vy_api_namespace("context")]
mod api {
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    use crate::api::{Call, CallError, Reply};

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

    /// Returns the plugin id, version, and remote connection information.
    #[vy_api_schema::vy_api_method]
    fn info(call: &mut Call<'_, '_>) -> Result<Reply<PluginContextReply>, CallError> {
        let env = call.context.env;
        Ok(Reply::Value(PluginContextReply {
            plugin_id: env.plugin_id.clone(),
            version: env.version.clone(),
            remote: RemoteInfo {
                is_remote: env.remote.is_remote,
                name: env.remote.name.clone(),
                os: env.remote.os.clone(),
                path_separator: env.remote.path_separator.clone(),
            },
        }))
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
