//! Client implementation of the `keymap` host trait.

use vy_client_api::namespaces::keymap::{
    KeyContextValue, KeymapApi, KeymapBindParams, KeymapSetContextParams, KeymapUnbindParams,
};
use vy_host_api::{CallError, Reply};

use super::ClientHost;
use crate::when;

impl KeymapApi for ClientHost<'_, '_, '_> {
    fn bind(&mut self, params: KeymapBindParams) -> Result<Reply<()>, CallError> {
        let owner = self.call.context.owner.clone();
        self.call
            .context
            .keymap
            .bind_with_args(
                &params.keys,
                params.command,
                params.when.as_deref(),
                params.args.unwrap_or(serde_json::Value::Null),
                owner,
            )
            .map_err(|error| CallError::invalid_state(error.to_string()))?;
        Ok(Reply::Value(()))
    }

    fn unbind(&mut self, params: KeymapUnbindParams) -> Result<Reply<usize>, CallError> {
        let count = self
            .call
            .context
            .keymap
            .unbind(&params.keys)
            .map_err(|error| CallError::invalid_state(error.to_string()))?;
        Ok(Reply::Value(count))
    }

    fn set_context(&mut self, params: KeymapSetContextParams) -> Result<Reply<()>, CallError> {
        self.call
            .context
            .keymap
            .set_context(params.key, context_value(params.value));
        Ok(Reply::Value(()))
    }
}

fn context_value(value: KeyContextValue) -> when::Value {
    match value {
        KeyContextValue::Bool(value) => when::Value::Bool(value),
        KeyContextValue::Text(value) => when::Value::Text(value),
        KeyContextValue::Number(value) => when::Value::Number(value),
    }
}
