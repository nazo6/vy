//! Client implementation of the `config` host traits.

use serde_json::Value;
use vy_api_schema::Json;
use vy_client_api::namespaces::config::{
    ConfigApi, ConfigGetParams, ConfigInspectParams, ConfigInspectReply, ConfigSetParams,
    ConfigSetReply,
};
use vy_host_api::config::ConfigReadApi;
use vy_host_api::{CallError, Reply};

use super::ClientHost;

impl ConfigReadApi for ClientHost<'_, '_, '_> {
    fn get(&mut self, params: ConfigGetParams) -> Result<Reply<Json>, CallError> {
        let value = self
            .call
            .context
            .config
            .get(&params.key)
            .cloned()
            .unwrap_or(Value::Null);
        Ok(Reply::Value(Json(value)))
    }

    fn inspect(
        &mut self,
        params: ConfigInspectParams,
    ) -> Result<Reply<ConfigInspectReply>, CallError> {
        let inspect = self.call.context.config.inspect(&params.key);
        Ok(Reply::Value(ConfigInspectReply {
            default: inspect.default,
            user: inspect.user,
            project: inspect.project,
            memory: inspect.memory,
            effective: inspect.effective,
        }))
    }
}

impl ConfigApi for ClientHost<'_, '_, '_> {
    fn set(&mut self, params: ConfigSetParams) -> Result<Reply<ConfigSetReply>, CallError> {
        let changed = self.call.context.config.apply(&params.values);
        self.call.outcome.config_changed = changed.clone();
        Ok(Reply::Value(ConfigSetReply { changed }))
    }
}
