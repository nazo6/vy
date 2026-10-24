//! The empty `ui` namespace.
//!
//! Widget builders live in the shim. The namespace exists so the generated
//! declarations can group `ui.statusline`, `ui.gutter`, and the other slots.

use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "ui",
    doc: "Builds native widget trees and updates UI slots.",
    sugar: None,
};
