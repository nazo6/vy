//! Shared `config` method and event declarations.
//!
//! Writing settings (`config.set` / `config.define`) stays client-only.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_api_schema::{EventDecl, Json, MethodDecl, NamespaceDecl};

use crate::{event_decl, method_decl};

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

method_decl!(
    GET,
    "config.get",
    ConfigGetParams,
    Json,
    "Returns the effective value of a setting key.",
    typed_key = "SettingsMap"
);
method_decl!(
    INSPECT,
    "config.inspect",
    ConfigInspectParams,
    ConfigInspectReply,
    "Returns every layer's value for a setting key, plus the effective value."
);

event_decl!(
    ON_DID_CHANGE,
    "config.onDidChange",
    "config",
    "onDidChange",
    ConfigChange,
    "Fires when the effective value of a setting key changes."
);

/// Every method in this namespace that the core implements.
pub static METHODS: &[MethodDecl] = &[GET, INSPECT];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[ON_DID_CHANGE];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "config",
    doc: "Reads and writes settings.",
    sugar: None,
};
