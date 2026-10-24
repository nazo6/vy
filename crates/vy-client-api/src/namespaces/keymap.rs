//! `keymap` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_host_api::{CallError, Reply};

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

/// Binds keys and updates keymap context values.
#[vy_api_schema::vy_api_trait("keymap")]
pub trait KeymapApi {
    /// Binds a key sequence to a command.
    #[vy_api_schema::vy_api_method(manual)]
    fn bind(&mut self, params: KeymapBindParams) -> Result<Reply<()>, CallError>;

    /// Removes a key binding and returns the number removed.
    #[vy_api_schema::vy_api_method]
    fn unbind(&mut self, params: KeymapUnbindParams) -> Result<Reply<usize>, CallError>;

    /// Sets a keymap context value used by `when` clauses.
    #[vy_api_schema::vy_api_method]
    fn set_context(&mut self, params: KeymapSetContextParams) -> Result<Reply<()>, CallError>;
}
