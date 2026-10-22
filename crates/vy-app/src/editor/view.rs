//! Pane-local view state (cursor, selection, mode, pending operators).

use crate::keymap::Mode;
use vy_core::{Position, Range};

use super::operator::{Operator, TextObjectKind};

/// The part of the editor that belongs to one pane.
///
/// The text and the change history stay with the document. Cursor, selection, mode,
/// and pending operators are independent for each pane that shows the document
/// (`docs/panes.md` and ADR 0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewState {
    pub(super) mode: Mode,
    pub(super) anchor: Position,
    pub(super) head: Position,
    pub(super) operator: Option<Operator>,
    pub(super) operator_count: Option<u32>,
    pub(super) text_object: Option<TextObjectKind>,
    pub(super) preferred_character: Option<u32>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            anchor: Position::new(0, 0),
            head: Position::new(0, 0),
            operator: None,
            operator_count: None,
            text_object: None,
            preferred_character: None,
        }
    }
}

impl ViewState {
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Cursor used for drawing. The selection start in Normal, otherwise the end.
    pub fn cursor(&self) -> Position {
        match self.mode {
            Mode::Normal => self.ordered().0,
            _ => self.head,
        }
    }

    pub fn selection(&self) -> Range {
        let (start, end) = self.ordered();
        Range::new(start, end)
    }

    pub fn operator_pending(&self) -> bool {
        self.operator.is_some()
    }

    pub fn text_object_pending(&self) -> bool {
        self.text_object.is_some()
    }

    pub(super) fn ordered(&self) -> (Position, Position) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}
