//! `ui.picker` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};

use crate::{PickerItem, method_decl};

/// `ui.picker.show` parameters.
///
/// When `items` is omitted the picker lists commands. `placeholder` is shown
/// in the filter input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PickerShowParams {
    #[serde(default)]
    #[ts(optional)]
    pub items: Option<Vec<PickerItem>>,
    #[ts(optional = nullable)]
    pub placeholder: Option<String>,
}

/// `ui.picker.show` reply.
///
/// The reply is `null` when the picker is dismissed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PickerShowReply {
    pub selected_id: String,
}

method_decl!(
    SHOW,
    "ui.picker.show",
    PickerShowParams,
    Option<PickerShowReply>,
    "Shows a picker and resolves with the chosen item."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[SHOW];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "ui.picker",
    doc: "Shows a picker and returns the chosen item.",
    sugar: None,
};
