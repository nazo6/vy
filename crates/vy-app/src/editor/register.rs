//! Registers used for yank, delete, and paste.

use crate::keymap::Mode;
use vy_core::{Position, Range};

use super::Editor;
use super::action::Action;
use super::operator::Operator;
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
        match self.mode {
            Mode::SelectLine => {
                let first = self.anchor.line.min(self.head.line);
                let last = self.anchor.line.max(self.head.line);
                let range = self.linewise_range(first, last);
                self.apply_operator_range(Operator::Yank, range);
                let start = Position::new(first, 0);
                self.set_mode(Mode::Normal);
                self.select_one_char(start);
                Action::None
            }
            Mode::SelectBlock => {
                let first_line = self.anchor.line.min(self.head.line);
                let last_line = self.anchor.line.max(self.head.line);
                let first_col = self.anchor.character.min(self.head.character);
                let last_col = self.anchor.character.max(self.head.character);

                let mut yanked_lines = Vec::new();
                for line in first_line..=last_line {
                    let length = self.text.line_len_utf16(line as usize).unwrap_or(0);
                    if length > first_col {
                        let end_col = (last_col + 1).min(length);
                        let range = Range::new(
                            Position::new(line, first_col),
                            Position::new(line, end_col),
                        );
                        if let Ok(sliced) = self.text.slice(range) {
                            yanked_lines.push(sliced);
                            continue;
                        }
                    }
                    yanked_lines.push(String::new());
                }
                let stored = yanked_lines.join("\n");
                self.store_register(&stored, RegisterKind::Charwise);
                let start = Position::new(first_line, first_col);
                self.set_mode(Mode::Normal);
                self.select_one_char(start);
                Action::None
            }
            _ => {
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
        }
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
