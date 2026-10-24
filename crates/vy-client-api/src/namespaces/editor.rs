//! `editor` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};
use vy_core::{Selection, TextEdit, Uri};
use vy_host_api::DocumentInfoReply;

use crate::{keymap::Mode, method_decl};

/// `editor.setMode` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EditorSetModeParams {
    pub mode: Mode,
}

/// `editor.setSelections` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EditorSetSelectionsParams {
    pub selections: Vec<Selection>,
}

/// `editor.edit` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EditorEditParams {
    pub edits: Vec<TextEdit>,
}

/// `editor.edit` reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct EditorEditReply {
    #[ts(type = "number")]
    pub version: u64,
}

/// `editor.open` parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EditorOpenParams {
    pub uri: Uri,
    #[serde(default)]
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub pane_id: Option<u64>,
}

/// Payload of `editor.onDidChangeSelection`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SelectionChange {
    pub selections: Vec<Selection>,
}

/// Payload of `editor.onDidChangeActiveDocument`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ActiveDocumentChange {
    pub document: Option<DocumentInfoReply>,
}

method_decl!(
    ACTIVE_DOCUMENT,
    "editor.activeDocument",
    Option<DocumentInfoReply>,
    "Returns the document in the focused pane, or null when none is open."
);
method_decl!(
    OPEN,
    "editor.open",
    EditorOpenParams,
    (),
    "Opens a document in an editor pane."
);
method_decl!(
    MODE,
    "editor.mode",
    Mode,
    "Returns the editor mode of the focused pane."
);
method_decl!(
    SET_MODE,
    "editor.setMode",
    EditorSetModeParams,
    (),
    "Switches the focused pane to a mode."
);
method_decl!(
    SELECTIONS,
    "editor.selections",
    Vec<Selection>,
    "Returns the selections of the focused pane."
);
method_decl!(
    SET_SELECTIONS,
    "editor.setSelections",
    EditorSetSelectionsParams,
    (),
    "Replaces the selections of the focused pane."
);
method_decl!(
    EDIT,
    "editor.edit",
    EditorEditParams,
    EditorEditReply,
    "Applies edits to the active document in one transaction."
);
method_decl!(UNDO, "editor.undo", (), "Undoes the last transaction.");
method_decl!(
    REDO,
    "editor.redo",
    (),
    "Redoes the last undone transaction."
);
method_decl!(
    DOCUMENTS,
    "editor.documents",
    Vec<DocumentInfoReply>,
    "Returns every open document."
);

crate::event_decl!(
    ON_DID_CHANGE_ACTIVE_DOCUMENT,
    "editor.onDidChangeActiveDocument",
    "editor",
    "onDidChangeActiveDocument",
    ActiveDocumentChange,
    "Fires when the document in the focused pane changes."
);
crate::event_decl!(
    ON_DID_CHANGE_SELECTION,
    "editor.onDidChangeSelection",
    "editor",
    "onDidChangeSelection",
    SelectionChange,
    "Fires when the selections in the focused pane change."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[
    ACTIVE_DOCUMENT,
    OPEN,
    MODE,
    SET_MODE,
    SELECTIONS,
    SET_SELECTIONS,
    EDIT,
    UNDO,
    REDO,
    DOCUMENTS,
];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[ON_DID_CHANGE_ACTIVE_DOCUMENT, ON_DID_CHANGE_SELECTION];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "editor",
    doc: "Reads and edits the active document.",
    sugar: None,
};
