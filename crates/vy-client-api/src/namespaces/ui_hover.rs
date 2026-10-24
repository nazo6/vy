//! `ui.hover` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::method_decl;

/// `ui.hover.show` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct HoverShowParams {
    pub line: usize,
    pub character: usize,
    pub text: String,
}

method_decl!(
    SHOW,
    "ui.hover.show",
    HoverShowParams,
    (),
    "Shows a hover popup at a position."
);
method_decl!(HIDE, "ui.hover.hide", (), "Hides the hover popup.");

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[SHOW, HIDE];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "ui.hover",
    doc: "Shows and hides hover popups.",
    sugar: None,
};
