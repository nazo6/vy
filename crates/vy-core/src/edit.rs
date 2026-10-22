use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::position::{Position, Range};

/// One change to the text: replaces `range` with `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    pub text: String,
}

impl TextEdit {
    pub fn new(range: Range, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    /// Inserts at a position.
    pub fn insert(at: Position, text: impl Into<String>) -> Self {
        Self::new(Range::empty(at), text)
    }

    /// Deletes a range.
    pub fn delete(range: Range) -> Self {
        Self::new(range, "")
    }
}

/// Why an edit could not be applied.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("base version {base} does not match the current version {current}")]
    VersionMismatch { base: u64, current: u64 },

    #[error("position {0:?} is outside the document")]
    InvalidPosition(Position),

    #[error("char index {0} is outside the document")]
    InvalidOffset(usize),

    #[error("range {0:?} starts after it ends")]
    InvalidRange(Range),

    #[error("edits must be sorted by position and must not overlap")]
    OverlappingEdits,
}
