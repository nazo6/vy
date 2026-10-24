//! `ui.gutter` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::{ThemeColor, method_decl};

/// `ui.gutter.createItem` parameters.
///
/// `line` is zero-based. The item is drawn in the gutter of that line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GutterCreateParams {
    pub id: String,
    pub line: usize,
    pub text: String,
    #[ts(optional = nullable)]
    pub color: Option<ThemeColor>,
}

/// `ui.gutter.disposeItem` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GutterDisposeParams {
    pub id: String,
}

method_decl!(
    CREATE_ITEM,
    "ui.gutter.createItem",
    GutterCreateParams,
    (),
    "Creates a gutter item and returns its id."
);
method_decl!(
    DISPOSE_ITEM,
    "ui.gutter.disposeItem",
    GutterDisposeParams,
    (),
    "Removes one gutter item."
);
method_decl!(CLEAR, "ui.gutter.clear", (), "Removes every gutter item.");

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[CREATE_ITEM, DISPOSE_ITEM, CLEAR];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "ui.gutter",
    doc: "Creates and removes gutter items.",
    sugar: None,
};
