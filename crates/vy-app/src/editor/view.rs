//! Pane-local view state (cursor, selection, mode, pending operators).

use crate::keymap::Mode;
use vy_core::Position;

use super::input_state::PendingInput;

/// The part of the editor that belongs to one pane.
///
/// The text and the change history stay with the document. Cursor, selection, mode,
/// and pending operators are independent for each pane that shows the document
/// (`docs/specs/panes.md` and ADR 0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewState {
    pub(super) mode: Mode,
    pub(super) anchor: Position,
    pub(super) head: Position,
    pub(super) pending: PendingInput,
    pub(super) preferred_character: Option<u32>,
    pub(super) preferred_display_col: Option<usize>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            mode: Mode::Normal,
            anchor: Position::new(0, 0),
            head: Position::new(0, 0),
            pending: PendingInput::Idle,
            preferred_character: None,
            preferred_display_col: None,
        }
    }
}

impl ViewState {
    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn anchor(&self) -> Position {
        self.anchor
    }

    pub fn head(&self) -> Position {
        self.head
    }

    /// Cursor used for drawing. The earlier endpoint in Normal, otherwise the head.
    pub fn cursor(&self) -> Position {
        match self.mode {
            Mode::Normal => self.anchor.min(self.head),
            _ => self.head,
        }
    }

    pub fn operator_pending(&self) -> bool {
        self.pending.operator_pending()
    }

    pub fn text_object_pending(&self) -> bool {
        self.pending.text_object_pending()
    }
}
