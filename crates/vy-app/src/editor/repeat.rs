//! Recording and repeating changes with .

use crate::keymap::Mode;
use vy_core::Position;

use super::Editor;
use super::action::Action;
use super::register::RegisterKind;

/// The kind of Insert session, remembered for `.`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum InsertKind {
    #[default]
    Normal,
    OpenBelow,
    OpenAbove,
}

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
    /// Opens a line below and inserts text.
    OpenBelow { text: String },
    /// Opens a line above and inserts text.
    OpenAbove { text: String },
}

impl Editor {
    /// Enters Insert mode with the caret collapsed at `at`.
    pub(super) fn begin_insert(&mut self, anchor: Position, at: Position) {
        self.insert_session.begin(InsertKind::Normal);
        self.set_mode(Mode::Insert);
        self.anchor = anchor;
        self.head = at;
    }

    /// Ends the Insert session and remembers its text for `.`.
    pub(super) fn finish_insert(&mut self) {
        let kind = self.insert_session.kind;
        let text = std::mem::take(&mut self.insert_session.buffer);
        match kind {
            InsertKind::Normal => {
                if !text.is_empty() {
                    self.last_change = Some(Repeat::Insert { text });
                }
            }
            InsertKind::OpenBelow => {
                self.last_change = Some(Repeat::OpenBelow { text });
            }
            InsertKind::OpenAbove => {
                self.last_change = Some(Repeat::OpenAbove { text });
            }
        }
        self.insert_session.clear();
    }

    /// Repeats the last change `count` times.
    pub(super) fn repeat_last(&mut self, count: u32) -> Action {
        match self.last_change.clone() {
            Some(Repeat::Delete { count: deleted }) => {
                self.delete_forward(deleted.saturating_mul(count), None)
            }
            Some(Repeat::Insert { text }) => {
                let at = self.ordered().0;
                self.insert_at(at, &text.repeat(count as usize))
            }
            Some(Repeat::Paste { text, kind, before }) => {
                self.paste_text(&text, kind, before, count)
            }
            Some(Repeat::OpenBelow { text }) => {
                let text = text.repeat(count as usize);
                self.repeat_open_below(&text)
            }
            Some(Repeat::OpenAbove { text }) => {
                let text = text.repeat(count as usize);
                self.repeat_open_above(&text)
            }
            None => Action::None,
        }
    }
}
