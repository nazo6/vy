//! Registers used for yank, delete, and paste.

use crate::keymap::Mode;
use vy_core::{Position, Range};

use super::Editor;
use super::action::Action;
use super::repeat::Repeat;

/// How register text is pasted back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum RegisterKind {
    #[default]
    Charwise,
    Linewise,
}

#[derive(Debug, Clone, Default)]
pub(super) struct Register {
    pub(super) text: String,
    pub(super) kind: RegisterKind,
}

/// Maps a register name  to  to an index. Other characters name no register.
pub(super) fn register_index(character: char) -> Option<usize> {
    character
        .is_ascii_lowercase()
        .then(|| (character as u8 - b'a') as usize)
}

impl Editor {
    pub(super) fn yank_selection(&mut self) -> Action {
        let (start, end) = self.ordered();
        if start == end {
            return Action::None;
        }

        let text = self.text.slice(Range::new(start, end)).unwrap_or_default();
        self.store_register(&text, RegisterKind::Charwise);
        self.set_mode(Mode::Normal);
        self.select_one_char(start);
        Action::None
    }

    /// Pastes the selected register text after the cursor, or before with `before`.
    pub(super) fn paste(&mut self, count: u32, before: bool) -> Action {
        let register = self.register().clone();
        if register.text.is_empty() {
            return Action::None;
        }
        self.paste_text(&register.text, register.kind, before, count)
    }

    pub(super) fn paste_text(
        &mut self,
        text: &str,
        kind: RegisterKind,
        before: bool,
        count: u32,
    ) -> Action {
        self.last_change = Some(Repeat::Paste {
            text: text.to_string(),
            kind,
            before,
        });

        match kind {
            RegisterKind::Charwise => {
                let (start, end) = self.ordered();
                let at = if before { start } else { end };
                self.insert_at(at, &text.repeat(count as usize))
            }
            RegisterKind::Linewise => {
                let line = self.current().line;
                if before {
                    let at = Position::new(line, 0);
                    let payload = format!("{text}\n").repeat(count as usize);
                    self.insert_at_with_cursor(at, &payload, at)
                } else if line + 1 < self.text.line_count() as u32 {
                    let at = Position::new(line + 1, 0);
                    let payload = format!("{text}\n").repeat(count as usize);
                    self.insert_at_with_cursor(at, &payload, at)
                } else {
                    let length = self.text.line_len_utf16(line as usize).unwrap_or_default();
                    let at = Position::new(line, length);
                    let payload = format!("\n{text}").repeat(count as usize);
                    let cursor = Position::new(line + 1, 0);
                    self.insert_at_with_cursor(at, &payload, cursor)
                }
            }
        }
    }

    pub(super) fn store_register(&mut self, text: &str, kind: RegisterKind) {
        let register = Register {
            text: text.to_string(),
            kind,
        };
        self.unnamed_register = register.clone();

        if let Some(index) = self.active_register {
            self.named_registers[index] = register;
        }
    }

    pub(super) fn register(&self) -> &Register {
        match self.active_register {
            Some(index) => &self.named_registers[index],
            None => &self.unnamed_register,
        }
    }
}
