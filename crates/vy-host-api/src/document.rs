//! Shared document values and declarations exposed to plugins.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use vy_core::{Position, Range, Uri};

use crate::{CallError, Reply};

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

/// Reads the text and offsets of the active document.
#[vy_api_schema::vy_api_trait("document")]
pub trait DocumentApi {
    /// Returns the document text, optionally limited to a range.
    #[vy_api_schema::vy_api_method]
    fn get_text(&mut self, params: DocumentGetTextParams) -> Result<Reply<String>, CallError>;

    /// Returns the text of a line, or null when the line is out of range.
    #[vy_api_schema::vy_api_method]
    fn line_at(&mut self, params: DocumentLineAtParams)
    -> Result<Reply<Option<String>>, CallError>;

    /// Converts a UTF-16 offset to a position.
    #[vy_api_schema::vy_api_method]
    fn position_at(
        &mut self,
        params: DocumentPositionAtParams,
    ) -> Result<Reply<Position>, CallError>;

    /// Converts a position to a UTF-16 offset.
    #[vy_api_schema::vy_api_method]
    fn offset_at(&mut self, params: DocumentOffsetAtParams) -> Result<Reply<usize>, CallError>;
}
