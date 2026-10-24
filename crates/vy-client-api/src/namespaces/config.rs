//! Client-only `config` declarations.
//!
//! Reading settings is shared through `vy_host_api::config::ConfigReadApi`;
//! this trait adds the client-side write path.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_host_api::{CallError, Reply};

pub use vy_host_api::config::{
    ConfigChange, ConfigGetParams, ConfigInspectParams, ConfigInspectReply, EVENTS, GET, INSPECT,
};

/// `config.set` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigSetParams {
    #[ts(type = "Record<string, unknown>")]
    pub values: serde_json::Map<String, Value>,
}

/// `config.set` reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ConfigSetReply {
    pub changed: Vec<String>,
}

/// Reads and writes settings.
#[vy_api_schema::vy_api_trait("config")]
pub trait ConfigApi: vy_host_api::config::ConfigReadApi {
    /// Sets one or more setting values in memory and returns the keys that changed.
    #[vy_api_schema::vy_api_method]
    fn set(&mut self, params: ConfigSetParams) -> Result<Reply<ConfigSetReply>, CallError>;
}
