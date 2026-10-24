//! Client-only `config` declarations.
//!
//! `config.get` and `config.inspect` are shared with the core; their
//! declarations are re-exported so the client handler module can attach to
//! them through `shared = vy_client_api::config`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_api_schema::MethodDecl;

use crate::method_decl;

pub use vy_host_api::config::{
    ConfigChange, ConfigGetParams, ConfigInspectParams, ConfigInspectReply, EVENTS, GET, INSPECT,
    NAMESPACE,
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

method_decl!(
    SET,
    "config.set",
    ConfigSetParams,
    ConfigSetReply,
    "Sets one or more setting values in memory and returns the keys that changed."
);

/// Every method that only the client implements.
pub static METHODS: &[MethodDecl] = &[SET];
