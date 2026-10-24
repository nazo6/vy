//! Shared document values and declarations exposed to plugins.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_api_schema::{EventDecl, MethodDecl, NamespaceDecl};
use vy_core::{Position, Range, Uri};

use crate::method_decl;

/// A document as the script sees it.
///
/// `end` is the position just after the last character. `version` increases
/// after every applied edit and can be used to detect a stale snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentInfoReply {
    #[ts(type = "number")]
    pub id: u64,
    pub uri: Uri,
    pub language_id: String,
    #[ts(type = "number")]
    pub version: u64,
    pub line_count: usize,
    pub end: Position,
}

/// `document.getText` parameters.
///
/// `documentId` defaults to the active document. `range` defaults to the whole
/// document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentGetTextParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    #[ts(optional = nullable)]
    pub range: Option<Range>,
}

/// `document.lineAt` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLineAtParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    pub line: usize,
}

/// `document.positionAt` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPositionAtParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    pub offset: usize,
}

/// `document.offsetAt` parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentOffsetAtParams {
    #[ts(type = "number")]
    #[ts(optional = nullable)]
    pub document_id: Option<u64>,
    pub position: Position,
}

method_decl!(
    GET_TEXT,
    "document.getText",
    DocumentGetTextParams,
    String,
    "Returns the document text, optionally limited to a range."
);
method_decl!(
    LINE_AT,
    "document.lineAt",
    DocumentLineAtParams,
    Option<String>,
    "Returns the text of a line, or null when the line is out of range."
);
method_decl!(
    POSITION_AT,
    "document.positionAt",
    DocumentPositionAtParams,
    Position,
    "Converts a UTF-16 offset to a position."
);
method_decl!(
    OFFSET_AT,
    "document.offsetAt",
    DocumentOffsetAtParams,
    usize,
    "Converts a position to a UTF-16 offset."
);

/// Every method in this namespace.
pub static METHODS: &[MethodDecl] = &[GET_TEXT, LINE_AT, POSITION_AT, OFFSET_AT];

/// Every event in this namespace.
pub static EVENTS: &[EventDecl] = &[];

/// JSDoc and sugar metadata for the namespace.
pub static NAMESPACE: NamespaceDecl = NamespaceDecl {
    path: "document",
    doc: "Reads the text and offsets of the active document.",
    sugar: None,
};
