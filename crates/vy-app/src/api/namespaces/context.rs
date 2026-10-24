//! `context` namespace.

/// Reports the plugin and connection metadata.
#[vy_api_schema::vy_api_namespace("context", shared = vy_host_api::context)]
mod api {
    use vy_host_api::context::{PluginContextReply, RemoteInfo};

    use crate::api::{Call, CallError, Reply};

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
