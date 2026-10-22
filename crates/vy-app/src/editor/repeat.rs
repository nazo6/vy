//! Recording and repeating changes with .

use crate::keymap::Mode;
use vy_core::Position;

use super::Editor;
use super::action::Action;
use super::register::RegisterKind;

/// A change that  can repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Repeat {
    /// Deletes  characters from the cursor.
    Delete { count: u32 },
    /// Inserts text the way an Insert session did.
    Insert { text: String },
    /// Repeats a paste with the same register content.
    Paste {
        text: String,
        kind: RegisterKind,
        before: bool,
    },
}

impl Editor {
    /// Enters Insert mode with the caret collapsed at `at`.
    pub(super) fn begin_insert(&mut self, anchor: Position, at: Position) {
        self.insert_buffer.clear();
        self.set_mode(Mode::Insert);
        self.anchor = anchor;
        self.head = at;
    }

    /// Ends the Insert session and remembers its text for `.`.
    pub(super) fn finish_insert(&mut self) {
        if !self.insert_buffer.is_empty() {
            self.last_change = Some(Repeat::Insert {
                text: std::mem::take(&mut self.insert_buffer),
            });
        }
        self.insert_buffer.clear();
    }

    /// Repeats the last change `count` times.
    pub(super) fn repeat_last(&mut self, count: u32) -> Action {
        match self.last_change.clone() {
            Some(Repeat::Delete { count: deleted }) => {
                self.delete_forward(deleted.saturating_mul(count))
            }
            Some(Repeat::Insert { text }) => {
                let at = self.ordered().0;
                self.insert_at(at, &text.repeat(count as usize))
            }
            Some(Repeat::Paste { text, kind, before }) => {
                self.paste_text(&text, kind, before, count)
            }
            None => Action::None,
        }
    }
}
