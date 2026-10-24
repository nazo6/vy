//! `keymap` namespace.

/// Binds keys and updates keymap context values.
#[vy_api_schema::vy_api_namespace("keymap")]
mod api {
    use serde::{Deserialize, Serialize};
    use ts_rs::TS;

    use crate::api::{Call, CallError, Reply};
    use crate::commands::Owner;
    use crate::when;

    /// `keymap.bind` parameters.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct KeymapBindParams {
        pub keys: String,
        pub command: String,
        #[ts(optional = nullable)]
        pub when: Option<String>,
    }

    /// `keymap.unbind` parameters.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
    pub struct KeymapUnbindParams {
        pub keys: String,
    }

    /// A value visible to a `when` expression, set by `keymap.setContext`.
    ///
    /// A context value is a boolean, a string, or a number.
    #[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
    #[serde(untagged)]
    pub enum KeyContextValue {
        Bool(bool),
        Text(String),
        Number(f64),
    }

    /// `keymap.setContext` parameters.
    #[derive(Debug, Clone, PartialEq, Deserialize, Serialize, TS)]
    pub struct KeymapSetContextParams {
        pub key: String,
        pub value: KeyContextValue,
    }

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
