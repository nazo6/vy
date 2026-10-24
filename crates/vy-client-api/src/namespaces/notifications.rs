//! `notifications` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::method_decl;

/// `notifications.show` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NotificationsShowParams {
    pub message: String,
}

method_decl!(
    SHOW,
    "notifications.show",
    NotificationsShowParams,
    (),
    "Shows a notification to the user."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[SHOW];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "notifications",
    doc: "Shows notifications to the user.",
    sugar: None,
};
