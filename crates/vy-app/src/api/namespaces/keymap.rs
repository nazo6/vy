//! `keymap` namespace.

/// Binds keys and updates keymap context values.
#[vy_api_schema::vy_api_namespace("keymap", shared = vy_client_api::namespaces::keymap)]
mod api {
    use vy_client_api::namespaces::keymap::{
        KeyContextValue, KeymapBindParams, KeymapSetContextParams, KeymapUnbindParams,
    };

    use crate::api::{Call, CallError, Reply};
    use crate::commands::Owner;
    use crate::when;

    /// Binds a key sequence to a command.
    #[vy_api_schema::vy_api_method]
    fn bind(call: &mut Call<'_, '_>, params: KeymapBindParams) -> Result<Reply<()>, CallError> {
        call.context
            .keymap
            .bind(
                &params.keys,
                params.command,
                params.when.as_deref(),
                Owner::Init,
            )
            .map_err(|error| CallError::invalid_state(error.to_string()))?;
        Ok(Reply::Value(()))
    }

    /// Removes a key binding and returns the number removed.
    #[vy_api_schema::vy_api_method]
    fn unbind(
        call: &mut Call<'_, '_>,
        params: KeymapUnbindParams,
    ) -> Result<Reply<usize>, CallError> {
        let count = call
            .context
            .keymap
            .unbind(&params.keys)
            .map_err(|error| CallError::invalid_state(error.to_string()))?;
        Ok(Reply::Value(count))
    }

    /// Sets a keymap context value used by `when` clauses.
    #[vy_api_schema::vy_api_method]
    fn set_context(
        call: &mut Call<'_, '_>,
        params: KeymapSetContextParams,
    ) -> Result<Reply<()>, CallError> {
        call.context
            .keymap
            .set_context(params.key, context_value(params.value));
        Ok(Reply::Value(()))
    }

    fn context_value(value: KeyContextValue) -> when::Value {
        match value {
            KeyContextValue::Bool(value) => when::Value::Bool(value),
            KeyContextValue::Text(value) => when::Value::Text(value),
            KeyContextValue::Number(value) => when::Value::Number(value),
        }
    }
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
