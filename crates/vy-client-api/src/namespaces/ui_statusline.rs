//! `ui.statusline` declarations.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::{ThemeColor, method_decl, statusline::StatuslineAlignment};

/// `ui.statusline.createItem` parameters.
///
/// `alignment` places the item on the left or right. `priority` orders items
/// within the same side; higher values are shown first. `command` runs when the
/// item is clicked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StatuslineCreateParams {
    pub id: String,
    pub alignment: StatuslineAlignment,
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<i32>,
    pub text: String,
    #[ts(optional = nullable)]
    pub color: Option<ThemeColor>,
    #[ts(optional = nullable)]
    pub command: Option<String>,
}

/// `ui.statusline.updateItem` parameters.
///
/// `color` and `command` distinguish an absent field (leave as is) from `null`
/// (clear it). The host validates `color` against the theme tokens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StatuslineUpdateParams {
    pub id: String,
    #[ts(optional = nullable)]
    pub text: Option<String>,
    #[ts(type = "string | null")]
    pub color: Option<Value>,
    #[ts(type = "string | null")]
    pub command: Option<Value>,
    #[ts(optional = nullable)]
    pub visible: Option<bool>,
}

/// `ui.statusline.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StatuslineDisposeParams {
    pub id: String,
}

method_decl!(
    CREATE_ITEM,
    "ui.statusline.createItem",
    StatuslineCreateParams,
    (),
    "Creates a status line item and returns its id."
);
method_decl!(
    UPDATE_ITEM,
    "ui.statusline.updateItem",
    StatuslineUpdateParams,
    (),
    "Updates the fields of a status line item."
);
method_decl!(
    DISPOSE_ITEM,
    "ui.statusline.disposeItem",
    StatuslineDisposeParams,
    (),
    "Removes a status line item."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[CREATE_ITEM, UPDATE_ITEM, DISPOSE_ITEM];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "ui.statusline",
    doc: "Creates and updates status line items.",
    sugar: None,
};
