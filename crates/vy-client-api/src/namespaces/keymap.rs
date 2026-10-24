//! `keymap` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::method_decl;

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

method_decl!(
    BIND,
    "keymap.bind",
    KeymapBindParams,
    (),
    "Binds a key sequence to a command."
);
method_decl!(
    UNBIND,
    "keymap.unbind",
    KeymapUnbindParams,
    usize,
    "Removes a key binding and returns the number removed."
);
method_decl!(
    SET_CONTEXT,
    "keymap.setContext",
    KeymapSetContextParams,
    (),
    "Sets a keymap context value used by `when` clauses."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[BIND, UNBIND, SET_CONTEXT];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "keymap",
    doc: "Binds keys and updates keymap context values.",
    sugar: None,
};
