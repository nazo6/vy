//! `editor` declarations.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_core::{Selection, TextEdit, Uri};
use vy_host_api::{CallError, DocumentInfoReply, Reply};

use crate::keymap::Mode;

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

/// Reads and edits the active document.
#[vy_api_schema::vy_api_trait("editor")]
pub trait EditorApi {
    /// Returns the document in the focused pane, or null when none is open.
    #[vy_api_schema::vy_api_method]
    fn active_document(&mut self) -> Result<Reply<Option<DocumentInfoReply>>, CallError>;

    /// Opens a document in an editor pane.
    #[vy_api_schema::vy_api_method]
    fn open(&mut self, params: EditorOpenParams) -> Result<Reply<()>, CallError>;

    /// Returns the editor mode of the focused pane.
    #[vy_api_schema::vy_api_method]
    fn mode(&mut self) -> Result<Reply<Mode>, CallError>;

    /// Switches the focused pane to a mode.
    #[vy_api_schema::vy_api_method]
    fn set_mode(&mut self, params: EditorSetModeParams) -> Result<Reply<()>, CallError>;

    /// Returns the selections of the focused pane.
    #[vy_api_schema::vy_api_method]
    fn selections(&mut self) -> Result<Reply<Vec<Selection>>, CallError>;

    /// Replaces the selections of the focused pane.
    #[vy_api_schema::vy_api_method]
    fn set_selections(&mut self, params: EditorSetSelectionsParams)
    -> Result<Reply<()>, CallError>;

    /// Applies edits to the active document in one transaction.
    #[vy_api_schema::vy_api_method]
    fn edit(&mut self, params: EditorEditParams) -> Result<Reply<EditorEditReply>, CallError>;

    /// Undoes the last transaction.
    #[vy_api_schema::vy_api_method]
    fn undo(&mut self) -> Result<Reply<()>, CallError>;

    /// Redoes the last undone transaction.
    #[vy_api_schema::vy_api_method]
    fn redo(&mut self) -> Result<Reply<()>, CallError>;

    /// Returns every open document.
    #[vy_api_schema::vy_api_method]
    fn documents(&mut self) -> Result<Reply<Vec<DocumentInfoReply>>, CallError>;

    /// Fires when the document in the focused pane changes.
    #[vy_api_schema::vy_api_event(
        "editor.onDidChangeActiveDocument",
        payload = ActiveDocumentChange
    )]
    const ON_DID_CHANGE_ACTIVE_DOCUMENT: ();

    /// Fires when the selections in the focused pane change.
    #[vy_api_schema::vy_api_event("editor.onDidChangeSelection", payload = SelectionChange)]
    const ON_DID_CHANGE_SELECTION: ();
}
