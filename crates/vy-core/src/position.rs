use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A zero-based line and a column in UTF-16 code units.
///
/// This matches how JavaScript counts string positions. The core stores char indices
/// internally and converts to this type at the boundary (see "Position representation"
/// in `docs/architecture.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A range between two positions. The end is exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// An empty range at a single position.
    pub const fn empty(at: Position) -> Self {
        Self { start: at, end: at }
    }
}

/// A cursor. A point when `anchor` and `head` are equal, otherwise a selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub const fn caret(at: Position) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }
}
