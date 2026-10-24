//! Shared `config` method and event declarations.
//!
//! Writing settings (`config.set`) stays client-only.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_api_schema::Json;

use crate::{CallError, Reply};

/// `config.get` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigGetParams {
    pub key: String,
}

/// `config.inspect` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigInspectParams {
    pub key: String,
}

/// `config.inspect` reply: every layer's value plus the effective one.
///
/// Layers are listed from lowest to highest precedence: `default`, `user`,
/// `project`, `memory`. A layer that does not define the key is `null`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ConfigInspectReply {
    #[ts(type = "unknown")]
    pub default: Option<Value>,
    #[ts(type = "unknown")]
    pub user: Option<Value>,
    #[ts(type = "unknown")]
    pub project: Option<Value>,
    #[ts(type = "unknown")]
    pub memory: Option<Value>,
    #[ts(type = "unknown")]
    pub effective: Option<Value>,
}

/// Payload of `config.onDidChange`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigChange {
    pub key: String,
}

/// Reads and writes settings.
#[vy_api_schema::vy_api_trait("config")]
pub trait ConfigReadApi {
    /// Returns the effective value of a setting key.
    #[vy_api_schema::vy_api_method(typed_key = SettingsMap)]
    fn get(&mut self, params: ConfigGetParams) -> Result<Reply<Json>, CallError>;

    /// Returns every layer's value for a setting key, plus the effective value.
    #[vy_api_schema::vy_api_method]
    fn inspect(
        &mut self,
        params: ConfigInspectParams,
    ) -> Result<Reply<ConfigInspectReply>, CallError>;

    /// Fires when the effective value of a setting key changes.
    #[vy_api_schema::vy_api_event("config.onDidChange", payload = ConfigChange)]
    const ON_DID_CHANGE: ();
}
