//! `config` namespace.

/// Reads and writes settings.
#[vy_api_schema::vy_api_namespace("config")]
mod api {
    use serde_json::Value;
    use vy_api_schema::{
        ConfigChange, ConfigGetParams, ConfigInspectParams, ConfigInspectReply, ConfigSetParams,
        ConfigSetReply, Json,
    };

    use crate::api::{Call, CallError, Reply};

    /// Returns the effective value of a setting key.
    #[vy_api_schema::vy_api_method(typed_key = SettingsMap)]
    fn get(call: &mut Call<'_, '_>, params: ConfigGetParams) -> Result<Reply<Json>, CallError> {
        let value = call
            .context
            .config
            .get(&params.key)
            .cloned()
            .unwrap_or(Value::Null);
        Ok(Reply::Value(Json(value)))
    }

    /// Sets one or more setting values in memory and returns the keys that changed.
    #[vy_api_schema::vy_api_method]
    fn set(
        call: &mut Call<'_, '_>,
        params: ConfigSetParams,
    ) -> Result<Reply<ConfigSetReply>, CallError> {
        let changed = call.context.config.apply(&params.values);
        call.outcome.config_changed = changed.clone();
        Ok(Reply::Value(ConfigSetReply { changed }))
    }

    /// Returns every layer's value for a setting key, plus the effective value.
    #[vy_api_schema::vy_api_method]
    fn inspect(
        call: &mut Call<'_, '_>,
        params: ConfigInspectParams,
    ) -> Result<Reply<ConfigInspectReply>, CallError> {
        let inspect = call.context.config.inspect(&params.key);
        Ok(Reply::Value(ConfigInspectReply {
            default: inspect.default,
            user: inspect.user,
            project: inspect.project,
            memory: inspect.memory,
            effective: inspect.effective,
        }))
    }

    /// Fires when the effective value of a setting key changes.
    #[vy_api_schema::vy_api_event("config.onDidChange", payload = ConfigChange)]
    const ON_DID_CHANGE: () = ();
}

pub use self::api::{EVENTS, METHODS, NAMESPACE, dispatch};
