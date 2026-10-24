//! Client implementation of the `context` host trait.

use vy_host_api::context::{ContextApi, PluginContextReply, RemoteInfo};
use vy_host_api::{CallError, Reply};

use super::ClientHost;

impl ContextApi for ClientHost<'_, '_, '_> {
    fn info(&mut self) -> Result<Reply<PluginContextReply>, CallError> {
        let env = self.call.context.env;
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
